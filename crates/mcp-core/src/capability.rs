//! Capability flags for MCP clients and servers.
//!
//! Capabilities are negotiated during the initialization handshake.
//! They determine what features are available in the session.

use serde::{Deserialize, Serialize};

/// Server capabilities advertised during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tool capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolCapability>,
    /// Resource capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceCapability>,
    /// Prompt capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptCapability>,
    /// Task capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<TaskCapability>,
    /// Logging capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,
    /// Completion capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<CompletionCapability>,
    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<serde_json::Value>,
}

impl ServerCapabilities {
    /// Create empty capabilities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable tool support.
    #[must_use]
    pub fn with_tools(mut self) -> Self {
        self.tools = Some(ToolCapability::default());
        self
    }

    /// Enable tool support with change notifications.
    #[must_use]
    pub fn with_tools_and_changes(mut self) -> Self {
        self.tools = Some(ToolCapability {
            list_changed: Some(true),
        });
        self
    }

    /// Enable resource support.
    #[must_use]
    pub fn with_resources(mut self) -> Self {
        self.resources = Some(ResourceCapability::default());
        self
    }

    /// Enable resource support with subscriptions.
    #[must_use]
    pub fn with_resources_and_subscriptions(mut self) -> Self {
        self.resources = Some(ResourceCapability {
            subscribe: Some(true),
            list_changed: Some(true),
        });
        self
    }

    /// Enable prompt support.
    #[must_use]
    pub fn with_prompts(mut self) -> Self {
        self.prompts = Some(PromptCapability::default());
        self
    }

    /// Enable task support.
    #[must_use]
    pub fn with_tasks(mut self) -> Self {
        self.tasks = Some(TaskCapability::default());
        self
    }

    /// Enable logging support.
    #[must_use]
    pub fn with_logging(mut self) -> Self {
        self.logging = Some(LoggingCapability {});
        self
    }

    /// Enable completion support.
    #[must_use]
    pub fn with_completions(mut self) -> Self {
        self.completions = Some(CompletionCapability {});
        self
    }

    /// Check if tools are supported.
    #[must_use]
    pub fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    /// Check if resources are supported.
    #[must_use]
    pub fn has_resources(&self) -> bool {
        self.resources.is_some()
    }

    /// Check if prompts are supported.
    #[must_use]
    pub fn has_prompts(&self) -> bool {
        self.prompts.is_some()
    }

    /// Check if tasks are supported.
    #[must_use]
    pub fn has_tasks(&self) -> bool {
        self.tasks.is_some()
    }
}

/// Client capabilities advertised during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Roots (file system access) capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    /// Sampling capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,
    /// Elicitation capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,
    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<serde_json::Value>,
}

impl ClientCapabilities {
    /// Create empty capabilities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable roots support.
    #[must_use]
    pub fn with_roots(mut self) -> Self {
        self.roots = Some(RootsCapability::default());
        self
    }

    /// Enable roots support with change notifications.
    #[must_use]
    pub fn with_roots_and_changes(mut self) -> Self {
        self.roots = Some(RootsCapability {
            list_changed: Some(true),
        });
        self
    }

    /// Enable sampling support.
    #[must_use]
    pub fn with_sampling(mut self) -> Self {
        self.sampling = Some(SamplingCapability {});
        self
    }

    /// Enable elicitation support.
    #[must_use]
    pub fn with_elicitation(mut self) -> Self {
        self.elicitation = Some(ElicitationCapability {});
        self
    }

    /// Check if roots are supported.
    #[must_use]
    pub fn has_roots(&self) -> bool {
        self.roots.is_some()
    }

    /// Check if sampling is supported.
    #[must_use]
    pub fn has_sampling(&self) -> bool {
        self.sampling.is_some()
    }

    /// Check if elicitation is supported.
    #[must_use]
    pub fn has_elicitation(&self) -> bool {
        self.elicitation.is_some()
    }
}

/// Tool capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCapability {
    /// If true, the server will send tool list changed notifications.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Resource capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceCapability {
    /// If true, the server supports resource subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// If true, the server will send resource list changed notifications.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Prompt capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptCapability {
    /// If true, the server will send prompt list changed notifications.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Task capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskCapability {
    /// If true, the server supports task cancellation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellable: Option<bool>,
}

/// Logging capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// Completion capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionCapability {}

/// Roots capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootsCapability {
    /// If true, the client will send roots list changed notifications.
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Sampling capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// Elicitation capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElicitationCapability {}

/// Server information provided during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
    /// Protocol version supported.
    #[serde(rename = "protocolVersion", skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
}

impl ServerInfo {
    /// Create new server info.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            protocol_version: Some(PROTOCOL_VERSION.to_string()),
        }
    }
}

/// Client information provided during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name.
    pub name: String,
    /// Client version.
    pub version: String,
}

impl ClientInfo {
    /// Create new client info.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// Initialize request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeRequest {
    /// Protocol version the client supports.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Client capabilities.
    pub capabilities: ClientCapabilities,
    /// Client information.
    #[serde(rename = "clientInfo")]
    pub client_info: ClientInfo,
}

impl InitializeRequest {
    /// Create a new initialize request.
    #[must_use]
    pub fn new(client_info: ClientInfo, capabilities: ClientCapabilities) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities,
            client_info,
        }
    }
}

/// Initialize response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Protocol version the server supports.
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server information.
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    /// Optional instructions for using this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

impl InitializeResult {
    /// Create a new initialize result.
    #[must_use]
    pub fn new(server_info: ServerInfo, capabilities: ServerCapabilities) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities,
            server_info,
            instructions: None,
        }
    }

    /// Set instructions.
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }
}

/// The protocol version supported by this implementation.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Initialized notification (sent by client after receiving initialize result).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializedNotification {}

/// Ping request for keep-alive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingRequest {}

/// Ping response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingResult {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_capabilities_builder() {
        let caps = ServerCapabilities::new()
            .with_tools()
            .with_resources_and_subscriptions()
            .with_prompts()
            .with_tasks();

        assert!(caps.has_tools());
        assert!(caps.has_resources());
        assert!(caps.has_prompts());
        assert!(caps.has_tasks());
        assert!(caps.resources.unwrap().subscribe.unwrap());
    }

    #[test]
    fn test_client_capabilities_builder() {
        let caps = ClientCapabilities::new()
            .with_roots_and_changes()
            .with_sampling()
            .with_elicitation();

        assert!(caps.has_roots());
        assert!(caps.has_sampling());
        assert!(caps.has_elicitation());
        assert!(caps.roots.unwrap().list_changed.unwrap());
    }

    #[test]
    fn test_initialize_request() {
        let client = ClientInfo::new("test-client", "1.0.0");
        let caps = ClientCapabilities::new().with_sampling();
        let request = InitializeRequest::new(client, caps);

        assert_eq!(request.protocol_version, PROTOCOL_VERSION);
        assert_eq!(request.client_info.name, "test-client");
    }

    #[test]
    fn test_initialize_result() {
        let server = ServerInfo::new("test-server", "1.0.0");
        let caps = ServerCapabilities::new().with_tools();
        let result = InitializeResult::new(server, caps)
            .instructions("Use this server to do things");

        assert_eq!(result.protocol_version, PROTOCOL_VERSION);
        assert!(result.instructions.is_some());
    }

    #[test]
    fn test_serialization() {
        let caps = ServerCapabilities::new()
            .with_tools_and_changes()
            .with_resources();

        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"listChanged\":true"));
    }
}
