//! Capability flags for MCP clients and servers.
//!
//! Capabilities are negotiated during the initialization handshake.
//! They determine what features are available in the session.

use crate::extension::ExtensionRegistry;
use crate::types::Icon;
use crate::types::meta::Meta;
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
    pub const fn with_tools_and_changes(mut self) -> Self {
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
    pub const fn with_resources_and_subscriptions(mut self) -> Self {
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
    pub const fn with_logging(mut self) -> Self {
        self.logging = Some(LoggingCapability {});
        self
    }

    /// Enable completion support.
    #[must_use]
    pub const fn with_completions(mut self) -> Self {
        self.completions = Some(CompletionCapability {});
        self
    }

    /// Check if tools are supported.
    #[must_use]
    pub const fn has_tools(&self) -> bool {
        self.tools.is_some()
    }

    /// Check if resources are supported.
    #[must_use]
    pub const fn has_resources(&self) -> bool {
        self.resources.is_some()
    }

    /// Check if prompts are supported.
    #[must_use]
    pub const fn has_prompts(&self) -> bool {
        self.prompts.is_some()
    }

    /// Check if tasks are supported.
    #[must_use]
    pub const fn has_tasks(&self) -> bool {
        self.tasks.is_some()
    }

    /// Check if completions are supported.
    #[must_use]
    pub const fn has_completions(&self) -> bool {
        self.completions.is_some()
    }

    /// Check if logging is supported.
    #[must_use]
    pub const fn has_logging(&self) -> bool {
        self.logging.is_some()
    }

    /// Check if resource subscriptions are supported.
    #[must_use]
    pub fn has_resource_subscribe(&self) -> bool {
        self.resources
            .as_ref()
            .and_then(|r| r.subscribe)
            .unwrap_or(false)
    }

    /// Set extensions from an extension registry.
    ///
    /// This populates the `experimental` field with extension declarations.
    ///
    /// # Arguments
    ///
    /// * `registry` - The extension registry containing extensions to advertise
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::capability::ServerCapabilities;
    /// use mcpkit_core::extension::{Extension, ExtensionRegistry};
    ///
    /// let registry = ExtensionRegistry::new()
    ///     .register(Extension::new("io.mcp.apps").with_version("0.1.0"));
    ///
    /// let caps = ServerCapabilities::new()
    ///     .with_tools()
    ///     .with_extensions(registry);
    ///
    /// assert!(caps.has_extension("io.mcp.apps"));
    /// ```
    #[must_use]
    pub fn with_extensions(mut self, registry: ExtensionRegistry) -> Self {
        if !registry.is_empty() {
            self.experimental = Some(registry.to_experimental());
        }
        self
    }

    /// Check if a specific extension is supported.
    ///
    /// # Arguments
    ///
    /// * `name` - The extension name to check
    #[must_use]
    pub fn has_extension(&self, name: &str) -> bool {
        self.experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental)
            .is_some_and(|registry| registry.has(name))
    }

    /// Get the extension registry from capabilities.
    ///
    /// Returns `None` if no extensions are declared or if parsing fails.
    #[must_use]
    pub fn extensions(&self) -> Option<ExtensionRegistry> {
        self.experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental)
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
    pub const fn with_roots_and_changes(mut self) -> Self {
        self.roots = Some(RootsCapability {
            list_changed: Some(true),
        });
        self
    }

    /// Enable sampling support.
    #[must_use]
    pub fn with_sampling(mut self) -> Self {
        self.sampling = Some(SamplingCapability::default());
        self
    }

    /// Enable sampling with tool-use support (declares `sampling.tools`).
    #[must_use]
    pub fn with_sampling_tools(mut self) -> Self {
        let sampling = self
            .sampling
            .get_or_insert_with(SamplingCapability::default);
        sampling.tools = Some(serde_json::json!({}));
        self
    }

    /// Enable sampling with context-inclusion support (declares
    /// `sampling.context`).
    #[must_use]
    pub fn with_sampling_context(mut self) -> Self {
        let sampling = self
            .sampling
            .get_or_insert_with(SamplingCapability::default);
        sampling.context = Some(serde_json::json!({}));
        self
    }

    /// Enable elicitation support (form mode).
    ///
    /// This declares `elicitation: {}`, which is form-capable (the 2025-06-18
    /// behaviour). Use [`with_url_elicitation`](Self::with_url_elicitation) to
    /// additionally declare URL-mode support.
    #[must_use]
    pub fn with_elicitation(mut self) -> Self {
        self.elicitation = Some(ElicitationCapability {
            form: None,
            url: None,
        });
        self
    }

    /// Declare form-mode elicitation support explicitly (`elicitation.form`).
    #[must_use]
    pub fn with_form_elicitation(mut self) -> Self {
        self.elicitation
            .get_or_insert_with(ElicitationCapability::default)
            .form = Some(serde_json::json!({}));
        self
    }

    /// Declare URL-mode elicitation support (`elicitation.url`).
    #[must_use]
    pub fn with_url_elicitation(mut self) -> Self {
        self.elicitation
            .get_or_insert_with(ElicitationCapability::default)
            .url = Some(serde_json::json!({}));
        self
    }

    /// Check if form-mode elicitation is supported.
    ///
    /// True when `elicitation.form` is declared, or when `elicitation` is present
    /// but empty (`{}`), which is form-capable for backwards compatibility.
    #[must_use]
    pub fn has_form_elicitation(&self) -> bool {
        self.elicitation
            .as_ref()
            .is_some_and(ElicitationCapability::has_form)
    }

    /// Check if URL-mode elicitation is supported (`elicitation.url` declared).
    #[must_use]
    pub fn has_url_elicitation(&self) -> bool {
        self.elicitation
            .as_ref()
            .is_some_and(ElicitationCapability::has_url)
    }

    /// Check if roots are supported.
    #[must_use]
    pub const fn has_roots(&self) -> bool {
        self.roots.is_some()
    }

    /// Check if sampling is supported.
    #[must_use]
    pub const fn has_sampling(&self) -> bool {
        self.sampling.is_some()
    }

    /// Whether the client declared tool-use support in sampling
    /// (`sampling.tools`).
    #[must_use]
    pub const fn has_sampling_tools(&self) -> bool {
        matches!(&self.sampling, Some(s) if s.tools.is_some())
    }

    /// Whether the client declared context-inclusion support in sampling
    /// (`sampling.context`).
    #[must_use]
    pub const fn has_sampling_context(&self) -> bool {
        matches!(&self.sampling, Some(s) if s.context.is_some())
    }

    /// Check if elicitation is supported.
    #[must_use]
    pub const fn has_elicitation(&self) -> bool {
        self.elicitation.is_some()
    }

    /// Set extensions from an extension registry.
    ///
    /// This populates the `experimental` field with extension declarations.
    #[must_use]
    pub fn with_extensions(mut self, registry: ExtensionRegistry) -> Self {
        if !registry.is_empty() {
            self.experimental = Some(registry.to_experimental());
        }
        self
    }

    /// Check if a specific extension is supported.
    #[must_use]
    pub fn has_extension(&self, name: &str) -> bool {
        self.experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental)
            .is_some_and(|registry| registry.has(name))
    }

    /// Get the extension registry from capabilities.
    #[must_use]
    pub fn extensions(&self) -> Option<ExtensionRegistry> {
        self.experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental)
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
pub struct SamplingCapability {
    /// Declared support for context inclusion (the `includeContext` parameter).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
    /// Declared support for tool use in sampling requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
}

/// Elicitation capability flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElicitationCapability {
    /// Declared support for form-mode elicitation. An absent `form` and `url`
    /// (an empty `{}`) is treated as form-capable for backwards compatibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form: Option<serde_json::Value>,
    /// Declared support for URL-mode elicitation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<serde_json::Value>,
}

impl ElicitationCapability {
    /// Whether form-mode elicitation is supported (`form` declared, or empty
    /// `{}` which is form-capable for backwards compatibility).
    #[must_use]
    pub const fn has_form(&self) -> bool {
        self.form.is_some() || self.url.is_none()
    }

    /// Whether URL-mode elicitation is supported (`url` declared).
    #[must_use]
    pub const fn has_url(&self) -> bool {
        self.url.is_some()
    }
}

/// Server information provided during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Optional human-readable display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Server version.
    pub version: String,
    /// Protocol version supported.
    #[serde(rename = "protocolVersion", skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Optional icons the client can display for this server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
}

impl ServerInfo {
    /// Create new server info.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            version: version.into(),
            protocol_version: Some(PROTOCOL_VERSION.to_string()),
            icons: None,
        }
    }

    /// Set the server's display title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add an icon the client can display for this server.
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icons.get_or_insert_with(Vec::new).push(icon);
        self
    }

    /// Set the server's icons, replacing any already set.
    #[must_use]
    pub fn icons(mut self, icons: impl IntoIterator<Item = Icon>) -> Self {
        self.icons = Some(icons.into_iter().collect());
        self
    }
}

/// Client information provided during initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name.
    pub name: String,
    /// Optional human-readable display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Client version.
    pub version: String,
    /// Optional icons the server can display for this client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
}

impl ClientInfo {
    /// Create new client info.
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            version: version.into(),
            icons: None,
        }
    }

    /// Set the client's display title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add an icon the server can display for this client.
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icons.get_or_insert_with(Vec::new).push(icon);
        self
    }

    /// Set the client's icons, replacing any already set.
    #[must_use]
    pub fn icons(mut self, icons: impl IntoIterator<Item = Icon>) -> Self {
        self.icons = Some(icons.into_iter().collect());
        self
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
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
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
            meta: None,
        }
    }

    /// Set instructions.
    #[must_use]
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }
}

/// The latest protocol version supported by this implementation.
///
/// This is the preferred version that clients and servers will advertise during initialization.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// All protocol versions supported by this implementation.
///
/// The SDK supports multiple protocol versions for backward compatibility:
/// - `2025-11-25`: Latest version with tasks, parallel tools, agent loops
/// - `2025-06-18`: Elicitation, structured output, resource links
/// - `2025-03-26`: OAuth 2.1, Streamable HTTP, tool annotations, audio
/// - `2024-11-05`: Original MCP specification, widely deployed
///
/// Version negotiation happens during initialization:
/// 1. Client sends its preferred (latest) version
/// 2. Server responds with the same version if supported, or its own preferred version
/// 3. Client must support the server's version or disconnect
///
/// For type-safe version handling, use [`crate::protocol_version::ProtocolVersion`].
///
/// # Example
///
/// ```
/// use mcpkit_core::capability::{SUPPORTED_PROTOCOL_VERSIONS, is_version_supported};
///
/// assert!(is_version_supported("2025-11-25"));
/// assert!(is_version_supported("2025-06-18"));
/// assert!(is_version_supported("2025-03-26"));
/// assert!(is_version_supported("2024-11-05"));
/// assert!(!is_version_supported("1.0.0"));
/// ```
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    "2025-11-25", // Latest - tasks, parallel tools, agent loops
    "2025-06-18", // Elicitation, structured output, resource links
    "2025-03-26", // OAuth 2.1, Streamable HTTP, tool annotations
    "2024-11-05", // Original MCP spec - widely deployed
];

/// Check if a protocol version is supported by this implementation.
///
/// # Arguments
///
/// * `version` - The protocol version string to check
///
/// # Returns
///
/// `true` if the version is in the list of supported versions, `false` otherwise.
///
/// # Example
///
/// ```
/// use mcpkit_core::capability::is_version_supported;
///
/// assert!(is_version_supported("2025-11-25"));
/// assert!(!is_version_supported("0.9.0"));
/// ```
#[must_use]
pub fn is_version_supported(version: &str) -> bool {
    SUPPORTED_PROTOCOL_VERSIONS.contains(&version)
}

/// Negotiate a protocol version between client and server.
///
/// Per the MCP specification:
/// - If the requested version is supported, return it
/// - Otherwise, return the server's preferred (latest) version
///
/// The client is then responsible for determining if it can support
/// the returned version, and disconnecting if not.
///
/// # Arguments
///
/// * `requested_version` - The version requested by the client
///
/// # Returns
///
/// The negotiated protocol version string.
///
/// # Example
///
/// ```
/// use mcpkit_core::capability::{negotiate_version, PROTOCOL_VERSION};
///
/// // Client requests a supported version - gets it back
/// assert_eq!(negotiate_version("2024-11-05"), "2024-11-05");
///
/// // Client requests the latest version - gets it back
/// assert_eq!(negotiate_version("2025-11-25"), "2025-11-25");
///
/// // Client requests unknown version - gets server's preferred version
/// assert_eq!(negotiate_version("1.0.0"), PROTOCOL_VERSION);
/// ```
#[must_use]
pub fn negotiate_version(requested_version: &str) -> &'static str {
    if is_version_supported(requested_version) {
        // Return the requested version if we support it
        SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .find(|&&v| v == requested_version)
            .copied()
            .unwrap_or(PROTOCOL_VERSION)
    } else {
        // Return our preferred (latest) version
        PROTOCOL_VERSION
    }
}

/// Protocol version negotiation result.
///
/// Used internally to track the outcome of version negotiation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionNegotiationResult {
    /// The requested version is supported and will be used.
    Accepted(String),
    /// The requested version is not supported; the server offers an alternative.
    /// Client should check if it supports this alternative version.
    CounterOffer {
        /// The version requested by the client.
        requested: String,
        /// The version offered by the server.
        offered: String,
    },
}

impl VersionNegotiationResult {
    /// Get the effective protocol version to use.
    #[must_use]
    pub fn version(&self) -> &str {
        match self {
            Self::Accepted(v) => v,
            Self::CounterOffer { offered, .. } => offered,
        }
    }

    /// Check if the negotiation was an exact match.
    #[must_use]
    pub const fn is_exact_match(&self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

/// Perform version negotiation and return detailed result.
///
/// This is useful when you need to know whether the negotiation
/// resulted in an exact match or a counter-offer.
///
/// # Arguments
///
/// * `requested_version` - The version requested by the client
///
/// # Returns
///
/// A [`VersionNegotiationResult`] indicating whether the version was
/// accepted or a counter-offer was made.
///
/// # Example
///
/// ```
/// use mcpkit_core::capability::{negotiate_version_detailed, VersionNegotiationResult};
///
/// let result = negotiate_version_detailed("2024-11-05");
/// assert!(result.is_exact_match());
///
/// let result = negotiate_version_detailed("unknown-version");
/// assert!(!result.is_exact_match());
/// ```
#[must_use]
pub fn negotiate_version_detailed(requested_version: &str) -> VersionNegotiationResult {
    if is_version_supported(requested_version) {
        VersionNegotiationResult::Accepted(requested_version.to_string())
    } else {
        VersionNegotiationResult::CounterOffer {
            requested: requested_version.to_string(),
            offered: PROTOCOL_VERSION.to_string(),
        }
    }
}

/// Initialized notification (sent by client after receiving initialize result).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializedNotification {}

/// Ping request for keep-alive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingRequest {}

/// Ping response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingResult {
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn elicitation_form_url_capability_semantics() {
        use super::ClientCapabilities;
        // Empty `{}` (legacy `with_elicitation`) is form-capable, not url.
        let form = ClientCapabilities::default().with_elicitation();
        assert!(form.has_elicitation());
        assert!(form.has_form_elicitation());
        assert!(!form.has_url_elicitation());
        assert_eq!(
            serde_json::to_value(&form.elicitation).unwrap(),
            serde_json::json!({}),
            "empty elicitation must still serialize as {{}} for compatibility"
        );

        // URL-only: url present, form not.
        let url = ClientCapabilities::default().with_url_elicitation();
        assert!(url.has_url_elicitation());
        assert!(!url.has_form_elicitation());

        // Both.
        let both = ClientCapabilities::default()
            .with_form_elicitation()
            .with_url_elicitation();
        assert!(both.has_form_elicitation());
        assert!(both.has_url_elicitation());
    }

    use super::*;

    #[test]
    fn test_server_capabilities_builder() -> Result<(), Box<dyn std::error::Error>> {
        let caps = ServerCapabilities::new()
            .with_tools()
            .with_resources_and_subscriptions()
            .with_prompts()
            .with_tasks();

        assert!(caps.has_tools());
        assert!(caps.has_resources());
        assert!(caps.has_prompts());
        assert!(caps.has_tasks());
        assert!(
            caps.resources
                .ok_or("Expected resources")?
                .subscribe
                .ok_or("Expected subscribe")?
        );
        Ok(())
    }

    #[test]
    fn test_client_capabilities_builder() -> Result<(), Box<dyn std::error::Error>> {
        let caps = ClientCapabilities::new()
            .with_roots_and_changes()
            .with_sampling()
            .with_elicitation();

        assert!(caps.has_roots());
        assert!(caps.has_sampling());
        assert!(caps.has_elicitation());
        assert!(
            caps.roots
                .ok_or("Expected roots")?
                .list_changed
                .ok_or("Expected list_changed")?
        );
        Ok(())
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
        let result =
            InitializeResult::new(server, caps).instructions("Use this server to do things");

        assert_eq!(result.protocol_version, PROTOCOL_VERSION);
        assert!(result.instructions.is_some());
    }

    #[test]
    fn test_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let caps = ServerCapabilities::new()
            .with_tools_and_changes()
            .with_resources();

        let json = serde_json::to_string(&caps)?;
        assert!(json.contains("\"tools\""));
        assert!(json.contains("\"listChanged\":true"));
        Ok(())
    }
}
