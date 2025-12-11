//! Typestate pattern for connection lifecycle management.
//!
//! This module implements the typestate pattern to enforce correct
//! connection state transitions at compile time. This prevents runtime
//! errors from calling methods on connections in invalid states.
//!
//! # Connection States
//!
//! ```text
//! Disconnected -> Connected -> Initializing -> Ready -> Closing -> Disconnected
//! ```
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::state::{Connection, Disconnected, Connected};
//!
//! // Connection starts in Disconnected state
//! let conn: Connection<Disconnected> = Connection::new();
//!
//! // Each state has appropriate methods
//! let id = conn.id();
//! assert!(!id.is_empty());
//!
//! // Connect to transition to Connected state
//! let connected: Connection<Connected> = conn.connect();
//! assert!(connected.connected_at().is_some());
//! ```

use std::marker::PhantomData;
use std::time::{Duration, Instant};

use crate::capability::{
    ClientCapabilities, ClientInfo, InitializeRequest, InitializeResult, ServerCapabilities,
    ServerInfo,
};
use crate::error::McpError;
use crate::protocol::RequestId;

/// Marker type for disconnected state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Disconnected;

/// Marker type for connected state (transport established).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connected;

/// Marker type for initializing state (handshake in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Initializing;

/// Marker type for ready state (fully operational).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ready;

/// Marker type for closing state (shutdown in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closing;

/// Internal connection data shared across states.
#[derive(Debug)]
pub struct ConnectionInner {
    /// Unique connection identifier.
    pub id: String,
    /// When the connection was established.
    pub connected_at: Option<Instant>,
    /// Last activity timestamp.
    pub last_activity: Option<Instant>,
    /// Request counter for generating IDs.
    pub request_counter: u64,
    /// Client info (available after initialization).
    pub client_info: Option<ClientInfo>,
    /// Server info (available after initialization).
    pub server_info: Option<ServerInfo>,
    /// Client capabilities (available after initialization).
    pub client_capabilities: Option<ClientCapabilities>,
    /// Server capabilities (available after initialization).
    pub server_capabilities: Option<ServerCapabilities>,
}

impl ConnectionInner {
    /// Create new connection inner data.
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            connected_at: None,
            last_activity: None,
            request_counter: 0,
            client_info: None,
            server_info: None,
            client_capabilities: None,
            server_capabilities: None,
        }
    }

    /// Generate the next request ID.
    fn next_request_id(&mut self) -> RequestId {
        self.request_counter += 1;
        RequestId::Number(self.request_counter)
    }

    /// Update last activity timestamp.
    fn touch(&mut self) {
        self.last_activity = Some(Instant::now());
    }
}

impl Default for ConnectionInner {
    fn default() -> Self {
        Self::new()
    }
}

/// A connection in a specific state.
///
/// The type parameter `S` represents the current state of the connection.
/// Different methods are available depending on the state.
#[derive(Debug)]
pub struct Connection<S> {
    inner: ConnectionInner,
    _state: PhantomData<S>,
}

impl Connection<Disconnected> {
    /// Create a new disconnected connection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: ConnectionInner::new(),
            _state: PhantomData,
        }
    }

    /// Get the connection ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Establish the connection (transition to Connected state).
    ///
    /// In a real implementation, this would take a transport and
    /// establish the connection. Here we just transition the state.
    #[must_use]
    pub fn connect(mut self) -> Connection<Connected> {
        self.inner.connected_at = Some(Instant::now());
        self.inner.touch();
        Connection {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl Default for Connection<Disconnected> {
    fn default() -> Self {
        Self::new()
    }
}

impl Connection<Connected> {
    /// Get the connection ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Get when the connection was established.
    #[must_use]
    pub fn connected_at(&self) -> Option<Instant> {
        self.inner.connected_at
    }

    /// Get how long the connection has been active.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.inner
            .connected_at
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }

    /// Begin initialization (transition to Initializing state).
    ///
    /// For clients: Send initialize request with client info and capabilities.
    /// For servers: This is called when receiving an initialize request.
    pub fn initialize(
        mut self,
        client_info: ClientInfo,
        client_capabilities: ClientCapabilities,
    ) -> (Connection<Initializing>, InitializeRequest) {
        self.inner.client_info = Some(client_info.clone());
        self.inner.client_capabilities = Some(client_capabilities.clone());
        self.inner.touch();

        let request = InitializeRequest::new(client_info, client_capabilities);

        (
            Connection {
                inner: self.inner,
                _state: PhantomData,
            },
            request,
        )
    }

    /// Disconnect (transition back to Disconnected state).
    #[must_use]
    pub fn disconnect(self) -> Connection<Disconnected> {
        Connection {
            inner: ConnectionInner::new(),
            _state: PhantomData,
        }
    }
}

impl Connection<Initializing> {
    /// Get the connection ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Get the client info.
    #[must_use]
    pub fn client_info(&self) -> Option<&ClientInfo> {
        self.inner.client_info.as_ref()
    }

    /// Get the client capabilities.
    #[must_use]
    pub fn client_capabilities(&self) -> Option<&ClientCapabilities> {
        self.inner.client_capabilities.as_ref()
    }

    /// Complete initialization (transition to Ready state).
    ///
    /// This is called after the initialize response is received (client)
    /// or sent (server).
    pub fn complete(
        mut self,
        server_info: ServerInfo,
        server_capabilities: ServerCapabilities,
    ) -> Connection<Ready> {
        self.inner.server_info = Some(server_info);
        self.inner.server_capabilities = Some(server_capabilities);
        self.inner.touch();

        Connection {
            inner: self.inner,
            _state: PhantomData,
        }
    }

    /// Abort initialization (transition back to Disconnected).
    #[must_use]
    pub fn abort(self) -> Connection<Disconnected> {
        Connection {
            inner: ConnectionInner::new(),
            _state: PhantomData,
        }
    }
}

impl Connection<Ready> {
    /// Get the connection ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Get when the connection was established.
    #[must_use]
    pub fn connected_at(&self) -> Option<Instant> {
        self.inner.connected_at
    }

    /// Get how long the connection has been active.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.inner
            .connected_at
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }

    /// Get the last activity timestamp.
    #[must_use]
    pub fn last_activity(&self) -> Option<Instant> {
        self.inner.last_activity
    }

    /// Get the client info.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized,
    /// as the typestate pattern ensures this is only callable in Ready state.
    /// Use `try_client_info()` for a fallible version.
    #[must_use]
    pub fn client_info(&self) -> &ClientInfo {
        self.inner.client_info.as_ref().expect("client_info should be set in Ready state")
    }

    /// Try to get the client info.
    ///
    /// Returns `None` if the client info was not set (should not happen in normal use).
    #[must_use]
    pub fn try_client_info(&self) -> Option<&ClientInfo> {
        self.inner.client_info.as_ref()
    }

    /// Get the server info.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized,
    /// as the typestate pattern ensures this is only callable in Ready state.
    /// Use `try_server_info()` for a fallible version.
    #[must_use]
    pub fn server_info(&self) -> &ServerInfo {
        self.inner.server_info.as_ref().expect("server_info should be set in Ready state")
    }

    /// Try to get the server info.
    ///
    /// Returns `None` if the server info was not set (should not happen in normal use).
    #[must_use]
    pub fn try_server_info(&self) -> Option<&ServerInfo> {
        self.inner.server_info.as_ref()
    }

    /// Get the client capabilities.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized,
    /// as the typestate pattern ensures this is only callable in Ready state.
    /// Use `try_client_capabilities()` for a fallible version.
    #[must_use]
    pub fn client_capabilities(&self) -> &ClientCapabilities {
        self.inner.client_capabilities.as_ref().expect("client_capabilities should be set in Ready state")
    }

    /// Try to get the client capabilities.
    ///
    /// Returns `None` if the client capabilities were not set (should not happen in normal use).
    #[must_use]
    pub fn try_client_capabilities(&self) -> Option<&ClientCapabilities> {
        self.inner.client_capabilities.as_ref()
    }

    /// Get the server capabilities.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized,
    /// as the typestate pattern ensures this is only callable in Ready state.
    /// Use `try_server_capabilities()` for a fallible version.
    #[must_use]
    pub fn server_capabilities(&self) -> &ServerCapabilities {
        self.inner.server_capabilities.as_ref().expect("server_capabilities should be set in Ready state")
    }

    /// Try to get the server capabilities.
    ///
    /// Returns `None` if the server capabilities were not set (should not happen in normal use).
    #[must_use]
    pub fn try_server_capabilities(&self) -> Option<&ServerCapabilities> {
        self.inner.server_capabilities.as_ref()
    }

    /// Generate the next request ID.
    pub fn next_request_id(&mut self) -> RequestId {
        self.inner.next_request_id()
    }

    /// Update the last activity timestamp.
    pub fn touch(&mut self) {
        self.inner.touch();
    }

    /// Check if the connection has been idle for longer than the given duration.
    #[must_use]
    pub fn is_idle(&self, timeout: Duration) -> bool {
        self.inner
            .last_activity
            .map(|t| t.elapsed() > timeout)
            .unwrap_or(false)
    }

    /// Begin shutdown (transition to Closing state).
    #[must_use]
    pub fn shutdown(self) -> Connection<Closing> {
        Connection {
            inner: self.inner,
            _state: PhantomData,
        }
    }
}

impl Connection<Closing> {
    /// Get the connection ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Complete the shutdown (transition to Disconnected state).
    #[must_use]
    pub fn close(self) -> Connection<Disconnected> {
        Connection {
            inner: ConnectionInner::new(),
            _state: PhantomData,
        }
    }
}

/// Builder for creating initialize results (used by servers).
pub struct InitializeResultBuilder {
    server_info: ServerInfo,
    capabilities: ServerCapabilities,
    instructions: Option<String>,
}

impl InitializeResultBuilder {
    /// Create a new builder with server info.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            server_info: ServerInfo::new(name, version),
            capabilities: ServerCapabilities::new(),
            instructions: None,
        }
    }

    /// Set the capabilities.
    #[must_use]
    pub fn capabilities(mut self, caps: ServerCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    /// Enable tool support.
    #[must_use]
    pub fn with_tools(mut self) -> Self {
        self.capabilities = self.capabilities.with_tools();
        self
    }

    /// Enable resource support.
    #[must_use]
    pub fn with_resources(mut self) -> Self {
        self.capabilities = self.capabilities.with_resources();
        self
    }

    /// Enable prompt support.
    #[must_use]
    pub fn with_prompts(mut self) -> Self {
        self.capabilities = self.capabilities.with_prompts();
        self
    }

    /// Enable task support.
    #[must_use]
    pub fn with_tasks(mut self) -> Self {
        self.capabilities = self.capabilities.with_tasks();
        self
    }

    /// Set instructions.
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Build the initialize result.
    #[must_use]
    pub fn build(self) -> InitializeResult {
        let mut result = InitializeResult::new(self.server_info, self.capabilities);
        if let Some(instructions) = self.instructions {
            result = result.instructions(instructions);
        }
        result
    }
}

/// Validate that a connection can transition to the ready state.
pub fn validate_initialization(
    _client_caps: &ClientCapabilities,
    _server_caps: &ServerCapabilities,
) -> Result<(), McpError> {
    // For now, just return Ok. In a real implementation, you might
    // check for required capability combinations.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_lifecycle() {
        // Start disconnected
        let conn: Connection<Disconnected> = Connection::new();
        assert!(!conn.id().is_empty());

        // Connect
        let conn: Connection<Connected> = conn.connect();
        assert!(conn.connected_at().is_some());

        // Initialize
        let client = ClientInfo::new("test", "1.0.0");
        let caps = ClientCapabilities::new();
        let (conn, request): (Connection<Initializing>, _) = conn.initialize(client, caps);
        assert!(conn.client_info().is_some());

        // Complete
        let server = ServerInfo::new("server", "1.0.0");
        let server_caps = ServerCapabilities::new().with_tools();
        let mut conn: Connection<Ready> = conn.complete(server, server_caps);
        assert!(conn.server_capabilities().has_tools());

        // Generate request IDs
        let id1 = conn.next_request_id();
        let id2 = conn.next_request_id();
        assert_ne!(id1, id2);

        // Shutdown
        let conn: Connection<Closing> = conn.shutdown();
        let _conn: Connection<Disconnected> = conn.close();
    }

    #[test]
    fn test_uptime() {
        let conn = Connection::new().connect();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(conn.uptime() >= std::time::Duration::from_millis(10));
    }

    #[test]
    fn test_idle_detection() {
        let client = ClientInfo::new("test", "1.0.0");
        let server = ServerInfo::new("server", "1.0.0");

        let (conn, _) = Connection::new()
            .connect()
            .initialize(client, ClientCapabilities::new());

        let conn = conn.complete(server, ServerCapabilities::new());

        // Should not be idle immediately
        assert!(!conn.is_idle(Duration::from_secs(1)));
    }

    #[test]
    fn test_initialize_result_builder() {
        let result = InitializeResultBuilder::new("my-server", "1.0.0")
            .with_tools()
            .with_resources()
            .instructions("Use this server to access tools and resources")
            .build();

        assert_eq!(result.server_info.name, "my-server");
        assert!(result.capabilities.has_tools());
        assert!(result.capabilities.has_resources());
        assert!(result.instructions.is_some());
    }

    #[test]
    fn test_abort_initialization() {
        let client = ClientInfo::new("test", "1.0.0");
        let (conn, _) = Connection::new()
            .connect()
            .initialize(client, ClientCapabilities::new());

        // Abort should return to disconnected
        let _conn: Connection<Disconnected> = conn.abort();
    }

    #[test]
    fn test_disconnect_from_connected() {
        let conn = Connection::new().connect();
        let _conn: Connection<Disconnected> = conn.disconnect();
    }

    #[test]
    fn test_fallible_accessors() {
        let client = ClientInfo::new("test-client", "1.0.0");
        let server = ServerInfo::new("test-server", "2.0.0");
        let client_caps = ClientCapabilities::new();
        let server_caps = ServerCapabilities::new().with_tools();

        let (conn, _) = Connection::new()
            .connect()
            .initialize(client.clone(), client_caps.clone());

        let conn = conn.complete(server.clone(), server_caps.clone());

        // Test fallible accessors return Some
        assert!(conn.try_client_info().is_some());
        assert!(conn.try_server_info().is_some());
        assert!(conn.try_client_capabilities().is_some());
        assert!(conn.try_server_capabilities().is_some());

        // Test values are correct
        assert_eq!(conn.try_client_info().unwrap().name, "test-client");
        assert_eq!(conn.try_server_info().unwrap().name, "test-server");
        assert!(conn.try_server_capabilities().unwrap().has_tools());
    }
}
