//! gRPC transport implementation.
//!
//! This module provides a gRPC-based transport for MCP communication using
//! bidirectional streaming. It leverages tonic for the gRPC implementation
//! and uses generated protobuf code for message serialization.

use crate::{Transport, TransportMetadata};
use async_lock::Mutex;
use mcpkit_core::protocol::Message;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

// Include the generated protobuf code
pub mod proto {
    tonic::include_proto!("mcp");
}

/// gRPC transport errors.
#[derive(Debug, Error)]
pub enum GrpcError {
    /// Connection error.
    #[error("connection error: {0}")]
    Connection(#[from] tonic::transport::Error),

    /// gRPC status error.
    #[error("gRPC error: {0}")]
    Status(#[from] tonic::Status),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Transport is closed.
    #[error("transport closed")]
    Closed,

    /// Channel send error.
    #[error("channel error: {0}")]
    Channel(String),

    /// URI parse error.
    #[error("invalid URI: {0}")]
    InvalidUri(String),
}

/// Configuration for gRPC transport.
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    /// The endpoint URI (e.g., `http://localhost:50051`).
    pub endpoint: String,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Request timeout.
    pub timeout: Duration,
    /// Enable TLS.
    pub tls: bool,
    /// Custom metadata to include in requests.
    pub metadata: HashMap<String, String>,
}

impl GrpcConfig {
    /// Create a new gRPC configuration with the given endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            connect_timeout: Duration::from_secs(10),
            timeout: Duration::from_secs(30),
            tls: false,
            metadata: HashMap::new(),
        }
    }

    /// Set the connection timeout.
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable TLS.
    #[must_use]
    pub const fn with_tls(mut self) -> Self {
        self.tls = true;
        self
    }

    /// Add custom metadata.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self::new("http://localhost:50051")
    }
}

/// gRPC transport for MCP communication.
///
/// This transport uses gRPC bidirectional streaming to transmit MCP messages.
/// Messages are serialized as JSON and wrapped in protobuf envelopes.
pub struct GrpcTransport {
    /// The gRPC channel (for client-side transports).
    channel: Option<Channel>,
    /// Send channel for outgoing messages (for client-side).
    tx: mpsc::Sender<Message>,
    /// Receive channel for incoming messages.
    rx: Mutex<mpsc::Receiver<Message>>,
    /// Connection state.
    connected: AtomicBool,
    /// Transport metadata.
    metadata: TransportMetadata,
    /// Send channel for outgoing gRPC messages (for server-side transports).
    outgoing_grpc_tx: Option<mpsc::Sender<Result<proto::McpMessage, Status>>>,
}

impl GrpcTransport {
    /// Connect to a gRPC server with the given configuration.
    ///
    /// This establishes a bidirectional streaming connection to the server.
    /// Messages can then be sent with `send()` and received with `recv()`.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails.
    pub async fn connect(config: GrpcConfig) -> Result<Self, GrpcError> {
        let uri: Uri = config
            .endpoint
            .parse()
            .map_err(|e| GrpcError::InvalidUri(format!("{e}")))?;

        let endpoint = Endpoint::from(uri.clone())
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout);

        let endpoint = if config.tls {
            endpoint.tls_config(tonic::transport::ClientTlsConfig::new())?
        } else {
            endpoint
        };

        let channel = endpoint.connect().await?;
        info!(endpoint = %config.endpoint, "Connected to gRPC server");

        // Create a gRPC client and establish the bidirectional stream
        let mut client = proto::mcp_service_client::McpServiceClient::new(channel.clone());

        // Create channels for outgoing messages
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<proto::McpMessage>(100);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Message>(100);

        // Convert the receiver into a stream for the gRPC call
        let outgoing_stream = ReceiverStream::new(outgoing_rx);

        // Call the streaming RPC
        let response = client.stream(outgoing_stream).await?;
        let mut inbound = response.into_inner();

        // Spawn a task to receive incoming messages
        let connected = Arc::new(AtomicBool::new(true));
        let connected_clone = Arc::clone(&connected);
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(msg) => match proto_to_message(&msg) {
                        Ok(message) => {
                            if incoming_tx.send(message).await.is_err() {
                                debug!("Client incoming channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse incoming message: {e}");
                        }
                    },
                    Err(e) => {
                        warn!("Client gRPC stream error: {e}");
                        break;
                    }
                }
            }
            connected_clone.store(false, Ordering::SeqCst);
            debug!("Client incoming stream closed");
        });

        // Create a wrapper sender that converts Message to proto::McpMessage
        let (msg_tx, mut msg_rx) = mpsc::channel::<Message>(100);
        let outgoing_tx_clone = outgoing_tx;
        tokio::spawn(async move {
            while let Some(msg) = msg_rx.recv().await {
                match message_to_proto(&msg) {
                    Ok(proto_msg) => {
                        if outgoing_tx_clone.send(proto_msg).await.is_err() {
                            debug!("Client outgoing channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to serialize message: {e}");
                    }
                }
            }
            debug!("Client outgoing channel closed");
        });

        let metadata = TransportMetadata::new("grpc")
            .remote_addr(config.endpoint)
            .bidirectional(true)
            .connected_now();

        Ok(Self {
            channel: Some(channel),
            tx: msg_tx,
            rx: Mutex::new(incoming_rx),
            connected: AtomicBool::new(true),
            metadata,
            outgoing_grpc_tx: None,
        })
    }

    /// Create a new gRPC transport from an existing channel.
    ///
    /// This is useful for server-side transports where the channel
    /// is created from an incoming connection.
    #[must_use]
    pub fn from_channel(channel: Channel, remote_addr: impl Into<String>) -> Self {
        let (tx, _rx_internal) = mpsc::channel(100);
        let (_tx_internal, rx) = mpsc::channel(100);

        let metadata = TransportMetadata::new("grpc")
            .remote_addr(remote_addr)
            .bidirectional(true)
            .connected_now();

        Self {
            channel: Some(channel),
            tx,
            rx: Mutex::new(rx),
            connected: AtomicBool::new(true),
            metadata,
            outgoing_grpc_tx: None,
        }
    }

    /// Get the underlying gRPC channel (if available).
    ///
    /// Returns `None` for server-side transports that were created from
    /// incoming connections.
    #[must_use]
    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }
}

impl Transport for GrpcTransport {
    type Error = GrpcError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(GrpcError::Closed);
        }

        debug!(?msg, "Sending gRPC message");

        // For server-side transports, use the gRPC output channel
        if let Some(ref grpc_tx) = self.outgoing_grpc_tx {
            let grpc_msg = message_to_proto(&msg)?;
            grpc_tx
                .send(Ok(grpc_msg))
                .await
                .map_err(|e| GrpcError::Channel(e.to_string()))
        } else {
            // For client-side transports, use the internal message channel
            self.tx
                .send(msg)
                .await
                .map_err(|e| GrpcError::Channel(e.to_string()))
        }
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        if !self.connected.load(Ordering::SeqCst) {
            return Ok(None);
        }

        let mut rx = self.rx.lock().await;
        if let Some(msg) = rx.recv().await {
            debug!(?msg, "Received gRPC message");
            Ok(Some(msg))
        } else {
            self.connected.store(false, Ordering::SeqCst);
            Ok(None)
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        info!("Closing gRPC transport");
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn metadata(&self) -> TransportMetadata {
        self.metadata.clone()
    }
}

/// Configuration for gRPC server.
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    /// Bind address (e.g., "0.0.0.0:50051").
    pub addr: String,
    /// Enable TLS.
    pub tls: bool,
    /// Maximum concurrent streams per connection.
    pub max_concurrent_streams: Option<u32>,
    /// TCP keepalive interval.
    pub tcp_keepalive: Option<Duration>,
    /// HTTP/2 keepalive interval.
    pub http2_keepalive_interval: Option<Duration>,
}

impl GrpcServerConfig {
    /// Create a new server configuration with the given bind address.
    #[must_use]
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            tls: false,
            max_concurrent_streams: Some(200),
            tcp_keepalive: Some(Duration::from_secs(60)),
            http2_keepalive_interval: Some(Duration::from_secs(30)),
        }
    }

    /// Enable TLS.
    #[must_use]
    pub const fn with_tls(mut self) -> Self {
        self.tls = true;
        self
    }

    /// Set maximum concurrent streams per connection.
    #[must_use]
    pub const fn max_concurrent_streams(mut self, max: u32) -> Self {
        self.max_concurrent_streams = Some(max);
        self
    }

    /// Set TCP keepalive interval.
    #[must_use]
    pub const fn tcp_keepalive(mut self, interval: Duration) -> Self {
        self.tcp_keepalive = Some(interval);
        self
    }

    /// Set HTTP/2 keepalive interval.
    #[must_use]
    pub const fn http2_keepalive_interval(mut self, interval: Duration) -> Self {
        self.http2_keepalive_interval = Some(interval);
        self
    }
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self::new("0.0.0.0:50051")
    }
}

/// gRPC server for MCP communication.
///
/// This server accepts incoming gRPC connections and creates transports
/// for each connection. Messages are exchanged as JSON-serialized MCP
/// protocol messages over bidirectional streaming.
///
/// # Protocol
///
/// The server uses a simple message envelope:
/// - Each message is a JSON-serialized MCP protocol message
/// - Messages are sent as individual stream items
/// - The stream is bidirectional, allowing both request/response and
///   server-initiated notifications
///
/// # Example
///
/// ```ignore
/// use mcpkit_transport::grpc::{GrpcServer, GrpcServerConfig};
///
/// let config = GrpcServerConfig::new("0.0.0.0:50051");
/// let server = GrpcServer::new(config);
///
/// // The server provides a way to get incoming transports
/// while let Some(transport) = server.accept().await? {
///     tokio::spawn(async move {
///         // Handle the MCP connection
///         handle_connection(transport).await;
///     });
/// }
/// ```
pub struct GrpcServer {
    config: GrpcServerConfig,
    /// Channel for receiving new connections (as transports).
    connection_rx: Mutex<mpsc::Receiver<GrpcTransport>>,
    /// Channel for sending new connections (used by the server task).
    #[allow(dead_code)]
    connection_tx: mpsc::Sender<GrpcTransport>,
    /// Whether the server is running.
    running: AtomicBool,
}

impl GrpcServer {
    /// Create a new gRPC server with the given configuration.
    ///
    /// Note: This creates the server structure but does not start
    /// listening. Call `start()` to begin accepting connections.
    #[must_use]
    pub fn new(config: GrpcServerConfig) -> Self {
        let (connection_tx, connection_rx) = mpsc::channel(100);

        Self {
            config,
            connection_rx: Mutex::new(connection_rx),
            connection_tx,
            running: AtomicBool::new(false),
        }
    }

    /// Get the server configuration.
    #[must_use]
    pub fn config(&self) -> &GrpcServerConfig {
        &self.config
    }

    /// Check if the server is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the bind address.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.config.addr
    }

    /// Accept an incoming connection.
    ///
    /// Returns `None` if the server has been stopped or an error occurred.
    ///
    /// # Note
    ///
    /// This is a placeholder implementation. A full implementation would
    /// use `tonic::Server` to accept connections and convert them to
    /// `GrpcTransport` instances.
    pub async fn accept(&self) -> Option<GrpcTransport> {
        let mut rx = self.connection_rx.lock().await;
        rx.recv().await
    }

    /// Stop the server.
    pub fn stop(&self) {
        info!("Stopping gRPC server");
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Builder for creating gRPC servers with custom configuration.
pub struct GrpcServerBuilder {
    config: GrpcServerConfig,
}

impl GrpcServerBuilder {
    /// Create a new server builder with the given bind address.
    #[must_use]
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            config: GrpcServerConfig::new(addr),
        }
    }

    /// Enable TLS.
    #[must_use]
    pub fn with_tls(mut self) -> Self {
        self.config = self.config.with_tls();
        self
    }

    /// Set maximum concurrent streams per connection.
    #[must_use]
    pub fn max_concurrent_streams(mut self, max: u32) -> Self {
        self.config = self.config.max_concurrent_streams(max);
        self
    }

    /// Set TCP keepalive interval.
    #[must_use]
    pub fn tcp_keepalive(mut self, interval: Duration) -> Self {
        self.config = self.config.tcp_keepalive(interval);
        self
    }

    /// Set HTTP/2 keepalive interval.
    #[must_use]
    pub fn http2_keepalive_interval(mut self, interval: Duration) -> Self {
        self.config = self.config.http2_keepalive_interval(interval);
        self
    }

    /// Build the gRPC server.
    #[must_use]
    pub fn build(self) -> GrpcServer {
        GrpcServer::new(self.config)
    }
}

// =============================================================================
// gRPC Service Implementation
// =============================================================================

// Re-export the generated types and service for external use
#[allow(unused_imports)]
pub use proto::McpMessage;
#[allow(unused_imports)]
pub use proto::mcp_service_client::McpServiceClient;
#[allow(unused_imports)]
pub use proto::mcp_service_server::{McpService, McpServiceServer};

/// Convert an MCP protocol message to a gRPC proto message.
fn message_to_proto(msg: &Message) -> Result<proto::McpMessage, serde_json::Error> {
    let payload = serde_json::to_string(msg)?;
    Ok(proto::McpMessage {
        payload,
        metadata: HashMap::new(),
    })
}

/// Convert a gRPC proto message to an MCP protocol message.
fn proto_to_message(msg: &proto::McpMessage) -> Result<Message, serde_json::Error> {
    serde_json::from_str(&msg.payload)
}

/// Internal service implementation that bridges gRPC streams to MCP transports.
///
/// This implementation is used by `GrpcServer::start()` to create the gRPC service.
/// It handles incoming bidirectional streams and creates `GrpcTransport` instances
/// for each connection.
#[derive(Clone)]
struct McpServiceImpl {
    /// Channel for sending new transports when connections are established.
    connection_tx: mpsc::Sender<GrpcTransport>,
}

#[tonic::async_trait]
impl proto::mcp_service_server::McpService for McpServiceImpl {
    type StreamStream = ReceiverStream<Result<proto::McpMessage, Status>>;

    async fn stream(
        &self,
        request: Request<Streaming<proto::McpMessage>>,
    ) -> Result<Response<Self::StreamStream>, Status> {
        let remote_addr = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        info!(remote = %remote_addr, "New gRPC MCP connection");

        // Create channels for bidirectional communication
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Result<proto::McpMessage, Status>>(100);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Message>(100);

        // Create a transport for this connection
        let transport =
            GrpcTransportInner::new(incoming_rx, outgoing_tx.clone(), remote_addr.clone());

        // Send the transport to the server's accept queue
        if let Err(e) = self.connection_tx.send(transport.into_transport()).await {
            error!(remote = %remote_addr, "Failed to queue transport: {e}");
            return Err(Status::internal("Failed to accept connection"));
        }

        // Spawn a task to handle incoming messages from the client
        let mut inbound = request.into_inner();
        let incoming_tx_clone = incoming_tx;
        tokio::spawn(async move {
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(msg) => match proto_to_message(&msg) {
                        Ok(message) => {
                            if incoming_tx_clone.send(message).await.is_err() {
                                debug!("Incoming channel closed");
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse incoming message: {e}");
                        }
                    },
                    Err(e) => {
                        warn!("gRPC stream error: {e}");
                        break;
                    }
                }
            }
            debug!("Incoming stream closed");
        });

        Ok(Response::new(ReceiverStream::new(outgoing_rx)))
    }
}

/// Internal transport state for gRPC connections.
///
/// This struct holds the state for a server-side gRPC connection before
/// it's converted into a full `GrpcTransport`.
struct GrpcTransportInner {
    /// Receive channel for incoming MCP messages.
    incoming_rx: Mutex<mpsc::Receiver<Message>>,
    /// Send channel for outgoing gRPC messages.
    outgoing_tx: mpsc::Sender<Result<proto::McpMessage, Status>>,
    /// Connection state.
    connected: AtomicBool,
    /// Transport metadata.
    metadata: TransportMetadata,
}

impl GrpcTransportInner {
    fn new(
        incoming_rx: mpsc::Receiver<Message>,
        outgoing_tx: mpsc::Sender<Result<proto::McpMessage, Status>>,
        remote_addr: String,
    ) -> Self {
        Self {
            incoming_rx: Mutex::new(incoming_rx),
            outgoing_tx,
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("grpc")
                .remote_addr(remote_addr)
                .bidirectional(true)
                .connected_now(),
        }
    }

    fn into_transport(self) -> GrpcTransport {
        // For server-side transports, we create a dummy channel since we use
        // the internal channels instead
        let (tx, _rx) = mpsc::channel(1);

        GrpcTransport {
            // Server-side transports don't have a client channel
            channel: None,
            tx,
            rx: self.incoming_rx,
            connected: self.connected,
            metadata: self.metadata,
            outgoing_grpc_tx: Some(self.outgoing_tx),
        }
    }
}

impl GrpcServer {
    /// Start the gRPC server and begin accepting connections.
    ///
    /// This spawns a background task that runs the tonic server. Incoming
    /// connections can be retrieved using the `accept()` method.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to bind to the configured address.
    pub async fn start(self: Arc<Self>) -> Result<(), GrpcError> {
        let addr: SocketAddr = self
            .config
            .addr
            .parse()
            .map_err(|e| GrpcError::InvalidUri(format!("Invalid bind address: {e}")))?;

        info!(addr = %addr, "Starting gRPC MCP server");

        self.running.store(true, Ordering::SeqCst);

        let service = McpServiceImpl {
            connection_tx: self.connection_tx.clone(),
        };

        // Build the tonic server
        let mut builder = Server::builder();

        if let Some(max_streams) = self.config.max_concurrent_streams {
            builder = builder.http2_max_pending_accept_reset_streams(Some(max_streams as usize));
        }

        // Create the gRPC service using the generated server wrapper
        let svc = proto::mcp_service_server::McpServiceServer::new(service);

        let server = Arc::clone(&self);

        // Spawn the server task
        tokio::spawn(async move {
            let result = builder
                .add_service(svc)
                .serve_with_shutdown(addr, async move {
                    // Wait until running is set to false
                    loop {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if !server.running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                })
                .await;

            if let Err(e) = result {
                error!("gRPC server error: {e}");
            }
            info!("gRPC server stopped");
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grpc_config_builder() {
        let config = GrpcConfig::new("http://localhost:50051")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .with_tls()
            .with_metadata("x-custom", "value");

        assert_eq!(config.endpoint, "http://localhost:50051");
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
        assert!(config.tls);
        assert_eq!(config.metadata.get("x-custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_grpc_config_default() {
        let config = GrpcConfig::default();
        assert_eq!(config.endpoint, "http://localhost:50051");
        assert!(!config.tls);
    }

    #[test]
    fn test_grpc_config_multiple_metadata() {
        let config = GrpcConfig::new("http://localhost:50051")
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2")
            .with_metadata("key3", "value3");

        assert_eq!(config.metadata.len(), 3);
        assert_eq!(config.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(config.metadata.get("key2"), Some(&"value2".to_string()));
        assert_eq!(config.metadata.get("key3"), Some(&"value3".to_string()));
    }

    #[test]
    fn test_grpc_config_timeouts() {
        let config = GrpcConfig::new("http://localhost:50051")
            .connect_timeout(Duration::from_millis(500))
            .timeout(Duration::from_secs(120));

        assert_eq!(config.connect_timeout, Duration::from_millis(500));
        assert_eq!(config.timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_grpc_server_builder() {
        let server = GrpcServerBuilder::new("0.0.0.0:50051")
            .with_tls()
            .max_concurrent_streams(100)
            .tcp_keepalive(Duration::from_secs(30))
            .http2_keepalive_interval(Duration::from_secs(15))
            .build();

        assert_eq!(server.config().addr, "0.0.0.0:50051");
        assert!(server.config().tls);
        assert_eq!(server.config().max_concurrent_streams, Some(100));
        assert_eq!(server.config().tcp_keepalive, Some(Duration::from_secs(30)));
        assert_eq!(
            server.config().http2_keepalive_interval,
            Some(Duration::from_secs(15))
        );
    }

    #[test]
    fn test_grpc_server_config() {
        let config = GrpcServerConfig::new("0.0.0.0:50051")
            .with_tls()
            .max_concurrent_streams(50)
            .tcp_keepalive(Duration::from_secs(120))
            .http2_keepalive_interval(Duration::from_secs(60));

        assert_eq!(config.addr, "0.0.0.0:50051");
        assert!(config.tls);
        assert_eq!(config.max_concurrent_streams, Some(50));
        assert_eq!(config.tcp_keepalive, Some(Duration::from_secs(120)));
        assert_eq!(
            config.http2_keepalive_interval,
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn test_grpc_server_config_default() {
        let config = GrpcServerConfig::default();
        assert_eq!(config.addr, "0.0.0.0:50051");
        assert!(!config.tls);
        assert!(config.max_concurrent_streams.is_some());
        assert!(config.tcp_keepalive.is_some());
        assert!(config.http2_keepalive_interval.is_some());
    }

    #[test]
    fn test_grpc_server_creation() {
        let config = GrpcServerConfig::new("0.0.0.0:50051");
        let server = GrpcServer::new(config);

        assert_eq!(server.addr(), "0.0.0.0:50051");
        assert!(!server.is_running());
    }

    #[test]
    fn test_grpc_server_stop() {
        let config = GrpcServerConfig::new("0.0.0.0:50051");
        let server = GrpcServer::new(config);

        assert!(!server.is_running());
        server.stop();
        assert!(!server.is_running());
    }

    #[test]
    fn test_grpc_error_display() {
        let closed_err = GrpcError::Closed;
        assert_eq!(format!("{closed_err}"), "transport closed");

        let channel_err = GrpcError::Channel("send failed".to_string());
        assert_eq!(format!("{channel_err}"), "channel error: send failed");

        let uri_err = GrpcError::InvalidUri("bad uri".to_string());
        assert_eq!(format!("{uri_err}"), "invalid URI: bad uri");
    }

    #[test]
    fn test_grpc_config_clone() {
        let config = GrpcConfig::new("http://localhost:50051")
            .with_tls()
            .with_metadata("key", "value");

        let cloned = config.clone();
        assert_eq!(cloned.endpoint, config.endpoint);
        assert_eq!(cloned.tls, config.tls);
        assert_eq!(cloned.metadata, config.metadata);
    }

    #[test]
    fn test_grpc_config_debug() {
        let config = GrpcConfig::new("http://localhost:50051");
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("GrpcConfig"));
        assert!(debug_str.contains("localhost:50051"));
    }
}
