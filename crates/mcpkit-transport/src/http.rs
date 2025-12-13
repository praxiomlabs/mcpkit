//! HTTP transport with Server-Sent Events (SSE) streaming.
//!
//! This module provides HTTP-based transport for MCP, supporting the Streamable
//! HTTP transport specification from MCP 2025-06-18.
//!
//! # Features
//!
//! - Standard HTTP POST requests for sending messages
//! - Server-Sent Events (SSE) for receiving streaming responses
//! - Session management with MCP session IDs
//! - Automatic reconnection with Last-Event-ID support
//! - Protocol version header handling
//!
//! # Protocol Reference
//!
//! The Streamable HTTP transport is defined in the MCP specification
//! [2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports).
//!
//! Key protocol requirements:
//! - Client sends JSON-RPC messages via HTTP POST
//! - Accept header must include both `application/json` and `text/event-stream`
//! - Server may respond with JSON or SSE stream
//! - Session ID assigned during initialization and included in subsequent requests
//! - Protocol version header required on all requests
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::http::HttpTransportConfig;
//! use std::time::Duration;
//!
//! // Configure an HTTP transport
//! let config = HttpTransportConfig::new("http://localhost:8080/mcp")
//!     .with_connect_timeout(Duration::from_secs(30))
//!     .with_request_timeout(Duration::from_secs(60))
//!     .with_max_reconnect_attempts(3);
//!
//! assert_eq!(config.base_url, "http://localhost:8080/mcp");
//! assert!(config.auto_reconnect);
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

#[cfg(feature = "http")]
use {
    bytes::Bytes,
    futures::StreamExt,
    reqwest::{
        header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE},
        Client, Response, StatusCode,
    },
};

/// MCP Protocol version for the HTTP transport.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Header name for MCP protocol version.
///
/// Note: HTTP/2 requires lowercase header names. HTTP/1.1 headers are
/// case-insensitive, so lowercase works universally.
pub const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Header name for MCP session ID.
///
/// Note: HTTP/2 requires lowercase header names. HTTP/1.1 headers are
/// case-insensitive, so lowercase works universally.
pub const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Default maximum message size (16 MB).
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Configuration for HTTP transport.
#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    /// Base URL for the MCP endpoint.
    pub base_url: String,
    /// Optional session ID for resuming sessions.
    pub session_id: Option<String>,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Request timeout.
    pub request_timeout: Duration,
    /// Whether to enable automatic SSE reconnection.
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts.
    pub max_reconnect_attempts: u32,
    /// Custom headers to include in requests.
    pub headers: Vec<(String, String)>,
    /// Protocol version to use.
    pub protocol_version: String,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

impl HttpTransportConfig {
    /// Create a new HTTP transport configuration.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            session_id: None,
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(60),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            headers: Vec::new(),
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Set the session ID for resuming a session.
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the connection timeout.
    #[must_use]
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub const fn without_auto_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }

    /// Set maximum reconnection attempts.
    #[must_use]
    pub const fn with_max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    /// Add a custom header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set the protocol version.
    #[must_use]
    pub fn with_protocol_version(mut self, version: impl Into<String>) -> Self {
        self.protocol_version = version.into();
        self
    }
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self::new("http://localhost:8080/mcp")
    }
}

/// HTTP transport state.
#[derive(Debug)]
struct HttpTransportState {
    /// Current session ID.
    session_id: Option<String>,
    /// Queue of received messages.
    message_queue: VecDeque<Message>,
    /// Last event ID for SSE reconnection.
    last_event_id: Option<String>,
    /// Current SSE buffer for parsing (used when http feature is enabled).
    #[cfg_attr(not(feature = "http"), allow(dead_code))]
    sse_buffer: String,
}

/// HTTP transport with SSE streaming support.
///
/// This transport implements the MCP Streamable HTTP transport specification.
/// It sends messages via HTTP POST and receives responses either as direct
/// JSON or via Server-Sent Events (SSE) streaming.
pub struct HttpTransport {
    config: HttpTransportConfig,
    state: AsyncMutex<HttpTransportState>,
    connected: AtomicBool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    #[cfg(feature = "http")]
    client: Client,
}

impl HttpTransport {
    /// Create a new HTTP transport with the given configuration.
    ///
    /// This creates the HTTP client but does not connect to the server.
    /// Connection is established on first send.
    #[cfg(feature = "http")]
    pub fn new(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let session_id = config.session_id.clone();
        Ok(Self {
            config,
            state: AsyncMutex::new(HttpTransportState {
                session_id,
                message_queue: VecDeque::new(),
                last_event_id: None,
                sse_buffer: String::new(),
            }),
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            client,
        })
    }

    /// Create a new HTTP transport without the http feature (stub).
    #[cfg(not(feature = "http"))]
    pub fn new(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let session_id = config.session_id.clone();
        Ok(Self {
            config,
            state: AsyncMutex::new(HttpTransportState {
                session_id,
                message_queue: VecDeque::new(),
                last_event_id: None,
                sse_buffer: String::new(),
            }),
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        })
    }

    /// Connect to the MCP server and establish a session.
    ///
    /// This is a convenience method that creates the transport and
    /// marks it as connected. The actual HTTP connection is made
    /// on the first send operation.
    pub async fn connect(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let transport = Self::new(config)?;
        transport.connected.store(true, Ordering::Release);
        Ok(transport)
    }

    /// Get the current session ID, if any.
    pub async fn session_id(&self) -> Option<String> {
        self.state.lock().await.session_id.clone()
    }

    /// Set the session ID.
    pub async fn set_session_id(&self, session_id: impl Into<String>) {
        self.state.lock().await.session_id = Some(session_id.into());
    }

    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Get the last event ID for SSE resumption.
    pub async fn last_event_id(&self) -> Option<String> {
        self.state.lock().await.last_event_id.clone()
    }

    /// Build headers for requests.
    #[cfg(feature = "http")]
    fn build_headers(&self, session_id: Option<&str>) -> Result<HeaderMap, TransportError> {
        let mut headers = HeaderMap::new();

        // Required headers per MCP spec
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert(
            MCP_PROTOCOL_VERSION_HEADER,
            HeaderValue::from_str(&self.config.protocol_version).map_err(|e| {
                TransportError::Connection {
                    message: format!("Invalid protocol version header: {e}"),
                }
            })?,
        );

        // Session ID if available
        if let Some(sid) = session_id {
            headers.insert(
                MCP_SESSION_ID_HEADER,
                HeaderValue::from_str(sid).map_err(|e| TransportError::Connection {
                    message: format!("Invalid session ID header: {e}"),
                })?,
            );
        }

        // Custom headers
        for (name, value) in &self.config.headers {
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|e| {
                    TransportError::Connection {
                        message: format!("Invalid header name '{name}': {e}"),
                    }
                })?,
                HeaderValue::from_str(value).map_err(|e| TransportError::Connection {
                    message: format!("Invalid header value for '{name}': {e}"),
                })?,
            );
        }

        Ok(headers)
    }

    /// Send a message and handle the response.
    #[cfg(feature = "http")]
    async fn send_post(&self, msg: &Message) -> Result<(), TransportError> {
        let body = serde_json::to_string(msg).map_err(|e| TransportError::Serialization {
            message: format!("Failed to serialize message: {e}"),
        })?;

        // Check message size limit
        if body.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: body.len(),
                max: self.config.max_message_size,
            });
        }

        let session_id = self.state.lock().await.session_id.clone();
        let headers = self.build_headers(session_id.as_deref())?;

        let response = self
            .client
            .post(&self.config.base_url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("HTTP POST failed: {e}"),
            })?;

        self.handle_response(response).await?;
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.connected.store(true, Ordering::Release);

        Ok(())
    }

    /// Handle the HTTP response, which may be JSON or SSE.
    #[cfg(feature = "http")]
    async fn handle_response(&self, response: Response) -> Result<(), TransportError> {
        let status = response.status();

        // Check for session ID in response headers
        if let Some(session_id) = response.headers().get(MCP_SESSION_ID_HEADER) {
            if let Ok(sid) = session_id.to_str() {
                self.state.lock().await.session_id = Some(sid.to_string());
            }
        }

        match status {
            StatusCode::OK => {
                let content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/json");

                if content_type.starts_with("text/event-stream") {
                    // Handle SSE stream
                    self.process_sse_stream(response).await
                } else {
                    // Handle direct JSON response
                    self.process_json_response(response).await
                }
            }
            StatusCode::ACCEPTED => {
                // 202 Accepted - no response body (for notifications)
                Ok(())
            }
            StatusCode::BAD_REQUEST => {
                let body = response.text().await.unwrap_or_default();
                Err(TransportError::Protocol {
                    message: format!("Bad request: {body}"),
                })
            }
            StatusCode::NOT_FOUND => {
                // Session expired
                self.state.lock().await.session_id = None;
                Err(TransportError::Connection {
                    message: "Session expired or not found".to_string(),
                })
            }
            _ => Err(TransportError::Protocol {
                message: format!("Unexpected status code: {status}"),
            }),
        }
    }

    /// Process a direct JSON response.
    #[cfg(feature = "http")]
    async fn process_json_response(&self, response: Response) -> Result<(), TransportError> {
        let body = response
            .text()
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to read response body: {e}"),
            })?;

        if body.is_empty() {
            return Ok(());
        }

        // Check message size limit
        if body.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: body.len(),
                max: self.config.max_message_size,
            });
        }

        let msg: Message =
            serde_json::from_str(&body).map_err(|e| TransportError::Serialization {
                message: format!("Failed to parse response: {e}"),
            })?;

        self.state.lock().await.message_queue.push_back(msg);
        self.messages_received.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Process an SSE stream.
    #[cfg(feature = "http")]
    async fn process_sse_stream(&self, response: Response) -> Result<(), TransportError> {
        let mut stream = response.bytes_stream();
        let mut state = self.state.lock().await;

        while let Some(chunk_result) = stream.next().await {
            let chunk: Bytes = chunk_result.map_err(|e| TransportError::Connection {
                message: format!("SSE stream error: {e}"),
            })?;

            let chunk_str = std::str::from_utf8(&chunk).map_err(|e| TransportError::Protocol {
                message: format!("Invalid UTF-8 in SSE stream: {e}"),
            })?;

            state.sse_buffer.push_str(chunk_str);

            // Process complete events
            self.process_sse_buffer(&mut state)?;
        }

        Ok(())
    }

    /// Process the SSE buffer and extract complete events.
    #[cfg_attr(not(feature = "http"), allow(dead_code))]
    fn process_sse_buffer(&self, state: &mut HttpTransportState) -> Result<(), TransportError> {
        // SSE events are delimited by double newlines
        while let Some(event_end) = state.sse_buffer.find("\n\n") {
            let event_str = state.sse_buffer[..event_end].to_string();
            state.sse_buffer = state.sse_buffer[event_end + 2..].to_string();

            // Parse the SSE event
            let mut event_id = None;
            let mut data_lines = Vec::new();

            for line in event_str.lines() {
                if let Some(id) = line.strip_prefix("id:") {
                    event_id = Some(id.trim().to_string());
                } else if let Some(data) = line.strip_prefix("data:") {
                    data_lines.push(data.trim_start().to_string());
                }
                // Ignore other fields (event:, retry:, etc.) for now
            }

            // Update last event ID
            if let Some(id) = event_id {
                state.last_event_id = Some(id);
            }

            // Join data lines and parse as JSON-RPC message
            if !data_lines.is_empty() {
                let data = data_lines.join("\n");
                if !data.is_empty() {
                    // Check message size limit
                    if data.len() > self.config.max_message_size {
                        return Err(TransportError::MessageTooLarge {
                            size: data.len(),
                            max: self.config.max_message_size,
                        });
                    }

                    match serde_json::from_str::<Message>(&data) {
                        Ok(msg) => {
                            state.message_queue.push_back(msg);
                            self.messages_received.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse SSE data as JSON-RPC: {e}");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Stub for `send_post` when http feature is disabled.
    #[cfg(not(feature = "http"))]
    async fn send_post(&self, _msg: &Message) -> Result<(), TransportError> {
        Err(TransportError::Connection {
            message: "HTTP transport requires the 'http' feature".to_string(),
        })
    }
}

impl Transport for HttpTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        self.send_post(&msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        let mut state = self.state.lock().await;

        // Return queued messages first
        if let Some(msg) = state.message_queue.pop_front() {
            return Ok(Some(msg));
        }

        // If no queued messages and not connected, return None
        if !self.connected.load(Ordering::Acquire) {
            return Ok(None);
        }

        Ok(None)
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::Release);

        #[cfg(feature = "http")]
        {
            // Send DELETE to terminate session if we have a session ID
            let session_id = self.state.lock().await.session_id.clone();
            if let Some(_sid) = session_id {
                let headers = self.build_headers(None)?;
                let _ = self
                    .client
                    .delete(&self.config.base_url)
                    .headers(headers)
                    .send()
                    .await;
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("http").remote_addr(&self.config.base_url)
    }
}

/// HTTP transport listener for server-side HTTP transport.
///
/// This listener provides axum-based server infrastructure for MCP over HTTP.
/// It supports both direct JSON responses and Server-Sent Events (SSE) streaming.
///
/// # Architecture
///
/// The listener creates transports for each incoming session. Sessions are tracked
/// via the `Mcp-Session-Id` header.
///
/// # Security
///
/// For DNS rebinding protection, configure allowed origins:
///
/// ```ignore
/// let listener = HttpTransportListener::new("127.0.0.1:8080")
///     .with_allowed_origin("https://trusted-app.com");
/// ```
///
/// # Example
///
/// ```ignore
/// use mcpkit_transport::http::HttpTransportListener;
///
/// let listener = HttpTransportListener::new("127.0.0.1:8080")
///     .with_session_timeout(Duration::from_secs(3600));
///
/// listener.start().await?;
/// ```
#[cfg(feature = "http")]
pub struct HttpTransportListener {
    /// The bind address.
    bind_addr: String,
    /// Session timeout duration.
    session_timeout: Duration,
    /// Whether the listener is running.
    running: AtomicBool,
    /// Shutdown signal sender.
    shutdown_tx: AsyncMutex<Option<tokio::sync::broadcast::Sender<()>>>,
    /// Allowed origins for DNS rebinding protection.
    /// If empty, origin validation is disabled.
    allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    max_message_size: usize,
}

#[cfg(feature = "http")]
impl HttpTransportListener {
    /// Create a new HTTP transport listener.
    #[must_use]
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            session_timeout: Duration::from_secs(3600), // 1 hour default
            running: AtomicBool::new(false),
            shutdown_tx: AsyncMutex::new(None),
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Set the session timeout.
    #[must_use]
    pub const fn with_session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout = timeout;
        self
    }

    /// Get the session timeout.
    pub const fn session_timeout(&self) -> Duration {
        self.session_timeout
    }

    /// Add an allowed origin for DNS rebinding protection.
    ///
    /// When configured, the server will validate the Origin header on
    /// incoming requests and reject requests from disallowed origins.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let listener = HttpTransportListener::new("0.0.0.0:8080")
    ///     .with_allowed_origin("https://trusted-app.com")
    ///     .with_allowed_origin("https://another-trusted-app.com");
    /// ```
    #[must_use]
    pub fn with_allowed_origin(mut self, origin: impl Into<String>) -> Self {
        self.allowed_origins.push(origin.into());
        self
    }

    /// Set multiple allowed origins at once.
    #[must_use]
    pub fn with_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_origins
            .extend(origins.into_iter().map(Into::into));
        self
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Check if an origin is allowed.
    ///
    /// Returns `true` if:
    /// - No origins are configured (origin validation disabled)
    /// - The origin is in the allowed list
    #[must_use]
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.is_empty() || self.allowed_origins.iter().any(|o| o == origin)
    }

    /// Get the allowed origins.
    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    /// Get the maximum message size.
    pub const fn max_message_size(&self) -> usize {
        self.max_message_size
    }

    /// Create an axum Router with MCP endpoints.
    ///
    /// This creates routes for:
    /// - POST /mcp - Handle incoming JSON-RPC messages
    /// - GET /mcp - SSE stream for server-to-client messages
    /// - DELETE /mcp - Close session
    ///
    /// The router should be integrated with a message handler that processes
    /// the MCP protocol messages.
    ///
    /// # Deprecated
    ///
    /// Use `create_router_with_config` instead for proper origin validation.
    pub fn create_router() -> axum::Router {
        Self::create_router_with_config(HttpServerConfig::default())
    }

    /// Create an axum Router with MCP endpoints and configuration.
    ///
    /// This creates routes for:
    /// - POST /mcp - Handle incoming JSON-RPC messages
    /// - GET /mcp - SSE stream for server-to-client messages
    /// - DELETE /mcp - Close session
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = HttpServerConfig::new()
    ///     .with_allowed_origin("https://trusted-app.com")
    ///     .with_max_message_size(8 * 1024 * 1024);
    /// let router = HttpTransportListener::create_router_with_config(config);
    /// ```
    pub fn create_router_with_config(config: HttpServerConfig) -> axum::Router {
        use axum::{
            routing::{delete, get, post},
            Router,
        };
        use std::sync::Arc;

        Router::new()
            .route("/mcp", post(handle_mcp_post_with_state))
            .route("/mcp", get(handle_mcp_sse_with_state))
            .route("/mcp", delete(handle_mcp_delete_with_state))
            .with_state(Arc::new(config))
    }

    /// Start the HTTP server.
    ///
    /// This starts an axum server listening on the configured bind address.
    pub async fn start(&self) -> Result<(), TransportError> {
        use std::future::IntoFuture;
        use tokio::net::TcpListener;

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let listener =
            TcpListener::bind(&self.bind_addr)
                .await
                .map_err(|e| TransportError::Connection {
                    message: format!("Failed to bind to {}: {}", self.bind_addr, e),
                })?;

        self.running.store(true, Ordering::Release);
        tracing::info!(addr = %self.bind_addr, "HTTP server started");

        // Create server config from listener settings
        let server_config = HttpServerConfig {
            allowed_origins: self.allowed_origins.clone(),
            max_message_size: self.max_message_size,
        };
        let router = Self::create_router_with_config(server_config);

        // Run the server
        tokio::select! {
            result = axum::serve(listener, router).into_future() => {
                if let Err(e) = result {
                    tracing::error!(error = %e, "HTTP server error");
                    return Err(TransportError::Connection {
                        message: format!("Server error: {e}"),
                    });
                }
            }
            _ = shutdown_rx.recv() => {
                tracing::info!("HTTP server shutting down");
            }
        }

        self.running.store(false, Ordering::Release);
        Ok(())
    }

    /// Stop the listener.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Release);
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
    }

    /// Check if the listener is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get the bind address.
    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }
}

/// Server-side configuration for HTTP transport.
///
/// This configuration is used by the axum router to validate requests
/// and enforce security policies.
#[cfg(feature = "http")]
#[derive(Debug, Clone, Default)]
pub struct HttpServerConfig {
    /// Allowed origins for DNS rebinding protection.
    /// If empty, origin validation is disabled.
    pub allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

#[cfg(feature = "http")]
impl HttpServerConfig {
    /// Create a new server configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Add an allowed origin for DNS rebinding protection.
    #[must_use]
    pub fn with_allowed_origin(mut self, origin: impl Into<String>) -> Self {
        self.allowed_origins.push(origin.into());
        self
    }

    /// Set multiple allowed origins at once.
    #[must_use]
    pub fn with_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_origins
            .extend(origins.into_iter().map(Into::into));
        self
    }

    /// Set maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Check if an origin is allowed.
    #[must_use]
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.is_empty() || self.allowed_origins.iter().any(|o| o == origin)
    }
}

/// Validate origin header for DNS rebinding protection.
#[cfg(feature = "http")]
fn validate_origin(
    headers: &axum::http::HeaderMap,
    config: &HttpServerConfig,
) -> Result<(), axum::response::Response> {
    use axum::{http::StatusCode, response::IntoResponse};

    // If no origins configured, skip validation
    if config.allowed_origins.is_empty() {
        return Ok(());
    }

    // Get origin header
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok());

    match origin {
        Some(origin_str) => {
            if config.is_origin_allowed(origin_str) {
                Ok(())
            } else {
                tracing::warn!(origin = %origin_str, "Rejecting request from disallowed origin");
                Err((StatusCode::FORBIDDEN, "Origin not allowed").into_response())
            }
        }
        None => {
            // No origin header - this can happen for same-origin requests
            // or non-browser clients. Allow by default.
            Ok(())
        }
    }
}

/// Handle POST requests with JSON-RPC messages (with state).
#[cfg(feature = "http")]
async fn handle_mcp_post_with_state(
    axum::extract::State(config): axum::extract::State<std::sync::Arc<HttpServerConfig>>,
    headers: axum::http::HeaderMap,
    body: String,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};

    // Validate origin
    if let Err(response) = validate_origin(&headers, &config) {
        return response;
    }

    // Check message size
    if body.len() > config.max_message_size {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Message too large: {} bytes (max {})",
                body.len(),
                config.max_message_size
            ),
        )
            .into_response();
    }

    // Check protocol version
    let version = headers
        .get(MCP_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok());

    if version != Some(MCP_PROTOCOL_VERSION) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Missing or invalid {MCP_PROTOCOL_VERSION_HEADER} header"),
        )
            .into_response();
    }

    // Parse message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response();
        }
    };

    // Get or create session
    let session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map_or_else(
            || uuid::Uuid::new_v4().to_string(),
            std::string::ToString::to_string,
        );

    // For now, just echo back as JSON
    // In a full implementation, this would route to the MCP handler
    let response_headers = [
        (MCP_SESSION_ID_HEADER, session_id.as_str()),
        (MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION),
    ];

    (
        StatusCode::OK,
        response_headers,
        serde_json::to_string(&msg).unwrap_or_default(),
    )
        .into_response()
}

/// Handle GET requests with SSE streaming (with state).
#[cfg(feature = "http")]
async fn handle_mcp_sse_with_state(
    axum::extract::State(config): axum::extract::State<std::sync::Arc<HttpServerConfig>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::{
        http::StatusCode,
        response::{
            sse::{Event, KeepAlive, Sse},
            IntoResponse,
        },
    };
    use futures::stream;
    use std::convert::Infallible;

    // Validate origin
    if let Err(response) = validate_origin(&headers, &config) {
        return response;
    }

    // Check Accept header
    let accept = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !accept.contains("text/event-stream") {
        return (StatusCode::NOT_ACCEPTABLE, "Must accept text/event-stream").into_response();
    }

    // Get session ID
    let session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());

    if session_id.is_none() {
        return (StatusCode::BAD_REQUEST, "Missing session ID").into_response();
    }

    // Create SSE stream
    // In a full implementation, this would pull from the session's outbound queue
    let stream = stream::once(async { Ok::<_, Infallible>(Event::default().data("connected")) });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Handle DELETE requests to close sessions (with state).
#[cfg(feature = "http")]
async fn handle_mcp_delete_with_state(
    axum::extract::State(config): axum::extract::State<std::sync::Arc<HttpServerConfig>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};

    // Validate origin
    if let Err(response) = validate_origin(&headers, &config) {
        return response;
    }

    let session_id = headers
        .get(MCP_SESSION_ID_HEADER)
        .and_then(|v| v.to_str().ok());

    match session_id {
        Some(_id) => {
            // In a full implementation, remove the session
            (StatusCode::OK, "Session closed").into_response()
        }
        None => (StatusCode::BAD_REQUEST, "Missing session ID").into_response(),
    }
}

/// Builder for HTTP transport.
#[derive(Debug, Default)]
pub struct HttpTransportBuilder {
    config: HttpTransportConfig,
}

impl HttpTransportBuilder {
    /// Create a new builder with the given base URL.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            config: HttpTransportConfig::new(base_url),
        }
    }

    /// Set the session ID.
    #[must_use]
    pub fn session_id(mut self, session_id: impl Into<String>) -> Self {
        self.config.session_id = Some(session_id.into());
        self
    }

    /// Set the connection timeout.
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = timeout;
        self
    }

    /// Add a custom header.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.headers.push((name.into(), value.into()));
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub const fn no_auto_reconnect(mut self) -> Self {
        self.config.auto_reconnect = false;
        self
    }

    /// Build the transport.
    pub fn build(self) -> Result<HttpTransport, TransportError> {
        HttpTransport::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = HttpTransportConfig::new("http://example.com/mcp")
            .with_session_id("session-123")
            .with_connect_timeout(Duration::from_secs(10))
            .with_header("X-Custom", "value")
            .with_protocol_version("2025-06-18");

        assert_eq!(config.base_url, "http://example.com/mcp");
        assert_eq!(config.session_id, Some("session-123".to_string()));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.headers.len(), 1);
        assert_eq!(config.protocol_version, "2025-06-18");
    }

    #[test]
    fn test_transport_builder() {
        let transport = HttpTransportBuilder::new("http://example.com/mcp")
            .session_id("test-session")
            .connect_timeout(Duration::from_secs(5))
            .header("Authorization", "Bearer token")
            .build()
            .unwrap();

        assert!(!transport.is_connected());
        assert_eq!(transport.messages_sent(), 0);
        assert_eq!(transport.messages_received(), 0);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_listener_creation() {
        let listener = HttpTransportListener::new("0.0.0.0:8080");
        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
        assert!(!listener.is_running());
    }

    #[tokio::test]
    async fn test_transport_metadata() {
        let transport =
            HttpTransport::new(HttpTransportConfig::new("http://localhost:8080")).unwrap();
        let metadata = transport.metadata();

        assert_eq!(metadata.transport_type, "http");
        assert_eq!(
            metadata.remote_addr,
            Some("http://localhost:8080".to_string())
        );
    }

    #[tokio::test]
    async fn test_session_id_management() {
        let transport =
            HttpTransport::new(HttpTransportConfig::new("http://localhost:8080")).unwrap();

        assert!(transport.session_id().await.is_none());

        transport.set_session_id("test-session-123").await;
        assert_eq!(
            transport.session_id().await,
            Some("test-session-123".to_string())
        );
    }

    #[test]
    fn test_sse_buffer_parsing() {
        let config = HttpTransportConfig::new("http://localhost:8080");
        let transport = HttpTransport::new(config).unwrap();

        // Simulate SSE events in the buffer
        let mut state = HttpTransportState {
            session_id: None,
            message_queue: VecDeque::new(),
            last_event_id: None,
            sse_buffer: String::from(
                "id: evt-001\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n",
            ),
        };

        transport.process_sse_buffer(&mut state).unwrap();

        assert_eq!(state.last_event_id, Some("evt-001".to_string()));
        assert_eq!(state.message_queue.len(), 1);
        assert!(state.sse_buffer.is_empty());
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_server_config_origin_validation_empty_allows_all() {
        let config = HttpServerConfig::new();
        // Empty allowed_origins means validation is disabled (allow all)
        assert!(config.is_origin_allowed("https://anything.com"));
        assert!(config.is_origin_allowed("http://malicious.com"));
        assert!(config.is_origin_allowed(""));
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_server_config_origin_validation_with_allowed_origins() {
        let config = HttpServerConfig::new()
            .with_allowed_origin("https://trusted-app.com")
            .with_allowed_origin("https://another-trusted.com");

        // Allowed origins should pass
        assert!(config.is_origin_allowed("https://trusted-app.com"));
        assert!(config.is_origin_allowed("https://another-trusted.com"));

        // Disallowed origins should fail
        assert!(!config.is_origin_allowed("https://evil.com"));
        assert!(!config.is_origin_allowed("http://trusted-app.com")); // wrong protocol
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_server_config_with_multiple_origins() {
        let origins = vec!["https://app1.com", "https://app2.com", "https://app3.com"];
        let config = HttpServerConfig::new().with_allowed_origins(origins);

        assert!(config.is_origin_allowed("https://app1.com"));
        assert!(config.is_origin_allowed("https://app2.com"));
        assert!(config.is_origin_allowed("https://app3.com"));
        assert!(!config.is_origin_allowed("https://app4.com"));
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_server_config_max_message_size() {
        let config = HttpServerConfig::new().with_max_message_size(1024);

        assert_eq!(config.max_message_size, 1024);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_listener_with_allowed_origins() {
        let listener = HttpTransportListener::new("0.0.0.0:8080")
            .with_allowed_origin("https://trusted-app.com")
            .with_max_message_size(8 * 1024 * 1024);

        assert_eq!(listener.allowed_origins().len(), 1);
        assert!(listener.is_origin_allowed("https://trusted-app.com"));
        assert!(!listener.is_origin_allowed("https://evil.com"));
        assert_eq!(listener.max_message_size(), 8 * 1024 * 1024);
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_http_listener_with_multiple_allowed_origins() {
        let origins = vec!["https://app1.com", "https://app2.com"];
        let listener = HttpTransportListener::new("0.0.0.0:8080").with_allowed_origins(origins);

        assert_eq!(listener.allowed_origins().len(), 2);
        assert!(listener.is_origin_allowed("https://app1.com"));
        assert!(listener.is_origin_allowed("https://app2.com"));
        assert!(!listener.is_origin_allowed("https://app3.com"));
    }
}
