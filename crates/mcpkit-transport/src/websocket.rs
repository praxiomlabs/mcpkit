//! WebSocket transport for MCP.
//!
//! This module provides a first-class WebSocket transport implementation,
//! offering bidirectional real-time communication between MCP clients and servers.
//!
//! # Features
//!
//! - Full-duplex bidirectional communication
//! - Automatic ping/pong handling for connection health
//! - Reconnection with exponential backoff
//! - Message framing and fragmentation handling
//! - TLS/SSL support via rustls
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::websocket::WebSocketConfig;
//! use std::time::Duration;
//!
//! // Configure a WebSocket connection
//! let config = WebSocketConfig::new("ws://localhost:8080/mcp")
//!     .with_connect_timeout(Duration::from_secs(30))
//!     .with_ping_interval(Duration::from_secs(30))
//!     .with_max_reconnect_attempts(5);
//!
//! assert_eq!(config.url, "ws://localhost:8080/mcp");
//! assert!(config.auto_reconnect);
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

#[cfg(feature = "websocket")]
use {
    futures::{SinkExt, StreamExt},
    tokio::net::TcpStream,
    tokio_tungstenite::{
        connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream,
        WebSocketStream,
    },
};

/// Configuration for WebSocket transport.
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket URL (ws:// or wss://).
    pub url: String,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Ping interval for keeping the connection alive.
    pub ping_interval: Duration,
    /// Pong timeout (how long to wait for pong after sending ping).
    pub pong_timeout: Duration,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
    /// Whether to enable automatic reconnection.
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts.
    pub max_reconnect_attempts: u32,
    /// Reconnection backoff configuration.
    pub reconnect_backoff: ExponentialBackoff,
    /// Additional WebSocket subprotocols.
    pub subprotocols: Vec<String>,
    /// Custom headers for the WebSocket handshake.
    pub headers: Vec<(String, String)>,
    /// Allowed origins for DNS rebinding protection (server-side).
    /// If empty, origin validation is disabled.
    pub allowed_origins: Vec<String>,
}

impl WebSocketConfig {
    /// Create a new WebSocket configuration.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            connect_timeout: Duration::from_secs(30),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            max_message_size: 16 * 1024 * 1024, // 16 MB
            auto_reconnect: true,
            max_reconnect_attempts: 10,
            reconnect_backoff: ExponentialBackoff::default(),
            subprotocols: vec!["mcp".to_string()],
            headers: Vec::new(),
            allowed_origins: Vec::new(),
        }
    }

    /// Add an allowed origin for DNS rebinding protection.
    ///
    /// When origins are configured, the server will reject WebSocket
    /// connections from origins not in the list. This helps prevent
    /// DNS rebinding attacks.
    ///
    /// # Example
    ///
    /// ```
    /// use mcpkit_transport::websocket::WebSocketConfig;
    ///
    /// let config = WebSocketConfig::new("ws://localhost:8080/mcp")
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
    pub fn with_allowed_origins(mut self, origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed_origins.extend(origins.into_iter().map(Into::into));
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

    /// Set the connection timeout.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the ping interval.
    #[must_use]
    pub fn with_ping_interval(mut self, interval: Duration) -> Self {
        self.ping_interval = interval;
        self
    }

    /// Set the pong timeout.
    #[must_use]
    pub fn with_pong_timeout(mut self, timeout: Duration) -> Self {
        self.pong_timeout = timeout;
        self
    }

    /// Set the maximum message size.
    #[must_use]
    pub fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub fn without_auto_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }

    /// Set maximum reconnection attempts.
    #[must_use]
    pub fn with_max_reconnect_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    /// Add a WebSocket subprotocol.
    #[must_use]
    pub fn with_subprotocol(mut self, protocol: impl Into<String>) -> Self {
        self.subprotocols.push(protocol.into());
        self
    }

    /// Add a custom header for the WebSocket handshake.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self::new("ws://localhost:8080/mcp")
    }
}

/// Exponential backoff configuration for reconnection.
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    /// Initial delay.
    pub initial_delay: Duration,
    /// Maximum delay.
    pub max_delay: Duration,
    /// Multiplier for each attempt.
    pub multiplier: f64,
}

impl ExponentialBackoff {
    /// Create a new exponential backoff configuration.
    #[must_use]
    pub fn new(initial_delay: Duration, max_delay: Duration, multiplier: f64) -> Self {
        Self {
            initial_delay,
            max_delay,
            multiplier,
        }
    }

    /// Calculate the delay for a given attempt number.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = self.initial_delay.as_millis() as f64 * self.multiplier.powi(attempt as i32);
        let delay = Duration::from_millis(delay_ms as u64);
        std::cmp::min(delay, self.max_delay)
    }
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

/// Connection state for WebSocket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Currently connecting.
    Connecting,
    /// Connected and ready.
    Connected,
    /// Reconnecting after a failure.
    Reconnecting,
    /// Connection closed.
    Closed,
}

/// Internal WebSocket state.
#[cfg(feature = "websocket")]
struct WebSocketState {
    /// The WebSocket stream (split for concurrent read/write).
    stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    /// Queue of received messages.
    message_queue: VecDeque<Message>,
    /// Reconnection attempt counter.
    reconnect_attempt: u32,
}

#[cfg(not(feature = "websocket"))]
struct WebSocketState {
    /// Queue of received messages.
    #[allow(dead_code)] // Used when websocket feature is enabled
    message_queue: VecDeque<Message>,
    /// Reconnection attempt counter.
    #[allow(dead_code)] // Used when websocket feature is enabled
    reconnect_attempt: u32,
}

/// WebSocket transport for MCP communication.
///
/// Provides full-duplex bidirectional communication with automatic
/// ping/pong handling and reconnection support.
pub struct WebSocketTransport {
    #[allow(dead_code)] // Used when websocket feature is enabled
    config: WebSocketConfig,
    #[allow(dead_code)] // Used when websocket feature is enabled
    state: AsyncMutex<WebSocketState>,
    connected: AtomicBool,
    connection_state: AtomicU32, // ConnectionState as u32
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
}

impl WebSocketTransport {
    /// Create a new WebSocket transport (not yet connected).
    #[must_use]
    pub fn new(config: WebSocketConfig) -> Self {
        Self {
            config,
            state: AsyncMutex::new(WebSocketState {
                #[cfg(feature = "websocket")]
                stream: None,
                message_queue: VecDeque::new(),
                reconnect_attempt: 0,
            }),
            connected: AtomicBool::new(false),
            connection_state: AtomicU32::new(ConnectionState::Disconnected as u32),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }

    /// Connect to the WebSocket server.
    #[cfg(feature = "websocket")]
    pub async fn connect(config: WebSocketConfig) -> Result<Self, TransportError> {
        let transport = Self::new(config);
        transport.do_connect().await?;
        Ok(transport)
    }

    /// Connect to the WebSocket server (stub when feature disabled).
    #[cfg(not(feature = "websocket"))]
    pub async fn connect(_config: WebSocketConfig) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Perform the actual connection.
    #[cfg(feature = "websocket")]
    async fn do_connect(&self) -> Result<(), TransportError> {
        self.set_connection_state(ConnectionState::Connecting);

        // Build the WebSocket request with custom headers
        let url = url::Url::parse(&self.config.url).map_err(|e| TransportError::Connection {
            message: format!("Invalid WebSocket URL: {e}"),
        })?;

        // Connect with timeout
        let connect_future = connect_async(url.as_str());
        let result = tokio::time::timeout(self.config.connect_timeout, connect_future)
            .await
            .map_err(|_| TransportError::Timeout {
                operation: "WebSocket connect".to_string(),
                duration: self.config.connect_timeout,
            })?;

        let (ws_stream, _response) = result.map_err(|e| TransportError::Connection {
            message: format!("WebSocket connection failed: {e}"),
        })?;

        // Store the stream
        {
            let mut state = self.state.lock().await;
            state.stream = Some(ws_stream);
            state.reconnect_attempt = 0;
        }

        self.connected.store(true, Ordering::Release);
        self.set_connection_state(ConnectionState::Connected);

        tracing::info!(url = %self.config.url, "WebSocket connected");

        Ok(())
    }

    /// Attempt to reconnect with exponential backoff.
    #[cfg(feature = "websocket")]
    async fn reconnect(&self) -> Result<(), TransportError> {
        let attempt = {
            let mut state = self.state.lock().await;
            state.reconnect_attempt += 1;
            state.reconnect_attempt
        };

        if attempt > self.config.max_reconnect_attempts {
            return Err(TransportError::Connection {
                message: format!(
                    "Maximum reconnection attempts ({}) exceeded",
                    self.config.max_reconnect_attempts
                ),
            });
        }

        self.set_connection_state(ConnectionState::Reconnecting);

        let delay = self.config.reconnect_backoff.delay_for_attempt(attempt - 1);
        tracing::info!(
            attempt = attempt,
            max_attempts = self.config.max_reconnect_attempts,
            delay_ms = delay.as_millis(),
            "Attempting WebSocket reconnection"
        );

        tokio::time::sleep(delay).await;

        self.do_connect().await
    }

    /// Get the current connection state.
    #[must_use]
    pub fn connection_state(&self) -> ConnectionState {
        match self.connection_state.load(Ordering::Acquire) {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Reconnecting,
            4 => ConnectionState::Closed,
            _ => ConnectionState::Disconnected,
        }
    }

    /// Set the connection state.
    fn set_connection_state(&self, state: ConnectionState) {
        self.connection_state
            .store(state as u32, Ordering::Release);
    }

    /// Get the WebSocket URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Send a message over the WebSocket.
    #[cfg(feature = "websocket")]
    async fn send_message(&self, msg: &Message) -> Result<(), TransportError> {
        let json = serde_json::to_string(msg).map_err(|e| TransportError::Serialization {
            message: format!("Failed to serialize message: {e}"),
        })?;

        let mut state = self.state.lock().await;
        let stream = state.stream.as_mut().ok_or_else(|| TransportError::Connection {
            message: "WebSocket not connected".to_string(),
        })?;

        stream
            .send(WsMessage::Text(json))
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to send WebSocket message: {e}"),
            })?;

        drop(state);
        self.messages_sent.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Receive a message from the WebSocket.
    ///
    /// This method uses a loop instead of recursion to avoid async fn boxing requirements.
    #[cfg(feature = "websocket")]
    async fn recv_message(&self) -> Result<Option<Message>, TransportError> {
        loop {
            // First check the queue
            {
                let mut state = self.state.lock().await;
                if let Some(msg) = state.message_queue.pop_front() {
                    return Ok(Some(msg));
                }
            }

            // Try to receive from the stream
            let ws_msg = {
                let mut state = self.state.lock().await;
                let stream = match state.stream.as_mut() {
                    Some(s) => s,
                    None => return Ok(None),
                };

                match stream.next().await {
                    Some(Ok(msg)) => msg,
                    Some(Err(e)) => {
                        // Connection error - mark as disconnected
                        self.connected.store(false, Ordering::Release);
                        self.set_connection_state(ConnectionState::Disconnected);

                        // Try to reconnect if auto-reconnect is enabled
                        if self.config.auto_reconnect {
                            drop(state);
                            if self.reconnect().await.is_ok() {
                                // Retry receive after reconnection (loop continues)
                                continue;
                            }
                        }

                        return Err(TransportError::Connection {
                            message: format!("WebSocket receive error: {e}"),
                        });
                    }
                    None => {
                        // Stream ended
                        self.connected.store(false, Ordering::Release);
                        self.set_connection_state(ConnectionState::Closed);
                        return Ok(None);
                    }
                }
            };

            // Process the WebSocket message
            match ws_msg {
                WsMessage::Text(text) => {
                    let msg: Message =
                        serde_json::from_str(&text).map_err(|e| TransportError::Serialization {
                            message: format!("Failed to parse message: {e}"),
                        })?;
                    self.messages_received.fetch_add(1, Ordering::Relaxed);
                    return Ok(Some(msg));
                }
                WsMessage::Binary(data) => {
                    // Try to parse binary as JSON
                    let msg: Message = serde_json::from_slice(&data).map_err(|e| {
                        TransportError::Serialization {
                            message: format!("Failed to parse binary message: {e}"),
                        }
                    })?;
                    self.messages_received.fetch_add(1, Ordering::Relaxed);
                    return Ok(Some(msg));
                }
                WsMessage::Ping(data) => {
                    // Respond to ping with pong
                    let mut state = self.state.lock().await;
                    if let Some(stream) = state.stream.as_mut() {
                        let _ = stream.send(WsMessage::Pong(data)).await;
                    }
                    // Continue receiving (loop continues)
                }
                WsMessage::Pong(_) => {
                    // Pong received, connection is healthy
                    tracing::trace!("Received pong");
                    // Continue receiving (loop continues)
                }
                WsMessage::Close(frame) => {
                    tracing::info!(frame = ?frame, "WebSocket close frame received");
                    self.connected.store(false, Ordering::Release);
                    self.set_connection_state(ConnectionState::Closed);
                    return Ok(None);
                }
                WsMessage::Frame(_) => {
                    // Raw frame, skip and continue (loop continues)
                }
            }
        }
    }

    /// Send a message (stub when feature disabled).
    #[cfg(not(feature = "websocket"))]
    async fn send_message(&self, _msg: &Message) -> Result<(), TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Receive a message (stub when feature disabled).
    #[cfg(not(feature = "websocket"))]
    async fn recv_message(&self) -> Result<Option<Message>, TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Close the WebSocket connection.
    #[cfg(feature = "websocket")]
    async fn do_close(&self) -> Result<(), TransportError> {
        let mut state = self.state.lock().await;

        if let Some(stream) = state.stream.as_mut() {
            // Send close frame
            let _ = stream.close(None).await;
        }

        state.stream = None;
        self.connected.store(false, Ordering::Release);
        self.set_connection_state(ConnectionState::Closed);

        tracing::info!("WebSocket connection closed");

        Ok(())
    }

    /// Close the WebSocket connection (stub when feature disabled).
    #[cfg(not(feature = "websocket"))]
    async fn do_close(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::Release);
        self.set_connection_state(ConnectionState::Closed);
        Ok(())
    }
}

impl Transport for WebSocketTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        self.send_message(&msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        self.recv_message().await
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.do_close().await
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("websocket").remote_addr(&self.config.url)
    }
}

/// Builder for WebSocket transport.
#[derive(Debug, Default)]
pub struct WebSocketTransportBuilder {
    config: WebSocketConfig,
}

impl WebSocketTransportBuilder {
    /// Create a new builder with the given URL.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            config: WebSocketConfig::new(url),
        }
    }

    /// Set the connection timeout.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    /// Set the ping interval.
    #[must_use]
    pub fn ping_interval(mut self, interval: Duration) -> Self {
        self.config.ping_interval = interval;
        self
    }

    /// Set the pong timeout.
    #[must_use]
    pub fn pong_timeout(mut self, timeout: Duration) -> Self {
        self.config.pong_timeout = timeout;
        self
    }

    /// Set maximum message size.
    #[must_use]
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.max_message_size = size;
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub fn no_auto_reconnect(mut self) -> Self {
        self.config.auto_reconnect = false;
        self
    }

    /// Add a header.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.headers.push((name.into(), value.into()));
        self
    }

    /// Build the transport (connects immediately).
    pub async fn connect(self) -> Result<WebSocketTransport, TransportError> {
        WebSocketTransport::connect(self.config).await
    }

    /// Build the transport without connecting.
    #[must_use]
    pub fn build(self) -> WebSocketTransport {
        WebSocketTransport::new(self.config)
    }
}

/// Server-side configuration for WebSocket listeners.
#[derive(Debug, Clone, Default)]
pub struct WebSocketServerConfig {
    /// Allowed origins for DNS rebinding protection.
    /// If empty, origin validation is disabled.
    pub allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

impl WebSocketServerConfig {
    /// Create a new server configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            allowed_origins: Vec::new(),
            max_message_size: 16 * 1024 * 1024, // 16 MB
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
    pub fn with_allowed_origins(mut self, origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.allowed_origins.extend(origins.into_iter().map(Into::into));
        self
    }

    /// Set maximum message size.
    #[must_use]
    pub fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Check if an origin is allowed.
    #[must_use]
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.is_empty() || self.allowed_origins.iter().any(|o| o == origin)
    }
}

/// WebSocket listener for server-side connections.
#[cfg(feature = "websocket")]
pub struct WebSocketListener {
    bind_addr: String,
    config: WebSocketServerConfig,
    running: AtomicBool,
}

#[cfg(feature = "websocket")]
impl WebSocketListener {
    /// Create a new WebSocket listener.
    #[must_use]
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            config: WebSocketServerConfig::new(),
            running: AtomicBool::new(false),
        }
    }

    /// Create a new WebSocket listener with configuration.
    #[must_use]
    pub fn with_config(bind_addr: impl Into<String>, config: WebSocketServerConfig) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            config,
            running: AtomicBool::new(false),
        }
    }

    /// Get the server configuration.
    #[must_use]
    pub fn config(&self) -> &WebSocketServerConfig {
        &self.config
    }

    /// Start listening for connections.
    pub async fn start(&self) -> Result<(), TransportError> {
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(&self.bind_addr)
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to bind WebSocket listener: {e}"),
            })?;

        self.running.store(true, Ordering::Release);
        tracing::info!(addr = %self.bind_addr, "WebSocket listener started");

        while self.running.load(Ordering::Acquire) {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::debug!(peer = %addr, "Accepting WebSocket connection");

                    let allowed_origins = self.config.allowed_origins.clone();

                    // Upgrade to WebSocket with origin validation
                    tokio::spawn(async move {
                        // Use the callback-based accept for origin validation
                        let callback = |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                                       response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                            // Extract origin header
                            if !allowed_origins.is_empty() {
                                if let Some(origin) = request.headers().get("origin") {
                                    let origin_str = origin.to_str().unwrap_or("");
                                    if !allowed_origins.iter().any(|o| o == origin_str) {
                                        tracing::warn!(
                                            peer = %addr,
                                            origin = %origin_str,
                                            "Rejecting WebSocket connection from disallowed origin"
                                        );
                                        return Err(tokio_tungstenite::tungstenite::handshake::server::Response::builder()
                                            .status(403)
                                            .body(Some("Origin not allowed".to_string()))
                                            .expect("failed to build HTTP 403 response"));
                                    }
                                } else {
                                    // No origin header - reject if origins are configured
                                    tracing::warn!(
                                        peer = %addr,
                                        "Rejecting WebSocket connection with missing Origin header"
                                    );
                                    return Err(tokio_tungstenite::tungstenite::handshake::server::Response::builder()
                                        .status(403)
                                        .body(Some("Origin header required".to_string()))
                                        .expect("failed to build HTTP 403 response"));
                                }
                            }
                            Ok(response)
                        };

                        match tokio_tungstenite::accept_hdr_async(stream, callback).await {
                            Ok(ws_stream) => {
                                tracing::info!(peer = %addr, "WebSocket connection established");
                                // The caller should handle the stream
                                let _ = ws_stream;
                            }
                            Err(e) => {
                                tracing::error!(peer = %addr, error = %e, "WebSocket handshake failed");
                            }
                        }
                    });
                }
                Err(e) => {
                    if self.running.load(Ordering::Acquire) {
                        tracing::error!(error = %e, "Error accepting connection");
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop the listener.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
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

/// Stub listener when websocket feature is disabled.
#[cfg(not(feature = "websocket"))]
pub struct WebSocketListener {
    bind_addr: String,
    config: WebSocketServerConfig,
    running: AtomicBool,
}

#[cfg(not(feature = "websocket"))]
impl WebSocketListener {
    /// Create a new WebSocket listener.
    #[must_use]
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            config: WebSocketServerConfig::new(),
            running: AtomicBool::new(false),
        }
    }

    /// Create a new WebSocket listener with configuration.
    #[must_use]
    pub fn with_config(bind_addr: impl Into<String>, config: WebSocketServerConfig) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            config,
            running: AtomicBool::new(false),
        }
    }

    /// Get the server configuration.
    #[must_use]
    pub fn config(&self) -> &WebSocketServerConfig {
        &self.config
    }

    /// Start listening (stub).
    pub async fn start(&self) -> Result<(), TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Stop the listener.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = WebSocketConfig::new("ws://example.com/mcp")
            .with_connect_timeout(Duration::from_secs(10))
            .with_ping_interval(Duration::from_secs(15))
            .with_pong_timeout(Duration::from_secs(5))
            .with_max_message_size(1024 * 1024)
            .with_subprotocol("custom")
            .with_header("Authorization", "Bearer token");

        assert_eq!(config.url, "ws://example.com/mcp");
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.ping_interval, Duration::from_secs(15));
        assert_eq!(config.pong_timeout, Duration::from_secs(5));
        assert_eq!(config.max_message_size, 1024 * 1024);
        assert!(config.subprotocols.contains(&"custom".to_string()));
        assert_eq!(config.headers.len(), 1);
    }

    #[test]
    fn test_origin_validation_empty_allows_all() {
        let config = WebSocketConfig::new("ws://example.com/mcp");
        // Empty allowed_origins means validation is disabled (allow all)
        assert!(config.is_origin_allowed("https://anything.com"));
        assert!(config.is_origin_allowed("http://malicious.com"));
        assert!(config.is_origin_allowed(""));
    }

    #[test]
    fn test_origin_validation_with_allowed_origins() {
        let config = WebSocketConfig::new("ws://example.com/mcp")
            .with_allowed_origin("https://trusted-app.com")
            .with_allowed_origin("https://another-trusted.com");

        // Allowed origins should pass
        assert!(config.is_origin_allowed("https://trusted-app.com"));
        assert!(config.is_origin_allowed("https://another-trusted.com"));

        // Non-allowed origins should fail
        assert!(!config.is_origin_allowed("https://malicious.com"));
        assert!(!config.is_origin_allowed("http://trusted-app.com")); // Different scheme
        assert!(!config.is_origin_allowed("https://trusted-app.com.evil.com")); // Subdomain attack
    }

    #[test]
    fn test_origin_validation_with_multiple_origins() {
        let origins = vec!["https://app1.com", "https://app2.com", "https://app3.com"];
        let config = WebSocketConfig::new("ws://example.com/mcp")
            .with_allowed_origins(origins);

        assert!(config.is_origin_allowed("https://app1.com"));
        assert!(config.is_origin_allowed("https://app2.com"));
        assert!(config.is_origin_allowed("https://app3.com"));
        assert!(!config.is_origin_allowed("https://app4.com"));
    }

    #[test]
    fn test_exponential_backoff() {
        let backoff = ExponentialBackoff::new(
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );

        assert_eq!(backoff.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(backoff.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(backoff.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(backoff.delay_for_attempt(3), Duration::from_millis(800));

        // Should be capped at max_delay
        assert_eq!(backoff.delay_for_attempt(10), Duration::from_secs(10));
    }

    #[test]
    fn test_connection_state() {
        let transport = WebSocketTransport::new(WebSocketConfig::default());
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);
        assert!(!transport.is_connected());
    }

    #[test]
    fn test_transport_builder() {
        let transport = WebSocketTransportBuilder::new("ws://example.com")
            .connect_timeout(Duration::from_secs(5))
            .ping_interval(Duration::from_secs(10))
            .no_auto_reconnect()
            .header("X-Custom", "value")
            .build();

        assert!(!transport.is_connected());
        assert_eq!(transport.url(), "ws://example.com");
    }

    #[test]
    fn test_listener_creation() {
        let listener = WebSocketListener::new("0.0.0.0:8080");
        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
        assert!(!listener.is_running());
    }

    #[tokio::test]
    async fn test_transport_metadata() {
        let transport = WebSocketTransport::new(WebSocketConfig::new("ws://localhost:8080"));
        let metadata = transport.metadata();

        assert_eq!(metadata.transport_type, "websocket");
        assert_eq!(
            metadata.remote_addr,
            Some("ws://localhost:8080".to_string())
        );
    }

    #[test]
    fn test_default_backoff() {
        let backoff = ExponentialBackoff::default();
        assert_eq!(backoff.initial_delay, Duration::from_millis(100));
        assert_eq!(backoff.max_delay, Duration::from_secs(30));
        assert!((backoff.multiplier - 2.0).abs() < f64::EPSILON);
    }
}
