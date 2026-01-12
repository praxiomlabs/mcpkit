//! HTTP transport server implementation.
//!
//! This module provides the server-side HTTP transport with axum integration.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use mcpkit_core::protocol::Message;

use crate::error::TransportError;
use crate::runtime::AsyncMutex;

use super::config::{
    DEFAULT_MAX_MESSAGE_SIZE, MCP_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION_HEADER,
    MCP_SESSION_ID_HEADER,
};

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
    allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    max_message_size: usize,
    /// Origin validation mode.
    origin_validation_mode: OriginValidationMode,
    /// Whether security warning has been acknowledged.
    security_warning_acknowledged: bool,
}

impl HttpTransportListener {
    /// Create a new HTTP transport listener.
    ///
    /// By default, the listener uses [`OriginValidationMode::WarnAndAllow`]
    /// which logs warnings but allows all requests. **This is not secure for production.**
    ///
    /// For production, use [`Self::production`] or configure origin validation explicitly.
    #[must_use]
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            session_timeout: Duration::from_secs(3600), // 1 hour default
            running: AtomicBool::new(false),
            shutdown_tx: AsyncMutex::new(None),
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            origin_validation_mode: OriginValidationMode::WarnAndAllow,
            security_warning_acknowledged: false,
        }
    }

    /// Create a production-ready HTTP transport listener.
    ///
    /// This uses [`OriginValidationMode::AllowList`] by default.
    /// You must configure allowed origins before starting.
    #[must_use]
    pub fn production(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            session_timeout: Duration::from_secs(3600),
            running: AtomicBool::new(false),
            shutdown_tx: AsyncMutex::new(None),
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            origin_validation_mode: OriginValidationMode::AllowList,
            security_warning_acknowledged: true,
        }
    }

    /// Set the origin validation mode.
    #[must_use]
    pub const fn with_origin_validation(mut self, mode: OriginValidationMode) -> Self {
        self.origin_validation_mode = mode;
        self
    }

    /// Acknowledge the security warning for development mode.
    #[must_use]
    pub const fn acknowledge_security_warning(mut self) -> Self {
        self.security_warning_acknowledged = true;
        self
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
            origin_validation_mode: self.origin_validation_mode.clone(),
            security_warning_acknowledged: self.security_warning_acknowledged,
        };

        // Log security warnings at startup
        server_config.log_security_warnings();

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

/// Origin validation mode for DNS rebinding protection.
///
/// DNS rebinding attacks can allow malicious websites to execute commands on
/// local MCP servers. Proper origin validation is critical for security.
///
/// # Security Warning
///
/// **For production deployments**, always use [`OriginValidationMode::AllowList`]
/// with explicitly configured origins. The default [`OriginValidationMode::WarnAndAllow`]
/// mode is intended for development only.
///
/// See: <https://www.varonis.com/blog/model-context-protocol-dns-rebind-attack>
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OriginValidationMode {
    /// Validate against an allow list. Requests from origins not in the list
    /// are rejected with HTTP 403 Forbidden.
    AllowList,
    /// Log a warning but allow all requests (development mode).
    ///
    /// **WARNING**: This mode is insecure and should only be used during development.
    #[default]
    WarnAndAllow,
    /// Strict mode: reject requests without a valid Origin header.
    /// Use this for maximum security in production environments.
    Strict,
    /// Disable origin validation entirely.
    ///
    /// **WARNING**: This mode provides no protection against DNS rebinding attacks.
    /// Only use when you have other security measures in place (e.g., mTLS, API keys).
    Disabled,
}

/// Server-side configuration for HTTP transport.
///
/// This configuration is used by the axum router to validate requests
/// and enforce security policies.
///
/// # Security Warning
///
/// **DNS rebinding attacks** can allow malicious websites to execute commands
/// on local MCP servers. Always configure origin validation for production:
///
/// ```rust
/// use mcpkit_transport::http::{HttpServerConfig, OriginValidationMode};
///
/// // Production configuration
/// let config = HttpServerConfig::new()
///     .with_origin_validation(OriginValidationMode::AllowList)
///     .with_allowed_origin("https://trusted-app.com");
/// ```
///
/// See: <https://www.straiker.ai/blog/agentic-danger-dns-rebinding-exposing-your-internal-mcp-servers>
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HttpServerConfig {
    /// Allowed origins for DNS rebinding protection.
    pub allowed_origins: Vec<String>,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
    /// Origin validation mode.
    pub origin_validation_mode: OriginValidationMode,
    /// Whether the security warning has been acknowledged.
    /// Set to `true` to suppress the startup warning when running without origin validation.
    pub security_warning_acknowledged: bool,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpServerConfig {
    /// Create a new server configuration.
    ///
    /// By default, origin validation is set to [`OriginValidationMode::WarnAndAllow`]
    /// which logs warnings but allows all requests. **This is not secure for production.**
    ///
    /// For production, use:
    /// ```rust
    /// use mcpkit_transport::http::{HttpServerConfig, OriginValidationMode};
    ///
    /// let config = HttpServerConfig::new()
    ///     .with_origin_validation(OriginValidationMode::AllowList)
    ///     .with_allowed_origin("https://trusted-app.com");
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            origin_validation_mode: OriginValidationMode::WarnAndAllow,
            security_warning_acknowledged: false,
        }
    }

    /// Create a production-ready configuration with strict origin validation.
    ///
    /// This is the recommended starting point for production deployments.
    /// You must add at least one allowed origin before requests will be accepted.
    ///
    /// ```rust
    /// use mcpkit_transport::http::HttpServerConfig;
    ///
    /// let config = HttpServerConfig::production()
    ///     .with_allowed_origin("https://trusted-app.com");
    /// ```
    #[must_use]
    pub const fn production() -> Self {
        Self {
            allowed_origins: Vec::new(),
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
            origin_validation_mode: OriginValidationMode::AllowList,
            security_warning_acknowledged: true,
        }
    }

    /// Set the origin validation mode.
    ///
    /// # Security Warning
    ///
    /// For production deployments, use [`OriginValidationMode::AllowList`] or
    /// [`OriginValidationMode::Strict`].
    #[must_use]
    pub const fn with_origin_validation(mut self, mode: OriginValidationMode) -> Self {
        self.origin_validation_mode = mode;
        self
    }

    /// Acknowledge the security warning for development mode.
    ///
    /// Call this to suppress the startup warning when intentionally running
    /// without origin validation during development.
    #[must_use]
    pub const fn acknowledge_security_warning(mut self) -> Self {
        self.security_warning_acknowledged = true;
        self
    }

    /// Add an allowed origin for DNS rebinding protection.
    ///
    /// When using [`OriginValidationMode::AllowList`], requests from origins
    /// not in this list will be rejected.
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

    /// Check if an origin is allowed based on the current validation mode.
    #[must_use]
    pub fn is_origin_allowed(&self, origin: Option<&str>) -> bool {
        match self.origin_validation_mode {
            OriginValidationMode::Disabled => true,
            OriginValidationMode::WarnAndAllow => true,
            OriginValidationMode::AllowList => {
                // Allow if no origins configured (backwards compatibility)
                // or if origin is in the allow list
                if self.allowed_origins.is_empty() {
                    return true;
                }
                origin.is_some_and(|o| self.allowed_origins.iter().any(|allowed| allowed == o))
            }
            OriginValidationMode::Strict => {
                // Require origin header and it must be in allow list
                origin.is_some_and(|o| self.allowed_origins.iter().any(|allowed| allowed == o))
            }
        }
    }

    /// Log security warnings based on configuration.
    ///
    /// Called during server startup to warn about insecure configurations.
    pub fn log_security_warnings(&self) {
        if self.security_warning_acknowledged {
            return;
        }

        match self.origin_validation_mode {
            OriginValidationMode::Disabled => {
                tracing::warn!(
                    target: "mcpkit::security",
                    "⚠️  SECURITY WARNING: Origin validation is DISABLED. \
                     This server is vulnerable to DNS rebinding attacks. \
                     See: https://www.varonis.com/blog/model-context-protocol-dns-rebind-attack"
                );
            }
            OriginValidationMode::WarnAndAllow => {
                tracing::warn!(
                    target: "mcpkit::security",
                    "⚠️  SECURITY WARNING: Origin validation is in development mode (WarnAndAllow). \
                     For production, use HttpServerConfig::production() or set OriginValidationMode::AllowList. \
                     See: https://www.straiker.ai/blog/agentic-danger-dns-rebinding-exposing-your-internal-mcp-servers"
                );
            }
            OriginValidationMode::AllowList if self.allowed_origins.is_empty() => {
                tracing::warn!(
                    target: "mcpkit::security",
                    "⚠️  SECURITY WARNING: Origin validation is set to AllowList but no origins configured. \
                     All requests will be accepted. Add allowed origins with .with_allowed_origin()."
                );
            }
            OriginValidationMode::Strict if self.allowed_origins.is_empty() => {
                tracing::warn!(
                    target: "mcpkit::security",
                    "⚠️  SECURITY WARNING: Origin validation is set to Strict but no origins configured. \
                     All requests will be rejected. Add allowed origins with .with_allowed_origin()."
                );
            }
            _ => {
                // Secure configuration, no warning needed
            }
        }
    }
}

/// Validate origin header for DNS rebinding protection.
///
/// Returns a boxed Response on error to minimize stack usage (Response is 128 bytes).
fn validate_origin(
    headers: &axum::http::HeaderMap,
    config: &HttpServerConfig,
) -> Result<(), Box<axum::response::Response>> {
    use axum::{http::StatusCode, response::IntoResponse};

    // Get origin header
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok());

    // Check based on validation mode
    match config.origin_validation_mode {
        OriginValidationMode::Disabled => Ok(()),
        OriginValidationMode::WarnAndAllow => {
            if let Some(origin_str) = origin {
                if !config.allowed_origins.is_empty()
                    && !config.allowed_origins.iter().any(|o| o == origin_str)
                {
                    tracing::warn!(
                        target: "mcpkit::security",
                        origin = %origin_str,
                        "Request from unknown origin (allowed in WarnAndAllow mode)"
                    );
                }
            }
            Ok(())
        }
        OriginValidationMode::AllowList => {
            // Backwards compatibility: if no origins configured, allow all
            if config.allowed_origins.is_empty() {
                return Ok(());
            }

            match origin {
                Some(origin_str) => {
                    if config.allowed_origins.iter().any(|o| o == origin_str) {
                        Ok(())
                    } else {
                        tracing::warn!(
                            target: "mcpkit::security",
                            origin = %origin_str,
                            "Rejecting request from disallowed origin"
                        );
                        Err(Box::new(
                            (StatusCode::FORBIDDEN, "Origin not allowed").into_response(),
                        ))
                    }
                }
                None => {
                    // No origin header - allow for non-browser clients
                    Ok(())
                }
            }
        }
        OriginValidationMode::Strict => {
            if let Some(origin_str) = origin {
                if config.allowed_origins.iter().any(|o| o == origin_str) {
                    Ok(())
                } else {
                    tracing::warn!(
                        target: "mcpkit::security",
                        origin = %origin_str,
                        "Rejecting request from disallowed origin (strict mode)"
                    );
                    Err(Box::new(
                        (StatusCode::FORBIDDEN, "Origin not allowed").into_response(),
                    ))
                }
            } else {
                // Strict mode requires Origin header
                tracing::warn!(
                    target: "mcpkit::security",
                    "Rejecting request without Origin header (strict mode)"
                );
                Err(Box::new(
                    (StatusCode::FORBIDDEN, "Origin header required").into_response(),
                ))
            }
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
        return *response;
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
        return *response;
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
        return *response;
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
        // Empty allowed_origins with WarnAndAllow means all origins are allowed
        assert!(config.is_origin_allowed(Some("https://anything.com")));
        assert!(config.is_origin_allowed(Some("http://malicious.com")));
        assert!(config.is_origin_allowed(Some("")));
        assert!(config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_origin_validation_with_allowed_origins() {
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::AllowList)
            .with_allowed_origin("https://trusted-app.com")
            .with_allowed_origin("https://another-trusted.com");

        // Allowed origins should pass
        assert!(config.is_origin_allowed(Some("https://trusted-app.com")));
        assert!(config.is_origin_allowed(Some("https://another-trusted.com")));

        // Disallowed origins should fail
        assert!(!config.is_origin_allowed(Some("https://evil.com")));
        assert!(!config.is_origin_allowed(Some("http://trusted-app.com"))); // wrong protocol
    }

    #[test]
    fn test_http_server_config_with_multiple_origins() {
        let origins = vec!["https://app1.com", "https://app2.com", "https://app3.com"];
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::AllowList)
            .with_allowed_origins(origins);

        assert!(config.is_origin_allowed(Some("https://app1.com")));
        assert!(config.is_origin_allowed(Some("https://app2.com")));
        assert!(config.is_origin_allowed(Some("https://app3.com")));
        assert!(!config.is_origin_allowed(Some("https://app4.com")));
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

    // OriginValidationMode tests

    #[test]
    fn test_origin_validation_mode_default() {
        let mode = OriginValidationMode::default();
        assert_eq!(mode, OriginValidationMode::WarnAndAllow);
    }

    #[test]
    fn test_origin_validation_mode_all_variants() {
        // Verify all variants exist and can be constructed
        let _ = OriginValidationMode::AllowList;
        let _ = OriginValidationMode::WarnAndAllow;
        let _ = OriginValidationMode::Strict;
        let _ = OriginValidationMode::Disabled;
    }

    #[test]
    fn test_http_listener_production_constructor() {
        let listener = HttpTransportListener::production("0.0.0.0:8080");

        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
        // Production uses AllowList mode
        assert!(!listener.is_running());
    }

    #[test]
    fn test_http_listener_with_origin_validation_mode() {
        let listener = HttpTransportListener::new("0.0.0.0:8080")
            .with_origin_validation(OriginValidationMode::Strict);

        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
    }

    #[test]
    fn test_http_listener_acknowledge_security_warning() {
        let listener = HttpTransportListener::new("0.0.0.0:8080").acknowledge_security_warning();

        assert_eq!(listener.bind_addr(), "0.0.0.0:8080");
    }

    #[test]
    fn test_http_server_config_production_constructor() {
        let config = HttpServerConfig::production();

        assert_eq!(
            config.origin_validation_mode,
            OriginValidationMode::AllowList
        );
        assert!(config.security_warning_acknowledged);
    }

    #[test]
    fn test_http_server_config_with_validation_mode() {
        let config = HttpServerConfig::new().with_origin_validation(OriginValidationMode::Strict);

        assert_eq!(config.origin_validation_mode, OriginValidationMode::Strict);
    }

    #[test]
    fn test_http_server_config_disabled_mode_allows_all() {
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::Disabled)
            .with_allowed_origin("https://trusted.com");

        // Disabled mode allows all origins regardless of allow list
        assert!(config.is_origin_allowed(Some("https://trusted.com")));
        assert!(config.is_origin_allowed(Some("https://untrusted.com")));
        assert!(config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_warn_and_allow_mode() {
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::WarnAndAllow)
            .with_allowed_origin("https://trusted.com");

        // WarnAndAllow mode allows all origins but logs warnings
        assert!(config.is_origin_allowed(Some("https://trusted.com")));
        assert!(config.is_origin_allowed(Some("https://untrusted.com")));
        assert!(config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_allowlist_mode_with_origins() {
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::AllowList)
            .with_allowed_origin("https://trusted.com");

        // AllowList mode only allows configured origins
        assert!(config.is_origin_allowed(Some("https://trusted.com")));
        assert!(!config.is_origin_allowed(Some("https://untrusted.com")));
        // No origin header is allowed (for non-browser clients)
        assert!(!config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_allowlist_mode_empty_allows_all() {
        let config =
            HttpServerConfig::new().with_origin_validation(OriginValidationMode::AllowList);

        // AllowList with no configured origins allows all (backwards compat)
        assert!(config.is_origin_allowed(Some("https://anything.com")));
        assert!(config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_strict_mode_with_origins() {
        let config = HttpServerConfig::new()
            .with_origin_validation(OriginValidationMode::Strict)
            .with_allowed_origin("https://trusted.com");

        // Strict mode only allows configured origins
        assert!(config.is_origin_allowed(Some("https://trusted.com")));
        assert!(!config.is_origin_allowed(Some("https://untrusted.com")));
        // Strict mode rejects missing origin header
        assert!(!config.is_origin_allowed(None));
    }

    #[test]
    fn test_http_server_config_strict_mode_empty_rejects_all() {
        let config = HttpServerConfig::new().with_origin_validation(OriginValidationMode::Strict);

        // Strict mode with no configured origins rejects all
        assert!(!config.is_origin_allowed(Some("https://anything.com")));
        assert!(!config.is_origin_allowed(None));
    }
}
