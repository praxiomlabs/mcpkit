//! MCP Apps Extension (SEP-1865).
//!
//! This module implements support for the MCP Apps extension, which enables
//! MCP servers to deliver interactive user interfaces to hosts.
//!
//! # Overview
//!
//! MCP Apps extends the protocol with:
//! - UI resources using the `ui://` URI scheme
//! - Tool metadata linking tools to UI templates
//! - Bidirectional communication between UIs and hosts
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::extension::apps::{UiResource, ToolUiMeta, AppsConfig};
//! use mcpkit_core::extension::{Extension, ExtensionRegistry};
//!
//! // Define a UI resource
//! let chart_ui = UiResource::new("ui://charts/bar-chart", "Bar Chart Viewer")
//!     .with_description("Interactive bar chart visualization");
//!
//! // Link a tool to the UI
//! let meta = ToolUiMeta::new("ui://charts/bar-chart");
//!
//! // Configure the apps extension
//! let apps = AppsConfig::new()
//!     .with_sandbox_permissions(vec!["allow-scripts".to_string()]);
//!
//! // Register the extension
//! let registry = ExtensionRegistry::new()
//!     .register(apps.into_extension());
//! ```
//!
//! # Security
//!
//! All UI content runs in sandboxed iframes with restricted permissions.
//! The extension supports configurable sandbox permissions for different
//! security requirements.
//!
//! # References
//!
//! - [SEP-1865: MCP Apps](https://github.com/modelcontextprotocol/ext-apps)
//! - [MCP Apps Blog Post](https://blog.modelcontextprotocol.io/posts/2025-11-21-mcp-apps/)

use serde::{Deserialize, Serialize};

use super::{Extension, namespaces};

/// The MCP Apps extension version.
pub const APPS_VERSION: &str = "0.1.0";

/// MIME type for MCP HTML content.
pub const MIME_TYPE_HTML_MCP: &str = "text/html+mcp";

/// Standard MIME type for HTML content.
pub const MIME_TYPE_HTML: &str = "text/html";

/// A UI resource declaration.
///
/// UI resources use the `ui://` URI scheme and contain HTML content
/// that can be rendered in sandboxed iframes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiResource {
    /// The UI resource URI (e.g., `ui://charts/bar-chart`).
    pub uri: String,

    /// Human-readable name for the UI.
    pub name: String,

    /// MIME type (typically "text/html" or "text/html+mcp").
    #[serde(default = "default_mime_type")]
    pub mime_type: String,

    /// Optional description of the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_mime_type() -> String {
    MIME_TYPE_HTML.to_string()
}

impl UiResource {
    /// Create a new UI resource.
    ///
    /// # Arguments
    ///
    /// * `uri` - The UI resource URI (should use `ui://` scheme)
    /// * `name` - Human-readable name
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::extension::apps::UiResource;
    ///
    /// let ui = UiResource::new("ui://widgets/counter", "Counter Widget");
    /// assert!(ui.uri.starts_with("ui://"));
    /// ```
    #[must_use]
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            mime_type: MIME_TYPE_HTML.to_string(),
            description: None,
        }
    }

    /// Set the MIME type.
    ///
    /// # Arguments
    ///
    /// * `mime_type` - The MIME type (e.g., "text/html+mcp")
    #[must_use]
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = mime_type.into();
        self
    }

    /// Set the description.
    ///
    /// # Arguments
    ///
    /// * `description` - Human-readable description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Check if this resource uses the MCP-enhanced HTML MIME type.
    #[must_use]
    pub fn is_mcp_html(&self) -> bool {
        self.mime_type == MIME_TYPE_HTML_MCP
    }

    /// Validate the URI scheme.
    ///
    /// Returns `true` if the URI uses the `ui://` scheme.
    #[must_use]
    pub fn has_valid_scheme(&self) -> bool {
        self.uri.starts_with("ui://")
    }
}

/// Tool metadata for UI linking.
///
/// This metadata is included in the `_meta` field of tool definitions
/// to link tools to UI resources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolUiMeta {
    /// The UI resource URI to render for this tool.
    #[serde(rename = "ui/resourceUri")]
    pub resource_uri: String,

    /// Optional display hints for the host.
    #[serde(rename = "ui/displayHints", skip_serializing_if = "Option::is_none")]
    pub display_hints: Option<UiDisplayHints>,
}

impl ToolUiMeta {
    /// Create new tool UI metadata.
    ///
    /// # Arguments
    ///
    /// * `resource_uri` - The UI resource URI to link
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::extension::apps::ToolUiMeta;
    ///
    /// let meta = ToolUiMeta::new("ui://charts/bar-chart");
    /// ```
    #[must_use]
    pub fn new(resource_uri: impl Into<String>) -> Self {
        Self {
            resource_uri: resource_uri.into(),
            display_hints: None,
        }
    }

    /// Set display hints.
    ///
    /// # Arguments
    ///
    /// * `hints` - Display hints for the host
    #[must_use]
    pub fn with_display_hints(mut self, hints: UiDisplayHints) -> Self {
        self.display_hints = Some(hints);
        self
    }

    /// Convert to a JSON value for inclusion in tool `_meta`.
    #[must_use]
    pub fn to_meta_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

/// Display hints for UI rendering.
///
/// Hosts may use these hints to determine how to display the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiDisplayHints {
    /// Suggested width in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,

    /// Suggested height in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,

    /// Whether the UI should be resizable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resizable: Option<bool>,

    /// Display mode preference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<UiDisplayMode>,
}

impl UiDisplayHints {
    /// Create new display hints.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the suggested size.
    #[must_use]
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Set whether the UI is resizable.
    #[must_use]
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = Some(resizable);
        self
    }

    /// Set the display mode.
    #[must_use]
    pub fn with_mode(mut self, mode: UiDisplayMode) -> Self {
        self.mode = Some(mode);
        self
    }
}

/// UI display mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UiDisplayMode {
    /// Inline display within the conversation.
    #[default]
    Inline,

    /// Modal/popup display.
    Modal,

    /// Sidebar display.
    Sidebar,

    /// Full-screen display.
    Fullscreen,
}

/// MCP Apps extension configuration.
///
/// This structure configures the Apps extension capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsConfig {
    /// Whether UI resources are supported.
    #[serde(default = "default_true")]
    pub ui_resources: bool,

    /// Sandbox permissions for iframes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sandbox_permissions: Vec<String>,

    /// Maximum UI content size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_size: Option<usize>,

    /// Allowed MIME types.
    #[serde(
        default = "default_allowed_mime_types",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_mime_types: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_allowed_mime_types() -> Vec<String> {
    vec![MIME_TYPE_HTML.to_string(), MIME_TYPE_HTML_MCP.to_string()]
}

impl AppsConfig {
    /// Create a new Apps configuration with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set sandbox permissions.
    ///
    /// # Arguments
    ///
    /// * `permissions` - List of iframe sandbox permissions
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::extension::apps::AppsConfig;
    ///
    /// let config = AppsConfig::new()
    ///     .with_sandbox_permissions(vec![
    ///         "allow-scripts".to_string(),
    ///         "allow-forms".to_string(),
    ///     ]);
    /// ```
    #[must_use]
    pub fn with_sandbox_permissions(mut self, permissions: Vec<String>) -> Self {
        self.sandbox_permissions = permissions;
        self
    }

    /// Set maximum content size.
    ///
    /// # Arguments
    ///
    /// * `size` - Maximum size in bytes
    #[must_use]
    pub fn with_max_content_size(mut self, size: usize) -> Self {
        self.max_content_size = Some(size);
        self
    }

    /// Set allowed MIME types.
    ///
    /// # Arguments
    ///
    /// * `types` - List of allowed MIME types
    #[must_use]
    pub fn with_allowed_mime_types(mut self, types: Vec<String>) -> Self {
        self.allowed_mime_types = types;
        self
    }

    /// Convert to an Extension for registration.
    #[must_use]
    pub fn into_extension(self) -> Extension {
        Extension::new(namespaces::MCP_APPS)
            .with_version(APPS_VERSION)
            .with_description("MCP Apps Extension for interactive UIs")
            .with_config(serde_json::to_value(self).unwrap_or_default())
    }
}

/// UI content for rendering.
///
/// This represents the actual HTML content of a UI resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiContent {
    /// The HTML content.
    pub html: String,

    /// Optional inline styles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub styles: Option<String>,

    /// Optional inline scripts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<String>,
}

impl UiContent {
    /// Create new UI content.
    ///
    /// # Arguments
    ///
    /// * `html` - The HTML content
    #[must_use]
    pub fn new(html: impl Into<String>) -> Self {
        Self {
            html: html.into(),
            styles: None,
            scripts: None,
        }
    }

    /// Add inline styles.
    #[must_use]
    pub fn with_styles(mut self, styles: impl Into<String>) -> Self {
        self.styles = Some(styles.into());
        self
    }

    /// Add inline scripts.
    #[must_use]
    pub fn with_scripts(mut self, scripts: impl Into<String>) -> Self {
        self.scripts = Some(scripts.into());
        self
    }

    /// Render the complete HTML document.
    ///
    /// Combines HTML, styles, and scripts into a complete document.
    #[must_use]
    pub fn render(&self) -> String {
        let mut doc = String::new();

        doc.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        doc.push_str("<meta charset=\"utf-8\">\n");
        doc.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");

        if let Some(ref styles) = self.styles {
            doc.push_str("<style>\n");
            doc.push_str(styles);
            doc.push_str("\n</style>\n");
        }

        doc.push_str("</head>\n<body>\n");
        doc.push_str(&self.html);

        if let Some(ref scripts) = self.scripts {
            doc.push_str("\n<script>\n");
            doc.push_str(scripts);
            doc.push_str("\n</script>\n");
        }

        doc.push_str("\n</body>\n</html>");
        doc
    }
}

/// Builder for creating UI-enabled tools.
///
/// This builder helps create tools that are linked to UI resources.
#[derive(Debug, Clone)]
pub struct UiToolBuilder {
    name: String,
    description: Option<String>,
    ui_resource_uri: String,
    display_hints: Option<UiDisplayHints>,
    fallback_text: Option<String>,
}

impl UiToolBuilder {
    /// Create a new UI tool builder.
    ///
    /// # Arguments
    ///
    /// * `name` - Tool name
    /// * `ui_resource_uri` - The UI resource URI
    #[must_use]
    pub fn new(name: impl Into<String>, ui_resource_uri: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            ui_resource_uri: ui_resource_uri.into(),
            display_hints: None,
            fallback_text: None,
        }
    }

    /// Set the tool description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set display hints.
    #[must_use]
    pub fn with_display_hints(mut self, hints: UiDisplayHints) -> Self {
        self.display_hints = Some(hints);
        self
    }

    /// Set fallback text for non-UI clients.
    #[must_use]
    pub fn with_fallback_text(mut self, text: impl Into<String>) -> Self {
        self.fallback_text = Some(text.into());
        self
    }

    /// Build the tool UI metadata.
    #[must_use]
    pub fn build_meta(&self) -> ToolUiMeta {
        let mut meta = ToolUiMeta::new(&self.ui_resource_uri);
        if let Some(ref hints) = self.display_hints {
            meta = meta.with_display_hints(hints.clone());
        }
        meta
    }

    /// Get the tool name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the description.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Get the fallback text.
    #[must_use]
    pub fn fallback_text(&self) -> Option<&str> {
        self.fallback_text.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_resource() {
        let ui = UiResource::new("ui://charts/bar", "Bar Chart")
            .with_description("A bar chart")
            .with_mime_type(MIME_TYPE_HTML_MCP);

        assert_eq!(ui.uri, "ui://charts/bar");
        assert_eq!(ui.name, "Bar Chart");
        assert!(ui.is_mcp_html());
        assert!(ui.has_valid_scheme());
    }

    #[test]
    fn test_tool_ui_meta() {
        let meta = ToolUiMeta::new("ui://widgets/counter")
            .with_display_hints(UiDisplayHints::new().with_size(400, 300));

        let value = meta.to_meta_value();
        assert!(value.get("ui/resourceUri").is_some());
        assert!(value.get("ui/displayHints").is_some());
    }

    #[test]
    fn test_apps_config() {
        let config = AppsConfig::new()
            .with_sandbox_permissions(vec!["allow-scripts".to_string()])
            .with_max_content_size(1024 * 1024);

        let ext = config.into_extension();
        assert_eq!(ext.name, namespaces::MCP_APPS);
        assert_eq!(ext.version, Some(APPS_VERSION.to_string()));
    }

    #[test]
    fn test_ui_content_render() {
        let content = UiContent::new("<div>Hello</div>")
            .with_styles("body { margin: 0; }")
            .with_scripts("console.log('loaded');");

        let html = content.render();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<div>Hello</div>"));
        assert!(html.contains("body { margin: 0; }"));
        assert!(html.contains("console.log('loaded');"));
    }

    #[test]
    fn test_ui_tool_builder() {
        let builder = UiToolBuilder::new("chart", "ui://charts/bar")
            .with_description("Display a bar chart")
            .with_display_hints(UiDisplayHints::new().with_mode(UiDisplayMode::Modal))
            .with_fallback_text("Chart displayed");

        assert_eq!(builder.name(), "chart");
        assert_eq!(builder.description(), Some("Display a bar chart"));

        let meta = builder.build_meta();
        assert_eq!(meta.resource_uri, "ui://charts/bar");
    }

    #[test]
    fn test_serialization() {
        let meta = ToolUiMeta::new("ui://test");
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("ui/resourceUri"));

        let parsed: ToolUiMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.resource_uri, "ui://test");
    }
}
