//! WebSocket transport server implementation.

use std::sync::atomic::{AtomicBool, Ordering};

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
    pub const fn config(&self) -> &WebSocketServerConfig {
        &self.config
    }

    /// Start listening for connections.
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
    pub const fn config(&self) -> &WebSocketServerConfig {
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
