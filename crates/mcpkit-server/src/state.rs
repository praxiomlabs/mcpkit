//! Typestate connection management for MCP servers.
//!
//! This module implements the typestate pattern for managing
//! connection lifecycle, ensuring compile-time correctness of
//! state transitions.
//!
//! # Connection Lifecycle
//!
//! ```text
//! Disconnected -> Connected -> Initializing -> Ready -> Closing
//! ```
//!
//! Each state transition is enforced at compile time through
//! different types, preventing invalid state transitions.

use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use std::marker::PhantomData;
use std::sync::Arc;

/// Connection state markers.
///
/// These types represent different states in the connection lifecycle.
/// They contain no data and are used purely for type-level state tracking.
pub mod state {
    /// Connection is disconnected (initial state).
    #[derive(Debug, Clone, Copy)]
    pub struct Disconnected;

    /// Connection is established but not initialized.
    #[derive(Debug, Clone, Copy)]
    pub struct Connected;

    /// Connection is in the initialization handshake.
    #[derive(Debug, Clone, Copy)]
    pub struct Initializing;

    /// Connection is fully initialized and ready for requests.
    #[derive(Debug, Clone, Copy)]
    pub struct Ready;

    /// Connection is closing down.
    #[derive(Debug, Clone, Copy)]
    pub struct Closing;
}

/// Internal connection data shared across states.
#[derive(Debug)]
pub struct ConnectionData {
    /// Client capabilities (set after initialization).
    pub client_capabilities: Option<ClientCapabilities>,
    /// Server capabilities advertised.
    pub server_capabilities: ServerCapabilities,
    /// Server information.
    pub server_info: ServerInfo,
    /// Protocol version negotiated.
    pub protocol_version: Option<String>,
    /// Session ID if applicable.
    pub session_id: Option<String>,
}

impl ConnectionData {
    /// Create new connection data.
    #[must_use]
    pub const fn new(server_info: ServerInfo, server_capabilities: ServerCapabilities) -> Self {
        Self {
            client_capabilities: None,
            server_capabilities,
            server_info,
            protocol_version: None,
            session_id: None,
        }
    }
}

/// A typestate connection that tracks lifecycle state at the type level.
///
/// The state parameter `S` ensures that only valid operations are
/// available for each connection state.
///
/// # Example
///
/// ```rust
/// use mcpkit_server::state::{Connection, state};
/// use mcpkit_core::capability::{ServerInfo, ServerCapabilities};
///
/// // Start disconnected
/// let conn: Connection<state::Disconnected> = Connection::new(
///     ServerInfo::new("my-server", "1.0.0"),
///     ServerCapabilities::new().with_tools(),
/// );
///
/// // The typestate pattern ensures compile-time safety:
/// // - A Disconnected connection can only call connect()
/// // - A Connected connection can only call initialize() or close()
/// // - A Ready connection can access capabilities
/// ```
pub struct Connection<S> {
    /// Shared connection data.
    inner: Arc<ConnectionData>,
    /// Phantom data to track state type.
    _state: PhantomData<S>,
}

impl<S> Clone for Connection<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _state: PhantomData,
        }
    }
}

impl<S> std::fmt::Debug for Connection<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("inner", &self.inner)
            .field("state", &std::any::type_name::<S>())
            .finish()
    }
}

impl Connection<state::Disconnected> {
    /// Create a new disconnected connection.
    #[must_use]
    pub fn new(server_info: ServerInfo, server_capabilities: ServerCapabilities) -> Self {
        Self {
            inner: Arc::new(ConnectionData::new(server_info, server_capabilities)),
            _state: PhantomData,
        }
    }

    /// Connect to establish a transport connection.
    ///
    /// This transitions from `Disconnected` to `Connected` state.
    pub async fn connect(self) -> Result<Connection<state::Connected>, McpError> {
        // In a real implementation, this would establish the transport
        Ok(Connection {
            inner: self.inner,
            _state: PhantomData,
        })
    }
}

impl Connection<state::Connected> {
    /// Start the initialization handshake.
    ///
    /// This transitions from `Connected` to `Initializing` state.
    pub async fn initialize(
        self,
        _protocol_version: &str,
    ) -> Result<Connection<state::Initializing>, McpError> {
        // In a real implementation, this would send the initialize request
        Ok(Connection {
            inner: self.inner,
            _state: PhantomData,
        })
    }

    /// Close the connection before initialization.
    pub async fn close(self) -> Result<(), McpError> {
        // Clean up resources
        Ok(())
    }
}

impl Connection<state::Initializing> {
    /// Complete the initialization handshake.
    ///
    /// This transitions from `Initializing` to `Ready` state.
    pub async fn complete(
        self,
        client_capabilities: ClientCapabilities,
        protocol_version: String,
    ) -> Result<Connection<state::Ready>, McpError> {
        // Update the connection data with negotiated values
        // In a real implementation, we'd use interior mutability
        let mut data = ConnectionData::new(
            self.inner.server_info.clone(),
            self.inner.server_capabilities.clone(),
        );
        data.client_capabilities = Some(client_capabilities);
        data.protocol_version = Some(protocol_version);

        Ok(Connection {
            inner: Arc::new(data),
            _state: PhantomData,
        })
    }

    /// Abort initialization.
    pub async fn abort(self) -> Result<Connection<state::Disconnected>, McpError> {
        Ok(Connection {
            inner: self.inner,
            _state: PhantomData,
        })
    }
}

impl Connection<state::Ready> {
    /// Get the client capabilities.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized
    /// through the typestate transitions. Use `try_client_capabilities()`
    /// for a fallible version.
    #[must_use]
    pub fn client_capabilities(&self) -> &ClientCapabilities {
        self.inner
            .client_capabilities
            .as_ref()
            .expect("Ready connection must have client capabilities")
    }

    /// Try to get the client capabilities.
    ///
    /// Returns `None` if capabilities were not set (should not happen in normal use).
    #[must_use]
    pub fn try_client_capabilities(&self) -> Option<&ClientCapabilities> {
        self.inner.client_capabilities.as_ref()
    }

    /// Get the server capabilities.
    #[must_use]
    pub fn server_capabilities(&self) -> &ServerCapabilities {
        &self.inner.server_capabilities
    }

    /// Get the server info.
    #[must_use]
    pub fn server_info(&self) -> &ServerInfo {
        &self.inner.server_info
    }

    /// Get the negotiated protocol version.
    ///
    /// # Panics
    ///
    /// This should never panic if the connection was properly initialized
    /// through the typestate transitions. Use `try_protocol_version()`
    /// for a fallible version.
    #[must_use]
    pub fn protocol_version(&self) -> &str {
        self.inner
            .protocol_version
            .as_ref()
            .expect("Ready connection must have protocol version")
    }

    /// Try to get the negotiated protocol version.
    ///
    /// Returns `None` if version was not set (should not happen in normal use).
    #[must_use]
    pub fn try_protocol_version(&self) -> Option<&str> {
        self.inner.protocol_version.as_deref()
    }

    /// Start graceful shutdown.
    ///
    /// This transitions from `Ready` to `Closing` state.
    pub async fn shutdown(self) -> Result<Connection<state::Closing>, McpError> {
        Ok(Connection {
            inner: self.inner,
            _state: PhantomData,
        })
    }
}

impl Connection<state::Closing> {
    /// Complete the shutdown and disconnect.
    pub async fn disconnect(self) -> Result<(), McpError> {
        // Clean up resources
        Ok(())
    }
}

/// A state machine wrapper for connections that allows runtime state tracking.
///
/// This provides an alternative to the pure typestate approach when
/// runtime state inspection is needed.
#[derive(Debug)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected(Connection<state::Disconnected>),
    /// Connected but not initialized.
    Connected(Connection<state::Connected>),
    /// In initialization handshake.
    Initializing(Connection<state::Initializing>),
    /// Ready for requests.
    Ready(Connection<state::Ready>),
    /// Closing down.
    Closing(Connection<state::Closing>),
}

impl ConnectionState {
    /// Create a new disconnected connection state.
    #[must_use]
    pub fn new(server_info: ServerInfo, server_capabilities: ServerCapabilities) -> Self {
        Self::Disconnected(Connection::new(server_info, server_capabilities))
    }

    /// Check if the connection is ready for requests.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    /// Check if the connection is disconnected.
    #[must_use]
    pub const fn is_disconnected(&self) -> bool {
        matches!(self, Self::Disconnected(_))
    }

    /// Get the current state name.
    #[must_use]
    pub const fn state_name(&self) -> &'static str {
        match self {
            Self::Disconnected(_) => "Disconnected",
            Self::Connected(_) => "Connected",
            Self::Initializing(_) => "Initializing",
            Self::Ready(_) => "Ready",
            Self::Closing(_) => "Closing",
        }
    }
}

/// Transition events for connection state changes.
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Connection established.
    Connected,
    /// Initialization started.
    InitializeStarted,
    /// Initialization completed successfully.
    InitializeCompleted {
        /// Negotiated protocol version.
        protocol_version: String,
    },
    /// Initialization failed.
    InitializeFailed {
        /// Error message.
        error: String,
    },
    /// Shutdown requested.
    ShutdownRequested,
    /// Connection closed.
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};

    #[test]
    fn test_connection_creation() {
        let info = ServerInfo::new("test", "1.0.0");
        let caps = ServerCapabilities::default();
        let conn: Connection<state::Disconnected> = Connection::new(info, caps);

        assert!(std::any::type_name_of_val(&conn._state).contains("Disconnected"));
    }

    #[tokio::test]
    async fn test_connection_lifecycle() {
        let info = ServerInfo::new("test", "1.0.0");
        let caps = ServerCapabilities::default();

        // Start disconnected
        let conn = Connection::new(info, caps);

        // Connect
        let conn = conn.connect().await.unwrap();

        // Initialize
        let conn = conn.initialize("2025-11-25").await.unwrap();

        // Complete
        let conn = conn
            .complete(ClientCapabilities::default(), "2025-11-25".to_string())
            .await
            .unwrap();

        // Verify ready state
        assert_eq!(conn.protocol_version(), "2025-11-25");

        // Shutdown
        let conn = conn.shutdown().await.unwrap();

        // Disconnect
        conn.disconnect().await.unwrap();
    }

    #[test]
    fn test_connection_state_enum() {
        let info = ServerInfo::new("test", "1.0.0");
        let caps = ServerCapabilities::default();

        let state = ConnectionState::new(info, caps);
        assert!(state.is_disconnected());
        assert!(!state.is_ready());
        assert_eq!(state.state_name(), "Disconnected");
    }
}
