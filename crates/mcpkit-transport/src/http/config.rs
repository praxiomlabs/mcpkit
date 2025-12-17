//! HTTP transport configuration types and constants.

use std::time::Duration;

/// MCP Protocol version for the HTTP transport.
///
/// This matches `ProtocolVersion::LATEST` from mcpkit-core.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

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

/// Builder for HTTP transport.
#[derive(Debug, Default)]
pub struct HttpTransportBuilder {
    /// The configuration being built.
    pub(crate) config: HttpTransportConfig,
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
}
