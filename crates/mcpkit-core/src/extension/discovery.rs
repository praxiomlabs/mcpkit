//! Extension discovery mechanism.
//!
//! This module provides utilities for discovering and querying extensions
//! from capabilities. Extension discovery allows clients to:
//!
//! - Detect which extensions a server supports
//! - Query extension versions and configurations
//! - Check for required vs optional extensions
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::extension::{Extension, ExtensionRegistry};
//! use mcpkit_core::extension::discovery::{ExtensionQuery, ExtensionRequirement};
//! use mcpkit_core::capability::ServerCapabilities;
//!
//! // Server declares extensions
//! let registry = ExtensionRegistry::new()
//!     .register(Extension::new("io.mcp.apps").with_version("0.1.0"))
//!     .register(Extension::new("io.example.custom").with_version("1.0.0"));
//!
//! let caps = ServerCapabilities::new()
//!     .with_tools()
//!     .with_extensions(registry);
//!
//! // Client queries for extensions
//! let query = ExtensionQuery::new()
//!     .require("io.mcp.apps")
//!     .optional("io.example.custom")
//!     .optional("io.example.missing");
//!
//! let result = query.check(&caps);
//! assert!(result.is_satisfied());
//! assert!(result.has("io.mcp.apps"));
//! assert!(result.has("io.example.custom"));
//! assert!(!result.has("io.example.missing"));
//! ```

use super::{Extension, ExtensionRegistry};
use crate::capability::{ClientCapabilities, ServerCapabilities};
use std::collections::HashMap;

/// Extension requirement level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionRequirement {
    /// Extension must be present for the query to be satisfied.
    Required,
    /// Extension is optional; its absence doesn't fail the query.
    Optional,
}

/// A query for checking extension support.
///
/// Build a query with required and optional extensions, then check
/// against capabilities to see which are satisfied.
#[derive(Debug, Clone, Default)]
pub struct ExtensionQuery {
    requirements: HashMap<String, ExtensionRequirement>,
    min_versions: HashMap<String, String>,
}

impl ExtensionQuery {
    /// Create a new empty extension query.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a required extension.
    ///
    /// The query will only be satisfied if this extension is present.
    #[must_use]
    pub fn require(mut self, name: impl Into<String>) -> Self {
        self.requirements
            .insert(name.into(), ExtensionRequirement::Required);
        self
    }

    /// Add an optional extension.
    ///
    /// The query can be satisfied even if this extension is absent.
    #[must_use]
    pub fn optional(mut self, name: impl Into<String>) -> Self {
        self.requirements
            .insert(name.into(), ExtensionRequirement::Optional);
        self
    }

    /// Require a minimum version for an extension.
    ///
    /// The extension must be present with at least this version.
    #[must_use]
    pub fn with_min_version(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        let name = name.into();
        // Ensure the extension is in requirements
        self.requirements
            .entry(name.clone())
            .or_insert(ExtensionRequirement::Required);
        self.min_versions.insert(name, version.into());
        self
    }

    /// Check the query against server capabilities.
    #[must_use]
    pub fn check(&self, capabilities: &ServerCapabilities) -> ExtensionQueryResult {
        let registry = capabilities
            .experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental);

        let mut found = HashMap::new();
        let mut missing_required = Vec::new();
        let mut version_mismatches = Vec::new();

        for (name, requirement) in &self.requirements {
            let extension = registry.as_ref().and_then(|r| r.get(name));

            if let Some(ext) = extension {
                // Check version if specified
                if let Some(min_version) = self.min_versions.get(name) {
                    if let Some(ref actual_version) = ext.version {
                        if !version_satisfies(actual_version, min_version) {
                            version_mismatches.push(VersionMismatch {
                                extension: name.clone(),
                                required: min_version.clone(),
                                actual: actual_version.clone(),
                            });
                            continue;
                        }
                    }
                }
                found.insert(name.clone(), ext.clone());
            } else if *requirement == ExtensionRequirement::Required {
                missing_required.push(name.clone());
            }
        }

        ExtensionQueryResult {
            found,
            missing_required,
            version_mismatches,
        }
    }

    /// Check the query against client capabilities.
    #[must_use]
    pub fn check_client(&self, capabilities: &ClientCapabilities) -> ExtensionQueryResult {
        let registry = capabilities
            .experimental
            .as_ref()
            .and_then(ExtensionRegistry::from_experimental);

        let mut found = HashMap::new();
        let mut missing_required = Vec::new();
        let mut version_mismatches = Vec::new();

        for (name, requirement) in &self.requirements {
            let extension = registry.as_ref().and_then(|r| r.get(name));

            if let Some(ext) = extension {
                if let Some(min_version) = self.min_versions.get(name) {
                    if let Some(ref actual_version) = ext.version {
                        if !version_satisfies(actual_version, min_version) {
                            version_mismatches.push(VersionMismatch {
                                extension: name.clone(),
                                required: min_version.clone(),
                                actual: actual_version.clone(),
                            });
                            continue;
                        }
                    }
                }
                found.insert(name.clone(), ext.clone());
            } else if *requirement == ExtensionRequirement::Required {
                missing_required.push(name.clone());
            }
        }

        ExtensionQueryResult {
            found,
            missing_required,
            version_mismatches,
        }
    }
}

/// Result of an extension query.
#[derive(Debug, Clone)]
pub struct ExtensionQueryResult {
    /// Extensions that were found.
    pub found: HashMap<String, Extension>,
    /// Required extensions that were missing.
    pub missing_required: Vec<String>,
    /// Extensions with version mismatches.
    pub version_mismatches: Vec<VersionMismatch>,
}

impl ExtensionQueryResult {
    /// Check if the query is satisfied (all required extensions present).
    #[must_use]
    pub fn is_satisfied(&self) -> bool {
        self.missing_required.is_empty() && self.version_mismatches.is_empty()
    }

    /// Check if a specific extension was found.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.found.contains_key(name)
    }

    /// Get a found extension by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Extension> {
        self.found.get(name)
    }

    /// Get the list of found extension names.
    #[must_use]
    pub fn found_names(&self) -> impl Iterator<Item = &str> {
        self.found.keys().map(String::as_str)
    }
}

/// Version mismatch information.
#[derive(Debug, Clone)]
pub struct VersionMismatch {
    /// Extension name.
    pub extension: String,
    /// Required minimum version.
    pub required: String,
    /// Actual version found.
    pub actual: String,
}

/// Simple semver-like version comparison.
///
/// Returns true if `actual` >= `required`.
fn version_satisfies(actual: &str, required: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let parts: Vec<&str> = s.split('.').collect();
        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };

    let actual = parse(actual);
    let required = parse(required);

    actual >= required
}

/// Negotiate extensions between client and server.
///
/// Returns the set of extensions both sides support.
#[must_use]
pub fn negotiate_extensions(
    client: &ClientCapabilities,
    server: &ServerCapabilities,
) -> ExtensionRegistry {
    let client_registry = client
        .experimental
        .as_ref()
        .and_then(ExtensionRegistry::from_experimental);
    let server_registry = server
        .experimental
        .as_ref()
        .and_then(ExtensionRegistry::from_experimental);

    let mut result = ExtensionRegistry::new();

    // Only include extensions both sides support
    if let (Some(client_reg), Some(server_reg)) = (client_registry, server_registry) {
        for name in client_reg.names() {
            if let Some(server_ext) = server_reg.get(name) {
                // Use server's extension info (version, config)
                result = result.register(server_ext.clone());
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_query_satisfied() {
        let registry = ExtensionRegistry::new()
            .register(Extension::new("io.mcp.apps").with_version("0.1.0"))
            .register(Extension::new("io.example.custom").with_version("1.0.0"));

        let caps = ServerCapabilities::new().with_extensions(registry);

        let query = ExtensionQuery::new()
            .require("io.mcp.apps")
            .optional("io.example.custom");

        let result = query.check(&caps);
        assert!(result.is_satisfied());
        assert!(result.has("io.mcp.apps"));
        assert!(result.has("io.example.custom"));
    }

    #[test]
    fn test_extension_query_missing_required() {
        let registry =
            ExtensionRegistry::new().register(Extension::new("io.mcp.apps").with_version("0.1.0"));

        let caps = ServerCapabilities::new().with_extensions(registry);

        let query = ExtensionQuery::new()
            .require("io.mcp.apps")
            .require("io.example.missing");

        let result = query.check(&caps);
        assert!(!result.is_satisfied());
        assert!(
            result
                .missing_required
                .contains(&"io.example.missing".to_string())
        );
    }

    #[test]
    fn test_extension_query_optional_missing() {
        let registry =
            ExtensionRegistry::new().register(Extension::new("io.mcp.apps").with_version("0.1.0"));

        let caps = ServerCapabilities::new().with_extensions(registry);

        let query = ExtensionQuery::new()
            .require("io.mcp.apps")
            .optional("io.example.missing");

        let result = query.check(&caps);
        assert!(result.is_satisfied());
        assert!(!result.has("io.example.missing"));
    }

    #[test]
    fn test_version_satisfies() {
        assert!(version_satisfies("1.0.0", "1.0.0"));
        assert!(version_satisfies("1.1.0", "1.0.0"));
        assert!(version_satisfies("2.0.0", "1.0.0"));
        assert!(!version_satisfies("0.9.0", "1.0.0"));
        assert!(!version_satisfies("1.0.0", "1.0.1"));
    }

    #[test]
    fn test_version_requirement() {
        let registry =
            ExtensionRegistry::new().register(Extension::new("io.mcp.apps").with_version("0.1.0"));

        let caps = ServerCapabilities::new().with_extensions(registry);

        // Version satisfied
        let query = ExtensionQuery::new().with_min_version("io.mcp.apps", "0.1.0");
        assert!(query.check(&caps).is_satisfied());

        // Version not satisfied
        let query = ExtensionQuery::new().with_min_version("io.mcp.apps", "0.2.0");
        let result = query.check(&caps);
        assert!(!result.is_satisfied());
        assert_eq!(result.version_mismatches.len(), 1);
    }

    #[test]
    fn test_negotiate_extensions() {
        let client_registry = ExtensionRegistry::new()
            .register(Extension::new("io.mcp.apps").with_version("0.1.0"))
            .register(Extension::new("io.client.only").with_version("1.0.0"));

        let server_registry = ExtensionRegistry::new()
            .register(Extension::new("io.mcp.apps").with_version("0.2.0"))
            .register(Extension::new("io.server.only").with_version("1.0.0"));

        let client_caps = ClientCapabilities::new().with_extensions(client_registry);
        let server_caps = ServerCapabilities::new().with_extensions(server_registry);

        let negotiated = negotiate_extensions(&client_caps, &server_caps);

        // Only common extension should be present
        assert!(negotiated.has("io.mcp.apps"));
        assert!(!negotiated.has("io.client.only"));
        assert!(!negotiated.has("io.server.only"));

        // Server's version should be used
        assert_eq!(
            negotiated.get("io.mcp.apps").unwrap().version,
            Some("0.2.0".to_string())
        );
    }
}
