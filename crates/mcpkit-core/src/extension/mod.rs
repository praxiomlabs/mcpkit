//! Protocol extension infrastructure.
//!
//! MCP extensions provide a way to add specialized capabilities that operate
//! outside the core protocol specification. Extensions follow these principles:
//!
//! - **Optional** - Implementations can choose to adopt extensions
//! - **Additive** - Extensions add capabilities without modifying core behavior
//! - **Composable** - Multiple extensions can work together without conflicts
//! - **Versioned** - Extensions can be versioned independently
//!
//! # Usage
//!
//! Extensions are declared in the `experimental` field of capabilities during
//! initialization. Each extension is identified by a unique name (typically
//! using reverse-domain notation) and can include version and configuration.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::extension::{Extension, ExtensionRegistry};
//! use mcpkit_core::capability::ServerCapabilities;
//!
//! // Define an extension
//! let healthcare = Extension::new("io.health.fhir")
//!     .with_version("1.0.0")
//!     .with_config(serde_json::json!({
//!         "fhir_version": "R4",
//!         "resources": ["Patient", "Observation"]
//!     }));
//!
//! // Create a registry with multiple extensions
//! let registry = ExtensionRegistry::new()
//!     .register(healthcare)
//!     .register(Extension::new("io.mcp.apps").with_version("0.1.0"));
//!
//! // Apply to capabilities
//! let caps = ServerCapabilities::new()
//!     .with_tools()
//!     .with_extensions(registry);
//!
//! // Check for extension support
//! assert!(caps.has_extension("io.health.fhir"));
//! ```
//!
//! # Standard Extensions
//!
//! The following extension namespaces are reserved:
//!
//! | Namespace | Description |
//! |-----------|-------------|
//! | `io.mcp.*` | Official MCP extensions (e.g., `io.mcp.apps`) |
//! | `io.anthropic.*` | Anthropic-specific extensions |
//! | `io.openai.*` | OpenAI-specific extensions |
//!
//! Third-party extensions should use reverse-domain notation (e.g., `com.example.myext`).
//!
//! # References
//!
//! - [MCP Extensions](https://modelcontextprotocol.io/specification/2025-11-25/extensions)
//! - [SEP-1865: MCP Apps Extension](https://github.com/modelcontextprotocol/ext-apps)
//!
//! # Submodules
//!
//! - [`apps`] - MCP Apps extension for interactive UIs (SEP-1865)
//! - [`discovery`] - Extension discovery and negotiation utilities
//! - [`templates`] - Domain-specific extension templates (healthcare, finance, `IoT`)

pub mod apps;
pub mod discovery;
pub mod templates;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A protocol extension declaration.
///
/// Extensions are identified by a unique name (typically reverse-domain notation)
/// and can include version information and custom configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    /// Extension name (e.g., "io.mcp.apps", "com.example.myext").
    pub name: String,

    /// Extension version (semver recommended).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Extension-specific configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Extension {
    /// Create a new extension with the given name.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique extension identifier (reverse-domain notation recommended)
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::extension::Extension;
    ///
    /// let ext = Extension::new("com.example.myext");
    /// assert_eq!(ext.name, "com.example.myext");
    /// ```
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            config: None,
            description: None,
        }
    }

    /// Set the extension version.
    ///
    /// # Arguments
    ///
    /// * `version` - Semver version string (e.g., "1.0.0")
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set extension-specific configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - JSON configuration object
    #[must_use]
    pub fn with_config(mut self, config: serde_json::Value) -> Self {
        self.config = Some(config);
        self
    }

    /// Set a human-readable description.
    ///
    /// # Arguments
    ///
    /// * `description` - Extension description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Registry of protocol extensions.
///
/// The registry provides a structured way to declare and manage multiple
/// extensions. It serializes to a format compatible with the `experimental`
/// capabilities field.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtensionRegistry {
    /// Registered extensions indexed by name.
    extensions: HashMap<String, Extension>,
}

impl ExtensionRegistry {
    /// Create a new empty extension registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an extension.
    ///
    /// If an extension with the same name already exists, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `extension` - The extension to register
    #[must_use]
    pub fn register(mut self, extension: Extension) -> Self {
        self.extensions.insert(extension.name.clone(), extension);
        self
    }

    /// Check if an extension is registered.
    ///
    /// # Arguments
    ///
    /// * `name` - Extension name to check
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.extensions.contains_key(name)
    }

    /// Get an extension by name.
    ///
    /// # Arguments
    ///
    /// * `name` - Extension name to retrieve
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Extension> {
        self.extensions.get(name)
    }

    /// Get all registered extension names.
    #[must_use]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.extensions.keys().map(String::as_str)
    }

    /// Get the number of registered extensions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.extensions.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extensions.is_empty()
    }

    /// Convert the registry to a JSON value for the `experimental` field.
    ///
    /// The output format is:
    /// ```json
    /// {
    ///   "extensions": {
    ///     "io.mcp.apps": { "version": "0.1.0", ... },
    ///     "com.example.myext": { "version": "1.0.0", ... }
    ///   }
    /// }
    /// ```
    #[must_use]
    pub fn to_experimental(&self) -> serde_json::Value {
        serde_json::json!({
            "extensions": self.extensions
        })
    }

    /// Parse an extension registry from an `experimental` field value.
    ///
    /// # Arguments
    ///
    /// * `experimental` - The experimental field value from capabilities
    ///
    /// # Returns
    ///
    /// An extension registry, or `None` if parsing fails.
    #[must_use]
    pub fn from_experimental(experimental: &serde_json::Value) -> Option<Self> {
        let extensions = experimental.get("extensions")?.as_object()?;
        let mut registry = Self::new();

        for (name, value) in extensions {
            if let Ok(mut ext) = serde_json::from_value::<Extension>(value.clone()) {
                ext.name.clone_from(name);
                registry = registry.register(ext);
            }
        }

        Some(registry)
    }
}

/// Known extension namespaces.
pub mod namespaces {
    /// Official MCP extensions namespace.
    pub const MCP: &str = "io.mcp";

    /// MCP Apps extension (SEP-1865).
    pub const MCP_APPS: &str = "io.mcp.apps";

    /// Anthropic-specific extensions.
    pub const ANTHROPIC: &str = "io.anthropic";

    /// OpenAI-specific extensions.
    pub const OPENAI: &str = "io.openai";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_builder() {
        let ext = Extension::new("com.example.test")
            .with_version("1.0.0")
            .with_description("Test extension")
            .with_config(serde_json::json!({"enabled": true}));

        assert_eq!(ext.name, "com.example.test");
        assert_eq!(ext.version, Some("1.0.0".to_string()));
        assert_eq!(ext.description, Some("Test extension".to_string()));
        assert!(ext.config.is_some());
    }

    #[test]
    fn test_extension_registry() {
        let registry = ExtensionRegistry::new()
            .register(Extension::new("io.mcp.apps").with_version("0.1.0"))
            .register(Extension::new("com.example.ext").with_version("2.0.0"));

        assert!(registry.has("io.mcp.apps"));
        assert!(registry.has("com.example.ext"));
        assert!(!registry.has("unknown"));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_experimental_roundtrip() {
        let registry =
            ExtensionRegistry::new().register(Extension::new("io.mcp.apps").with_version("0.1.0"));

        let json = registry.to_experimental();
        let parsed = ExtensionRegistry::from_experimental(&json).unwrap();

        assert!(parsed.has("io.mcp.apps"));
        assert_eq!(
            parsed.get("io.mcp.apps").unwrap().version,
            Some("0.1.0".to_string())
        );
    }

    #[test]
    fn test_serialization() {
        let ext = Extension::new("test").with_version("1.0.0");
        let json = serde_json::to_string(&ext).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"version\":\"1.0.0\""));
    }
}
