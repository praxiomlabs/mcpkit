//! WebSocket transport client implementation.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use mcpkit_core::protocol::Message;

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};

use super::config::{ConnectionState, WebSocketConfig};

#[cfg(feature = "websocket")]
use {
    futures::{SinkExt, StreamExt},
    tokio::net::TcpStream,
    tokio_tungstenite::{
        MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message as WsMessage,
    },
};

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
    pub const fn new(config: WebSocketConfig) -> Self {
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
        self.connection_state.store(state as u32, Ordering::Release);
    }

    /// Get the WebSocket URL.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Get the number of messages sent.
    #[must_use]
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    #[must_use]
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
        let stream = state
            .stream
            .as_mut()
            .ok_or_else(|| TransportError::Connection {
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
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    /// Set the ping interval.
    #[must_use]
    pub const fn ping_interval(mut self, interval: Duration) -> Self {
        self.config.ping_interval = interval;
        self
    }

    /// Set the pong timeout.
    #[must_use]
    pub const fn pong_timeout(mut self, timeout: Duration) -> Self {
        self.config.pong_timeout = timeout;
        self
    }

    /// Set maximum message size.
    #[must_use]
    pub const fn max_message_size(mut self, size: usize) -> Self {
        self.config.max_message_size = size;
        self
    }

    /// Disable automatic reconnection.
    #[must_use]
    pub const fn no_auto_reconnect(mut self) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_initial_message_counters() {
        let transport = WebSocketTransport::new(WebSocketConfig::default());
        assert_eq!(transport.messages_sent(), 0);
        assert_eq!(transport.messages_received(), 0);
    }

    #[test]
    fn test_connection_state_enum_values() {
        // Test all ConnectionState variants are correctly mapped
        let transport = WebSocketTransport::new(WebSocketConfig::default());

        // Set each state and verify it can be retrieved correctly
        transport.set_connection_state(ConnectionState::Disconnected);
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);

        transport.set_connection_state(ConnectionState::Connecting);
        assert_eq!(transport.connection_state(), ConnectionState::Connecting);

        transport.set_connection_state(ConnectionState::Connected);
        assert_eq!(transport.connection_state(), ConnectionState::Connected);

        transport.set_connection_state(ConnectionState::Reconnecting);
        assert_eq!(transport.connection_state(), ConnectionState::Reconnecting);

        transport.set_connection_state(ConnectionState::Closed);
        assert_eq!(transport.connection_state(), ConnectionState::Closed);
    }

    #[test]
    fn test_connection_state_invalid_value_defaults_to_disconnected() {
        let transport = WebSocketTransport::new(WebSocketConfig::default());
        // Set an invalid state value directly
        transport.connection_state.store(255, Ordering::Release);
        // Should default to Disconnected for unknown values
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);
    }

    #[test]
    fn test_builder_all_options() {
        let transport = WebSocketTransportBuilder::new("wss://secure.example.com/mcp")
            .connect_timeout(Duration::from_secs(15))
            .ping_interval(Duration::from_secs(20))
            .pong_timeout(Duration::from_secs(8))
            .max_message_size(8 * 1024 * 1024)
            .no_auto_reconnect()
            .header("Authorization", "Bearer token")
            .header("X-Request-Id", "abc123")
            .build();

        assert_eq!(transport.url(), "wss://secure.example.com/mcp");
        assert!(!transport.is_connected());
        assert_eq!(transport.connection_state(), ConnectionState::Disconnected);
    }

    #[test]
    fn test_config_reconnect_settings() {
        let config = WebSocketConfig::new("ws://localhost:8080")
            .without_auto_reconnect()
            .with_max_reconnect_attempts(5);

        assert!(!config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 5);
    }

    #[test]
    fn test_config_default_reconnect_enabled() {
        let config = WebSocketConfig::default();
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 10);
    }

    #[test]
    fn test_config_custom_backoff() {
        use super::super::config::ExponentialBackoff;

        let backoff =
            ExponentialBackoff::new(Duration::from_millis(50), Duration::from_secs(5), 1.5);

        // Test backoff calculations
        assert_eq!(backoff.delay_for_attempt(0), Duration::from_millis(50));
        // 50 * 1.5 = 75ms
        assert_eq!(backoff.delay_for_attempt(1), Duration::from_millis(75));
        // 50 * 1.5^2 = 112.5ms -> 112ms
        assert_eq!(backoff.delay_for_attempt(2), Duration::from_millis(112));

        // Eventually caps at max_delay
        assert_eq!(backoff.delay_for_attempt(100), Duration::from_secs(5));
    }

    #[test]
    fn test_url_accessor() {
        let transport =
            WebSocketTransport::new(WebSocketConfig::new("ws://test.example.com:9000/path"));
        assert_eq!(transport.url(), "ws://test.example.com:9000/path");
    }

    #[test]
    fn test_websocket_scheme_variations() {
        // Test ws:// scheme
        let transport_ws = WebSocketTransport::new(WebSocketConfig::new("ws://localhost:8080"));
        assert_eq!(transport_ws.url(), "ws://localhost:8080");

        // Test wss:// scheme
        let transport_wss = WebSocketTransport::new(WebSocketConfig::new("wss://localhost:8443"));
        assert_eq!(transport_wss.url(), "wss://localhost:8443");
    }

    #[test]
    fn test_connection_state_enum_debug() {
        // Ensure ConnectionState implements Debug
        let state = ConnectionState::Connecting;
        let debug_output = format!("{state:?}");
        assert!(debug_output.contains("Connecting"));
    }

    #[test]
    fn test_connection_state_clone_and_copy() {
        let state = ConnectionState::Connected;
        let cloned = state; // Copy instead of clone for Copy types
        let copied = state;

        assert_eq!(state, cloned);
        assert_eq!(state, copied);
    }

    #[test]
    fn test_builder_default() {
        let builder = WebSocketTransportBuilder::default();
        let transport = builder.build();

        // Default URL from WebSocketConfig::default
        assert_eq!(transport.url(), "ws://localhost:8080/mcp");
    }
}
