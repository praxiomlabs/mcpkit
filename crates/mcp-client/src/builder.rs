//! Client builder for fluent construction.
//!
//! The [`ClientBuilder`] provides a fluent API for constructing MCP clients
//! with customizable options.

use mcp_core::capability::{ClientCapabilities, ClientInfo};
use mcp_core::error::McpError;
use mcp_transport::Transport;

use crate::client::{initialize, Client};

/// Builder for constructing MCP clients.
///
/// Use this builder to configure and create an MCP client connection.
///
/// # Example
///
/// ```ignore
/// let client = ClientBuilder::new()
///     .name("my-client")
///     .version("1.0.0")
///     .with_sampling()
///     .with_roots()
///     .build(transport)
///     .await?;
/// ```
pub struct ClientBuilder {
    name: String,
    version: String,
    capabilities: ClientCapabilities,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    /// Create a new client builder with default values.
    pub fn new() -> Self {
        Self {
            name: "mcp-client".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: ClientCapabilities::default(),
        }
    }

    /// Set the client name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the client version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Enable sampling capability.
    ///
    /// When enabled, the server can request LLM completions from this client.
    pub fn with_sampling(mut self) -> Self {
        self.capabilities = self.capabilities.with_sampling();
        self
    }

    /// Enable elicitation capability.
    ///
    /// When enabled, the server can request user input from this client.
    pub fn with_elicitation(mut self) -> Self {
        self.capabilities = self.capabilities.with_elicitation();
        self
    }

    /// Enable roots capability.
    ///
    /// When enabled, the client exposes file system roots to the server.
    pub fn with_roots(mut self) -> Self {
        self.capabilities = self.capabilities.with_roots();
        self
    }

    /// Enable roots capability with change notifications.
    pub fn with_roots_and_changes(mut self) -> Self {
        self.capabilities = self.capabilities.with_roots_and_changes();
        self
    }

    /// Set custom capabilities.
    pub fn capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Build and connect the client using the given transport.
    ///
    /// This performs the MCP handshake and returns a connected client.
    ///
    /// # Errors
    ///
    /// Returns an error if the handshake fails or the transport encounters an error.
    pub async fn build<T: Transport>(self, transport: T) -> Result<Client<T>, McpError> {
        let client_info = ClientInfo::new(&self.name, &self.version);
        let init_result = initialize(&transport, &client_info, &self.capabilities).await?;
        Ok(Client::new(transport, init_result, client_info, self.capabilities))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_defaults() {
        let builder = ClientBuilder::new();
        assert_eq!(builder.name, "mcp-client");
        assert!(!builder.capabilities.has_sampling());
        assert!(!builder.capabilities.has_roots());
    }

    #[test]
    fn test_builder_fluent() {
        let builder = ClientBuilder::new()
            .name("test-client")
            .version("1.0.0")
            .with_sampling()
            .with_roots();

        assert_eq!(builder.name, "test-client");
        assert_eq!(builder.version, "1.0.0");
        assert!(builder.capabilities.has_sampling());
        assert!(builder.capabilities.has_roots());
    }
}
