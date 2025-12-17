//! HTTP transport server implementation.
//!
//! This module provides the server-side HTTP transport with axum integration.

#![cfg(feature = "http")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mcpkit_core::protocol::Message;

use crate::error::TransportError;
use crate::runtime::AsyncMutex;

use super::config::{DEFAULT_MAX_MESSAGE_SIZE, MCP_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION_HEADER, MCP_SESSION_ID_HEADER};

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
    #[must_use]
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
    #[must_use]
    pub fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    /// Get the maximum message size.
    #[must_use]
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
            Router,
            routing::{delete, get, post},
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

/// Server-side configuration for HTTP transport.
///
/// This configuration is used by the axum router to validate requests
/// and enforce security policies.
#[derive(Debug, Clone, Default)]
pub struct HttpServerConfig {
    /// Allowed origins for DNS rebinding protection.
    /// If empty, origin validation is disabled.
    pub allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

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
async fn handle_mcp_sse_with_state(
    axum::extract::State(config): axum::extract::State<std::sync::Arc<HttpServerConfig>>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    use axum::{
        http::StatusCode,
        response::{
            IntoResponse,
            sse::{Event, KeepAlive, Sse},
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_creation() {
        let listener = HttpTransportListener::new("0.0.0.0:8080");
        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
        assert!(!listener.is_running());
    }

    #[test]
    fn test_http_server_config_origin_validation_empty_allows_all() {
        let config = HttpServerConfig::new();
        // Empty allowed_origins means validation is disabled (allow all)
        assert!(config.is_origin_allowed("https://anything.com"));
        assert!(config.is_origin_allowed("http://malicious.com"));
        assert!(config.is_origin_allowed(""));
    }

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

    #[test]
    fn test_http_server_config_with_multiple_origins() {
        let origins = vec!["https://app1.com", "https://app2.com", "https://app3.com"];
        let config = HttpServerConfig::new().with_allowed_origins(origins);

        assert!(config.is_origin_allowed("https://app1.com"));
        assert!(config.is_origin_allowed("https://app2.com"));
        assert!(config.is_origin_allowed("https://app3.com"));
        assert!(!config.is_origin_allowed("https://app4.com"));
    }

    #[test]
    fn test_http_server_config_max_message_size() {
        let config = HttpServerConfig::new().with_max_message_size(1024);

        assert_eq!(config.max_message_size, 1024);
    }

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
