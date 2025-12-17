//! WebSocket transport configuration types.

use std::time::Duration;

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
    pub fn with_allowed_origins(
        mut self,
        origins: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.allowed_origins
            .extend(origins.into_iter().map(Into::into));
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
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the ping interval.
    #[must_use]
    pub const fn with_ping_interval(mut self, interval: Duration) -> Self {
        self.ping_interval = interval;
        self
    }

    /// Set the pong timeout.
    #[must_use]
    pub const fn with_pong_timeout(mut self, timeout: Duration) -> Self {
        self.pong_timeout = timeout;
        self
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
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
    pub const fn new(initial_delay: Duration, max_delay: Duration, multiplier: f64) -> Self {
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
        let config = WebSocketConfig::new("ws://example.com/mcp").with_allowed_origins(origins);

        assert!(config.is_origin_allowed("https://app1.com"));
        assert!(config.is_origin_allowed("https://app2.com"));
        assert!(config.is_origin_allowed("https://app3.com"));
        assert!(!config.is_origin_allowed("https://app4.com"));
    }

    #[test]
    fn test_exponential_backoff() {
        let backoff =
            ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(10), 2.0);

        assert_eq!(backoff.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(backoff.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(backoff.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(backoff.delay_for_attempt(3), Duration::from_millis(800));

        // Should be capped at max_delay
        assert_eq!(backoff.delay_for_attempt(10), Duration::from_secs(10));
    }

    #[test]
    fn test_default_backoff() {
        let backoff = ExponentialBackoff::default();
        assert_eq!(backoff.initial_delay, Duration::from_millis(100));
        assert_eq!(backoff.max_delay, Duration::from_secs(30));
        assert!((backoff.multiplier - 2.0).abs() < f64::EPSILON);
    }
}
