//! Protocol version types and negotiation.
//!
//! This module provides a type-safe representation of MCP protocol versions
//! with capability detection methods for version-specific features.
//!
//! # Protocol Version History
//!
//! | Version | Date | Key Changes |
//! |---------|------|-------------|
//! | 2024-11-05 | Nov 2024 | Initial MCP specification, HTTP+SSE transport |
//! | 2025-03-26 | Mar 2025 | OAuth 2.1, Streamable HTTP, batching, tool annotations, audio |
//! | 2025-06-18 | Jun 2025 | Elicitation, structured output, resource links, removed batching |
//! | 2025-11-25 | Nov 2025 | Tasks, parallel tools, server-side agent loops |
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::protocol_version::ProtocolVersion;
//!
//! // Parse version from string
//! let version: ProtocolVersion = "2025-03-26".parse().unwrap();
//!
//! // Check feature support
//! assert!(version.supports_oauth());
//! assert!(version.supports_tool_annotations());
//! assert!(!version.supports_elicitation()); // Added in 2025-06-18
//!
//! // Version comparison
//! assert!(ProtocolVersion::V2025_11_25 > ProtocolVersion::V2024_11_05);
//! ```

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// MCP protocol versions in chronological order.
///
/// The ordering is: `V2024_11_05 < V2025_03_26 < V2025_06_18 < V2025_11_25`
///
/// This enum implements `Ord`, so you can compare versions:
///
/// ```rust
/// use mcpkit_core::protocol_version::ProtocolVersion;
///
/// assert!(ProtocolVersion::V2025_11_25 > ProtocolVersion::V2024_11_05);
/// assert!(ProtocolVersion::V2025_03_26 >= ProtocolVersion::V2024_11_05);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ProtocolVersion {
    /// Original MCP specification (November 2024).
    ///
    /// Features: HTTP+SSE transport, tools, resources, prompts.
    V2024_11_05,

    /// OAuth 2.1 and Streamable HTTP update (March 2025).
    ///
    /// New features:
    /// - OAuth 2.1 authorization framework
    /// - Streamable HTTP transport (replaces HTTP+SSE)
    /// - JSON-RPC batching (removed in 2025-06-18)
    /// - Tool annotations (readOnly, destructive)
    /// - Audio content type
    /// - Completions capability
    V2025_03_26,

    /// Security and elicitation update (June 2025).
    ///
    /// New features:
    /// - Elicitation (server requesting user input)
    /// - Structured tool output
    /// - Resource links in tool results
    /// - Protected resource metadata
    /// - `_meta` field in messages
    /// - `title` field for display names
    ///
    /// Breaking changes:
    /// - Removed JSON-RPC batching
    /// - MCP-Protocol-Version header required
    V2025_06_18,

    /// Tasks and parallel tools update (November 2025).
    ///
    /// New features:
    /// - Tasks with async status tracking
    /// - Parallel tool calls
    /// - Server-side agent loops
    /// - Tool calling in sampling requests
    V2025_11_25,
}

impl ProtocolVersion {
    /// The latest supported protocol version.
    pub const LATEST: Self = Self::V2025_11_25;

    /// The default version used when not specified.
    pub const DEFAULT: Self = Self::LATEST;

    /// All supported versions in chronological order.
    pub const ALL: &'static [Self] = &[
        Self::V2024_11_05,
        Self::V2025_03_26,
        Self::V2025_06_18,
        Self::V2025_11_25,
    ];

    /// Returns the string representation used in the protocol.
    ///
    /// This matches the format expected in MCP messages (e.g., `"2025-11-25"`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::V2024_11_05 => "2024-11-05",
            Self::V2025_03_26 => "2025-03-26",
            Self::V2025_06_18 => "2025-06-18",
            Self::V2025_11_25 => "2025-11-25",
        }
    }

    // =========================================================================
    // Transport Features
    // =========================================================================

    /// Whether this version uses HTTP+SSE transport (original spec).
    ///
    /// Only `V2024_11_05` uses the original HTTP+SSE transport.
    /// Later versions use Streamable HTTP.
    #[must_use]
    pub const fn supports_sse_transport(&self) -> bool {
        matches!(self, Self::V2024_11_05)
    }

    /// Whether this version supports Streamable HTTP transport.
    ///
    /// Added in 2025-03-26, replacing HTTP+SSE.
    #[must_use]
    pub const fn supports_streamable_http(&self) -> bool {
        matches!(
            self,
            Self::V2025_03_26 | Self::V2025_06_18 | Self::V2025_11_25
        )
    }

    /// Whether this version supports JSON-RPC batching.
    ///
    /// Only available in 2025-03-26. Removed in 2025-06-18.
    #[must_use]
    pub const fn supports_batching(&self) -> bool {
        matches!(self, Self::V2025_03_26)
    }

    /// Whether the MCP-Protocol-Version header is required in HTTP requests.
    ///
    /// Required from 2025-06-18 onwards.
    #[must_use]
    pub const fn requires_version_header(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    // =========================================================================
    // Content Types
    // =========================================================================

    /// Whether this version supports audio content type.
    ///
    /// Added in 2025-03-26.
    #[must_use]
    pub const fn supports_audio_content(&self) -> bool {
        matches!(
            self,
            Self::V2025_03_26 | Self::V2025_06_18 | Self::V2025_11_25
        )
    }

    // =========================================================================
    // Tool Features
    // =========================================================================

    /// Whether this version supports tool annotations.
    ///
    /// Tool annotations describe behavior like `readOnly`, `destructive`, `idempotent`.
    /// Added in 2025-03-26.
    #[must_use]
    pub const fn supports_tool_annotations(&self) -> bool {
        matches!(
            self,
            Self::V2025_03_26 | Self::V2025_06_18 | Self::V2025_11_25
        )
    }

    /// Whether this version supports structured tool output.
    ///
    /// Allows tools to return structured data alongside text.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_structured_tool_output(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports resource links in tool results.
    ///
    /// Allows tool results to reference resources.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_resource_links(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports parallel tool calls.
    ///
    /// Allows multiple tools to be called concurrently.
    /// Added in 2025-11-25.
    #[must_use]
    pub const fn supports_parallel_tools(&self) -> bool {
        matches!(self, Self::V2025_11_25)
    }

    // =========================================================================
    // Authorization Features
    // =========================================================================

    /// Whether this version supports OAuth 2.1 authorization.
    ///
    /// Added in 2025-03-26.
    #[must_use]
    pub const fn supports_oauth(&self) -> bool {
        matches!(
            self,
            Self::V2025_03_26 | Self::V2025_06_18 | Self::V2025_11_25
        )
    }

    /// Whether this version supports protected resource metadata.
    ///
    /// Enables discovery of OAuth authorization servers.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_protected_resources(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    // =========================================================================
    // Server Features
    // =========================================================================

    /// Whether this version supports elicitation.
    ///
    /// Elicitation allows servers to request additional information from users.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_elicitation(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports tasks with async status tracking.
    ///
    /// Tasks allow tracking long-running operations.
    /// Added in 2025-11-25.
    #[must_use]
    pub const fn supports_tasks(&self) -> bool {
        matches!(self, Self::V2025_11_25)
    }

    /// Whether this version supports server-side agent loops.
    ///
    /// Enables sophisticated multi-step reasoning on the server.
    /// Added in 2025-11-25.
    #[must_use]
    pub const fn supports_agent_loops(&self) -> bool {
        matches!(self, Self::V2025_11_25)
    }

    /// Whether this version supports tool calling in sampling requests.
    ///
    /// Allows servers to include tool definitions in sampling.
    /// Added in 2025-11-25.
    #[must_use]
    pub const fn supports_sampling_tools(&self) -> bool {
        matches!(self, Self::V2025_11_25)
    }

    // =========================================================================
    // Schema Features
    // =========================================================================

    /// Whether this version supports the `_meta` field in messages.
    ///
    /// Provides metadata for messages.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_meta_field(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports the `title` field for display names.
    ///
    /// Separate from `name` which is the programmatic identifier.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_title_field(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports the `context` field in completion requests.
    ///
    /// Provides previously-resolved variable values.
    /// Added in 2025-06-18.
    #[must_use]
    pub const fn supports_completion_context(&self) -> bool {
        matches!(self, Self::V2025_06_18 | Self::V2025_11_25)
    }

    /// Whether this version supports the `completions` capability.
    ///
    /// Explicitly indicates support for argument autocompletion.
    /// Added in 2025-03-26.
    #[must_use]
    pub const fn supports_completions_capability(&self) -> bool {
        matches!(
            self,
            Self::V2025_03_26 | Self::V2025_06_18 | Self::V2025_11_25
        )
    }

    // =========================================================================
    // Version Negotiation
    // =========================================================================

    /// Negotiate the best protocol version between requested and supported versions.
    ///
    /// Returns the highest version that is:
    /// 1. Less than or equal to the requested version
    /// 2. In the supported versions list
    ///
    /// Returns `None` if no compatible version exists.
    ///
    /// # Arguments
    ///
    /// * `requested` - The version string requested by the client
    /// * `supported` - List of versions supported by the server
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::protocol_version::ProtocolVersion;
    ///
    /// // Server supports all versions, client requests latest
    /// let negotiated = ProtocolVersion::negotiate(
    ///     "2025-11-25",
    ///     ProtocolVersion::ALL
    /// );
    /// assert_eq!(negotiated, Some(ProtocolVersion::V2025_11_25));
    ///
    /// // Client requests older version
    /// let negotiated = ProtocolVersion::negotiate(
    ///     "2024-11-05",
    ///     ProtocolVersion::ALL
    /// );
    /// assert_eq!(negotiated, Some(ProtocolVersion::V2024_11_05));
    ///
    /// // Client requests unknown future version
    /// let negotiated = ProtocolVersion::negotiate(
    ///     "2026-01-01",
    ///     ProtocolVersion::ALL
    /// );
    /// // Returns latest supported version
    /// assert_eq!(negotiated, Some(ProtocolVersion::V2025_11_25));
    /// ```
    #[must_use]
    pub fn negotiate(requested: &str, supported: &[Self]) -> Option<Self> {
        // Try to parse the requested version
        if let Ok(requested_version) = Self::from_str(requested) {
            // Find the highest supported version <= requested
            supported
                .iter()
                .filter(|v| **v <= requested_version)
                .max()
                .copied()
        } else {
            // Unknown version string - return latest supported
            // This handles future versions gracefully
            supported.iter().max().copied()
        }
    }

    /// Check if this version can communicate with another version.
    ///
    /// Newer servers can communicate with older clients using backward compatibility.
    ///
    /// # Arguments
    ///
    /// * `client_version` - The version the client supports
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::protocol_version::ProtocolVersion;
    ///
    /// let server = ProtocolVersion::V2025_11_25;
    ///
    /// // Server can talk to older clients
    /// assert!(server.is_compatible_with(ProtocolVersion::V2024_11_05));
    ///
    /// // Server can talk to same version
    /// assert!(server.is_compatible_with(ProtocolVersion::V2025_11_25));
    /// ```
    #[must_use]
    pub const fn is_compatible_with(&self, client_version: Self) -> bool {
        // Server can communicate with clients at same or older version
        // Client version must be <= server version
        (client_version as u8) <= (*self as u8)
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing an unknown protocol version string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionParseError {
    /// The unknown version string.
    pub version: String,
}

impl fmt::Display for VersionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown protocol version '{}', supported versions: {:?}",
            self.version,
            ProtocolVersion::ALL
                .iter()
                .map(ProtocolVersion::as_str)
                .collect::<Vec<_>>()
        )
    }
}

impl std::error::Error for VersionParseError {}

impl FromStr for ProtocolVersion {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2024-11-05" => Ok(Self::V2024_11_05),
            "2025-03-26" => Ok(Self::V2025_03_26),
            "2025-06-18" => Ok(Self::V2025_06_18),
            "2025-11-25" => Ok(Self::V2025_11_25),
            _ => Err(VersionParseError {
                version: s.to_string(),
            }),
        }
    }
}

impl From<ProtocolVersion> for String {
    fn from(version: ProtocolVersion) -> Self {
        version.as_str().to_string()
    }
}

impl TryFrom<String> for ProtocolVersion {
    type Error = VersionParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<&str> for ProtocolVersion {
    type Error = VersionParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_ordering() {
        assert!(ProtocolVersion::V2024_11_05 < ProtocolVersion::V2025_03_26);
        assert!(ProtocolVersion::V2025_03_26 < ProtocolVersion::V2025_06_18);
        assert!(ProtocolVersion::V2025_06_18 < ProtocolVersion::V2025_11_25);
    }

    #[test]
    fn test_version_parse() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            "2024-11-05".parse::<ProtocolVersion>()?,
            ProtocolVersion::V2024_11_05
        );
        assert_eq!(
            "2025-03-26".parse::<ProtocolVersion>()?,
            ProtocolVersion::V2025_03_26
        );
        assert_eq!(
            "2025-06-18".parse::<ProtocolVersion>()?,
            ProtocolVersion::V2025_06_18
        );
        assert_eq!(
            "2025-11-25".parse::<ProtocolVersion>()?,
            ProtocolVersion::V2025_11_25
        );
        assert!("unknown".parse::<ProtocolVersion>().is_err());
        Ok(())
    }

    #[test]
    fn test_version_display() {
        assert_eq!(ProtocolVersion::V2024_11_05.to_string(), "2024-11-05");
        assert_eq!(ProtocolVersion::V2025_11_25.to_string(), "2025-11-25");
    }

    #[test]
    fn test_feature_support() {
        // V2024_11_05 - Original features only
        let v1 = ProtocolVersion::V2024_11_05;
        assert!(v1.supports_sse_transport());
        assert!(!v1.supports_streamable_http());
        assert!(!v1.supports_oauth());
        assert!(!v1.supports_elicitation());
        assert!(!v1.supports_tasks());

        // V2025_03_26 - OAuth, Streamable HTTP, etc.
        let v2 = ProtocolVersion::V2025_03_26;
        assert!(!v2.supports_sse_transport());
        assert!(v2.supports_streamable_http());
        assert!(v2.supports_oauth());
        assert!(v2.supports_tool_annotations());
        assert!(v2.supports_batching());
        assert!(!v2.supports_elicitation());

        // V2025_06_18 - Elicitation, no batching
        let v3 = ProtocolVersion::V2025_06_18;
        assert!(v3.supports_oauth());
        assert!(!v3.supports_batching());
        assert!(v3.supports_elicitation());
        assert!(v3.supports_meta_field());
        assert!(!v3.supports_tasks());

        // V2025_11_25 - Tasks, parallel tools
        let v4 = ProtocolVersion::V2025_11_25;
        assert!(v4.supports_elicitation());
        assert!(v4.supports_tasks());
        assert!(v4.supports_parallel_tools());
        assert!(v4.supports_agent_loops());
    }

    #[test]
    fn test_negotiate() {
        let all = ProtocolVersion::ALL;

        // Exact match
        assert_eq!(
            ProtocolVersion::negotiate("2024-11-05", all),
            Some(ProtocolVersion::V2024_11_05)
        );
        assert_eq!(
            ProtocolVersion::negotiate("2025-11-25", all),
            Some(ProtocolVersion::V2025_11_25)
        );

        // Unknown future version - returns latest
        assert_eq!(
            ProtocolVersion::negotiate("2026-01-01", all),
            Some(ProtocolVersion::V2025_11_25)
        );

        // Server only supports old version
        let old_only = &[ProtocolVersion::V2024_11_05];
        assert_eq!(
            ProtocolVersion::negotiate("2025-11-25", old_only),
            Some(ProtocolVersion::V2024_11_05)
        );

        // Empty supported list
        assert_eq!(ProtocolVersion::negotiate("2025-11-25", &[]), None);
    }

    #[test]
    fn test_is_compatible_with() {
        let latest = ProtocolVersion::V2025_11_25;
        let oldest = ProtocolVersion::V2024_11_05;

        // Latest server is compatible with all older versions
        assert!(latest.is_compatible_with(oldest));
        assert!(latest.is_compatible_with(ProtocolVersion::V2025_03_26));
        assert!(latest.is_compatible_with(ProtocolVersion::V2025_06_18));
        assert!(latest.is_compatible_with(latest));

        // Old server can only handle same or older
        assert!(oldest.is_compatible_with(oldest));
        assert!(!oldest.is_compatible_with(latest));
    }

    #[test]
    fn test_serde() -> Result<(), Box<dyn std::error::Error>> {
        let v = ProtocolVersion::V2025_11_25;

        // Serialize to string
        let json = serde_json::to_string(&v)?;
        assert_eq!(json, "\"2025-11-25\"");

        // Deserialize from string
        let parsed: ProtocolVersion = serde_json::from_str("\"2024-11-05\"")?;
        assert_eq!(parsed, ProtocolVersion::V2024_11_05);
        Ok(())
    }
}
