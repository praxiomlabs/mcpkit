//! WebSocket transport server implementation.
//!
//! This module provides server-side WebSocket transport for MCP.
//!
//! # Connection Handling
//!
//! The listener accepts connections and makes them available through
//! the [`WebSocketListener::accept`] method. Use this in a loop to
//! handle incoming connections:
//!
//! ```ignore
//! let listener = WebSocketListener::new("0.0.0.0:8080").start().await?;
//!
//! while let Ok(transport) = listener.accept().await {
//!     tokio::spawn(async move {
//!         // Handle the connection
//!         while let Some(msg) = transport.recv().await? {
//!             // Process messages
//!         }
//!     });
//! }
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "websocket")]
use std::sync::Arc;
#[cfg(feature = "websocket")]
use std::sync::atomic::AtomicU64;

use crate::error::TransportError;

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
    pub const fn new() -> Self {
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

/// WebSocket listener for server-side connections.
///
/// This listener accepts incoming WebSocket connections and provides them
/// through the [`accept`](Self::accept) method. It properly tracks active
/// connections and task handles for graceful shutdown.
///
/// # Example
///
/// ```ignore
/// use mcpkit_transport::websocket::WebSocketListener;
///
/// let listener = WebSocketListener::new("0.0.0.0:8080");
/// listener.start().await?;
///
/// while let Ok(transport) = listener.accept().await {
///     tokio::spawn(async move {
///         // Handle the connection
///     });
/// }
/// ```
#[cfg(feature = "websocket")]
pub struct WebSocketListener {
    bind_addr: String,
    config: WebSocketServerConfig,
    running: AtomicBool,
    /// Channel for delivering accepted connections to callers.
    connection_tx: tokio::sync::mpsc::Sender<AcceptedConnection>,
    /// Channel for receiving accepted connections.
    connection_rx: crate::runtime::AsyncMutex<tokio::sync::mpsc::Receiver<AcceptedConnection>>,
    /// Active connection count for metrics and shutdown coordination (shared with guards).
    active_connections: Arc<AtomicU64>,
    /// Shutdown signal sender.
    shutdown_tx: crate::runtime::AsyncMutex<Option<tokio::sync::broadcast::Sender<()>>>,
}

// SAFETY: WebSocketListener is RefUnwindSafe because:
// - All fields are either inherently panic-safe or wrapped in Arc/AtomicBool
// - The AsyncMutex fields only contain types that can safely be dropped after a panic
// - This maintains backwards compatibility with v0.2.5
#[cfg(feature = "websocket")]
impl std::panic::RefUnwindSafe for WebSocketListener {}

/// An accepted WebSocket connection with metadata.
#[cfg(feature = "websocket")]
pub struct AcceptedConnection {
    /// The WebSocket stream.
    pub stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    /// Remote peer address.
    pub peer_addr: std::net::SocketAddr,
    /// Connection ID for tracking.
    pub connection_id: u64,
}

#[cfg(feature = "websocket")]
impl WebSocketListener {
    /// Create a new WebSocket listener.
    #[must_use]
    pub fn new(bind_addr: impl Into<String>) -> Self {
        // Buffer up to 32 pending connections
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        Self {
            bind_addr: bind_addr.into(),
            config: WebSocketServerConfig::new(),
            running: AtomicBool::new(false),
            connection_tx: tx,
            connection_rx: crate::runtime::AsyncMutex::new(rx),
            active_connections: Arc::new(AtomicU64::new(0)),
            shutdown_tx: crate::runtime::AsyncMutex::new(None),
        }
    }

    /// Create a new WebSocket listener with configuration.
    #[must_use]
    pub fn with_config(bind_addr: impl Into<String>, config: WebSocketServerConfig) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        Self {
            bind_addr: bind_addr.into(),
            config,
            running: AtomicBool::new(false),
            connection_tx: tx,
            connection_rx: crate::runtime::AsyncMutex::new(rx),
            active_connections: Arc::new(AtomicU64::new(0)),
            shutdown_tx: crate::runtime::AsyncMutex::new(None),
        }
    }

    /// Get the server configuration.
    #[must_use]
    pub const fn config(&self) -> &WebSocketServerConfig {
        &self.config
    }

    /// Get the number of active connections.
    #[must_use]
    pub fn active_connections(&self) -> u64 {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Accept the next incoming connection.
    ///
    /// This method returns the next accepted WebSocket connection, or an error
    /// if the listener has been stopped.
    ///
    /// # Example
    ///
    /// ```ignore
    /// while let Ok(conn) = listener.accept().await {
    ///     let transport = WebSocketTransport::from_stream(conn.stream);
    ///     // Handle the transport...
    /// }
    /// ```
    pub async fn accept(&self) -> Result<AcceptedConnection, TransportError> {
        let mut rx = self.connection_rx.lock().await;
        rx.recv().await.ok_or_else(|| TransportError::Connection {
            message: "Listener stopped".to_string(),
        })
    }

    /// Start listening for connections.
    ///
    /// This spawns a background task that accepts connections and makes them
    /// available through [`accept`](Self::accept). Call [`stop`](Self::stop)
    /// to shut down the listener.
    pub async fn start(&self) -> Result<(), TransportError> {
        use tokio::net::TcpListener;

        let listener =
            TcpListener::bind(&self.bind_addr)
                .await
                .map_err(|e| TransportError::Connection {
                    message: format!("Failed to bind WebSocket listener: {e}"),
                })?;

        self.running.store(true, Ordering::Release);
        tracing::info!(addr = %self.bind_addr, "WebSocket listener started");

        let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx.clone());

        let connection_id = Arc::new(AtomicU64::new(0));

        while self.running.load(Ordering::Acquire) {
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            tracing::debug!(peer = %addr, "Accepting WebSocket connection");

                            let allowed_origins = self.config.allowed_origins.clone();
                            let tx = self.connection_tx.clone();
                            let conn_id = connection_id.fetch_add(1, Ordering::Relaxed);
                            let active_conns_counter = Arc::clone(&self.active_connections);

                            // Increment active connection count
                            self.active_connections.fetch_add(1, Ordering::Relaxed);

                            // Create guard that decrements on drop
                            let guard = ActiveConnectionGuard {
                                counter: active_conns_counter,
                            };

                            // Spawn task to handle WebSocket upgrade
                            tokio::spawn(async move {
                                let _guard = guard;

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
                                        tracing::info!(
                                            peer = %addr,
                                            connection_id = conn_id,
                                            "WebSocket connection established"
                                        );

                                        // Send the accepted connection to the channel
                                        let connection = AcceptedConnection {
                                            stream: ws_stream,
                                            peer_addr: addr,
                                            connection_id: conn_id,
                                        };

                                        if tx.send(connection).await.is_err() {
                                            tracing::warn!(
                                                connection_id = conn_id,
                                                "Connection channel closed, dropping connection"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            peer = %addr,
                                            error = %e,
                                            "WebSocket handshake failed"
                                        );
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
                _ = shutdown_rx.recv() => {
                    tracing::info!("WebSocket listener shutting down");
                    break;
                }
            }
        }

        self.running.store(false, Ordering::Release);
        Ok(())
    }

    /// Stop the listener gracefully.
    ///
    /// This signals the listener to stop accepting new connections. Existing
    /// connections remain active until explicitly closed.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Release);
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        tracing::info!(
            active_connections = self.active_connections(),
            "WebSocket listener stopped"
        );
    }

    /// Check if the listener is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get the bind address.
    #[must_use]
    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }
}

/// Guard that decrements active connection count on drop.
///
/// Uses `Arc<AtomicU64>` for safe shared ownership across tasks.
#[cfg(feature = "websocket")]
struct ActiveConnectionGuard {
    counter: Arc<AtomicU64>,
}

#[cfg(feature = "websocket")]
impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Stub listener when websocket feature is disabled.
#[cfg(not(feature = "websocket"))]
pub struct WebSocketListener {
    bind_addr: String,
    config: WebSocketServerConfig,
    running: AtomicBool,
}

/// Stub for `AcceptedConnection` when websocket feature is disabled.
#[cfg(not(feature = "websocket"))]
pub struct AcceptedConnection {
    _private: (),
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
    pub const fn config(&self) -> &WebSocketServerConfig {
        &self.config
    }

    /// Get the number of active connections (always 0 when feature disabled).
    #[must_use]
    pub fn active_connections(&self) -> u64 {
        0
    }

    /// Accept a connection (stub - always returns error).
    pub async fn accept(&self) -> Result<AcceptedConnection, TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Start listening (stub).
    pub async fn start(&self) -> Result<(), TransportError> {
        Err(TransportError::Connection {
            message: "WebSocket transport requires the 'websocket' feature".to_string(),
        })
    }

    /// Stop the listener.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    /// Check if the listener is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Get the bind address.
    #[must_use]
    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_creation() {
        let listener = WebSocketListener::new("0.0.0.0:8080");
        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
        assert!(!listener.is_running());
    }
}
