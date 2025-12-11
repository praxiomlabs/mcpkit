# ADR 001: Protocol Version Abstraction Layer

## Status

Proposed

## Context

mcpkit needs to support multiple MCP protocol versions to achieve competitive parity:

| Version | Status | Key Features |
|---------|--------|--------------|
| 2024-11-05 | Original | Baseline MCP spec, HTTP+SSE transport |
| 2025-03-26 | Major Update | OAuth 2.1, Streamable HTTP, JSON-RPC batching, tool annotations, audio content |
| 2025-06-18 | Security Update | Removed batching, elicitation, structured output, resource links, MCP-Protocol-Version header |
| 2025-11-25 | Current | Tasks, tool calling in sampling, server-side agent loops, parallel tool calls |

Currently, mcpkit has version negotiation but no feature gating—all handlers behave identically regardless of negotiated version.

## Decision Drivers

1. **Compatibility**: Servers must communicate with clients at different versions
2. **Simplicity**: Most users shouldn't need to think about versions
3. **Type Safety**: Compile-time guarantees where possible
4. **Performance**: Zero-cost abstractions for version checking
5. **Future-proofing**: Easy to add new versions as MCP evolves

## Considered Options

### Option A: Version-Polymorphic Types (Generics)

```rust
pub struct Tool<V: ProtocolVersion> {
    pub name: String,
    pub description: Option<String>,
    pub annotations: Option<V::ToolAnnotations>,  // Only in 2025-03-26+
}
```

**Pros**: Full type safety, compile-time version selection
**Cons**: Complex generics infect entire codebase, poor ergonomics

### Option B: Feature Flags (Compile-Time)

```rust
#[cfg(feature = "mcp-2025-03-26")]
pub struct ToolAnnotations { ... }

impl Tool {
    #[cfg(feature = "mcp-2025-03-26")]
    pub fn with_annotations(self, annotations: ToolAnnotations) -> Self { ... }
}
```

**Pros**: Zero-cost, no runtime overhead
**Cons**: Can't support multiple versions simultaneously, bad for servers

### Option C: Runtime Version Enum + Capability Gating (Selected)

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProtocolVersion {
    V2024_11_05,  // Original
    V2025_03_26,  // OAuth, Streamable HTTP
    V2025_06_18,  // Elicitation, structured output
    V2025_11_25,  // Tasks, parallel tools
}

impl ProtocolVersion {
    pub fn supports_tool_annotations(&self) -> bool {
        *self >= ProtocolVersion::V2025_03_26
    }

    pub fn supports_elicitation(&self) -> bool {
        *self >= ProtocolVersion::V2025_06_18
    }

    pub fn supports_tasks(&self) -> bool {
        *self >= ProtocolVersion::V2025_11_25
    }
}
```

**Pros**: Runtime flexibility, simple API, easy to extend
**Cons**: Runtime checks (mitigated by inlining)

## Decision

**Option C: Runtime Version Enum + Capability Gating**

This approach:
1. Allows a single server/client binary to support multiple versions
2. Keeps types simple (no generics pollution)
3. Uses `Ord` trait for version comparison (`>=` semantics)
4. Provides clear capability methods for feature detection

## Detailed Design

### 1. Protocol Version Enum

Location: `mcpkit-core/src/protocol_version.rs`

```rust
use std::str::FromStr;

/// MCP Protocol versions in chronological order.
///
/// Ordering: V2024_11_05 < V2025_03_26 < V2025_06_18 < V2025_11_25
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtocolVersion {
    /// Original MCP specification (November 2024)
    V2024_11_05,
    /// OAuth 2.1, Streamable HTTP, tool annotations (March 2025)
    V2025_03_26,
    /// Elicitation, structured output, resource links (June 2025)
    V2025_06_18,
    /// Tasks, parallel tools, server-side agent loops (November 2025)
    V2025_11_25,
}

impl ProtocolVersion {
    /// The latest supported protocol version.
    pub const LATEST: Self = Self::V2025_11_25;

    /// All supported versions in chronological order.
    pub const ALL: &'static [Self] = &[
        Self::V2024_11_05,
        Self::V2025_03_26,
        Self::V2025_06_18,
        Self::V2025_11_25,
    ];

    /// Returns the string representation used in the protocol.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V2024_11_05 => "2024-11-05",
            Self::V2025_03_26 => "2025-03-26",
            Self::V2025_06_18 => "2025-06-18",
            Self::V2025_11_25 => "2025-11-25",
        }
    }
}
```

### 2. Capability Methods

```rust
impl ProtocolVersion {
    // === Transport Features ===

    /// HTTP+SSE transport (original, 2024-11-05)
    pub fn supports_sse_transport(&self) -> bool {
        *self == Self::V2024_11_05
    }

    /// Streamable HTTP transport (2025-03-26+)
    pub fn supports_streamable_http(&self) -> bool {
        *self >= Self::V2025_03_26
    }

    /// JSON-RPC batching (only 2025-03-26)
    pub fn supports_batching(&self) -> bool {
        *self == Self::V2025_03_26
    }

    // === Content Types ===

    /// Audio content type (2025-03-26+)
    pub fn supports_audio_content(&self) -> bool {
        *self >= Self::V2025_03_26
    }

    // === Tool Features ===

    /// Tool annotations (readOnly, destructive) (2025-03-26+)
    pub fn supports_tool_annotations(&self) -> bool {
        *self >= Self::V2025_03_26
    }

    /// Structured tool output (2025-06-18+)
    pub fn supports_structured_tool_output(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    /// Resource links in tool results (2025-06-18+)
    pub fn supports_resource_links(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    // === Authorization ===

    /// OAuth 2.1 authorization (2025-03-26+)
    pub fn supports_oauth(&self) -> bool {
        *self >= Self::V2025_03_26
    }

    /// Protected resource metadata (2025-06-18+)
    pub fn supports_protected_resources(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    // === Server Features ===

    /// Elicitation (server requesting user input) (2025-06-18+)
    pub fn supports_elicitation(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    /// Tasks with async status tracking (2025-11-25+)
    pub fn supports_tasks(&self) -> bool {
        *self >= Self::V2025_11_25
    }

    /// Parallel tool calls (2025-11-25+)
    pub fn supports_parallel_tools(&self) -> bool {
        *self >= Self::V2025_11_25
    }

    /// Server-side agent loops (2025-11-25+)
    pub fn supports_agent_loops(&self) -> bool {
        *self >= Self::V2025_11_25
    }

    // === HTTP Transport ===

    /// Requires MCP-Protocol-Version header (2025-06-18+)
    pub fn requires_version_header(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    // === Schema Features ===

    /// _meta field in messages (2025-06-18+)
    pub fn supports_meta_field(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    /// title field for display names (2025-06-18+)
    pub fn supports_title_field(&self) -> bool {
        *self >= Self::V2025_06_18
    }

    /// context field in CompletionRequest (2025-06-18+)
    pub fn supports_completion_context(&self) -> bool {
        *self >= Self::V2025_06_18
    }
}
```

### 3. Version Negotiation

```rust
impl ProtocolVersion {
    /// Negotiate the best common version.
    ///
    /// Returns the highest version supported by both parties.
    pub fn negotiate(requested: &str, supported: &[Self]) -> Option<Self> {
        let requested = Self::from_str(requested).ok()?;

        // Find highest supported version <= requested
        supported
            .iter()
            .filter(|v| **v <= requested)
            .max()
            .copied()
    }

    /// Check if this version is compatible with another.
    ///
    /// Versions are backward-compatible: newer versions can speak older protocols.
    pub fn is_compatible_with(&self, other: Self) -> bool {
        // Server can handle client at same or older version
        other <= *self
    }
}

impl FromStr for ProtocolVersion {
    type Err = VersionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "2024-11-05" => Ok(Self::V2024_11_05),
            "2025-03-26" => Ok(Self::V2025_03_26),
            "2025-06-18" => Ok(Self::V2025_06_18),
            "2025-11-25" => Ok(Self::V2025_11_25),
            _ => Err(VersionParseError::Unknown(s.to_string())),
        }
    }
}
```

### 4. Context Integration

```rust
// In mcpkit-server/src/context.rs
pub struct Context<'a> {
    // Existing fields...

    /// The negotiated protocol version for this session.
    protocol_version: ProtocolVersion,
}

impl<'a> Context<'a> {
    /// Get the negotiated protocol version.
    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_version
    }

    /// Check if a feature is available in this session.
    ///
    /// # Example
    ///
    /// ```rust
    /// if ctx.protocol_version().supports_elicitation() {
    ///     // Safe to use elicitation
    /// }
    /// ```
}
```

### 5. Optional Type Variations

For types that vary significantly between versions, use optional fields:

```rust
/// Tool definition with version-aware optional fields.
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,

    /// Human-friendly display name (2025-06-18+).
    /// Falls back to `name` for older versions.
    pub title: Option<String>,

    /// Tool behavior annotations (2025-03-26+).
    /// Contains hints like `read_only`, `destructive`, `idempotent`.
    pub annotations: Option<ToolAnnotations>,
}

/// Tool behavior annotations (2025-03-26+).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Whether the tool only reads data (no side effects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,

    /// Whether the tool may have destructive side effects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive: Option<bool>,

    /// Whether the tool is idempotent (same result on repeat calls).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent: Option<bool>,

    /// Whether results may differ between calls (e.g., time-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world: Option<bool>,
}
```

### 6. Serialization Filtering

For responses, filter out unsupported fields:

```rust
impl Tool {
    /// Serialize for a specific protocol version.
    ///
    /// Omits fields not supported by the version.
    pub fn to_value_for_version(&self, version: ProtocolVersion) -> Value {
        let mut obj = json!({
            "name": self.name,
            "inputSchema": self.input_schema,
        });

        if let Some(desc) = &self.description {
            obj["description"] = json!(desc);
        }

        // title: 2025-06-18+
        if version.supports_title_field() {
            if let Some(title) = &self.title {
                obj["title"] = json!(title);
            }
        }

        // annotations: 2025-03-26+
        if version.supports_tool_annotations() {
            if let Some(ann) = &self.annotations {
                obj["annotations"] = serde_json::to_value(ann).unwrap();
            }
        }

        obj
    }
}
```

## Implementation Plan

### Phase 1: Core Types (PV-01, PV-02)
1. Create `ProtocolVersion` enum in `mcpkit-core`
2. Add capability methods
3. Add `FromStr`, `Display`, serde implementations
4. Update `SUPPORTED_VERSIONS` constant

### Phase 2: Version Negotiation (PV-07)
1. Update `negotiate_version()` to use enum
2. Store `ProtocolVersion` in server state
3. Pass version through `Context`

### Phase 3: Type Updates (PV-04, PV-05, PV-06)
1. Add optional fields for version-specific features
2. Add serialization helpers
3. Update Tool, Resource, Prompt types

### Phase 4: Handler Updates
1. Add version checks to handlers
2. Return appropriate errors for unsupported features
3. Update macros to generate version-aware code

### Phase 5: Testing (PV-10, PV-11)
1. Unit tests for version comparison
2. Integration tests for negotiation
3. Cross-version compatibility matrix tests

## Consequences

### Positive
- Single binary supports all versions
- Clear API for feature detection
- Type-safe version comparisons
- Easy to add new versions

### Negative
- Runtime overhead for version checks (minimal, inlined)
- Optional fields increase type complexity
- Must maintain backward compatibility

### Neutral
- Existing code continues to work (defaults to latest)
- Gradual migration possible

## References

- [MCP 2024-11-05 Specification](https://modelcontextprotocol.io/specification/2024-11-05)
- [MCP 2025-03-26 Changelog](https://modelcontextprotocol.io/specification/2025-03-26/changelog)
- [MCP 2025-06-18 Changelog](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-06-18/changelog.mdx)
- [MCP 2025-11-25 Release](https://blog.modelcontextprotocol.io/posts/2025-11-25-first-mcp-anniversary/)
