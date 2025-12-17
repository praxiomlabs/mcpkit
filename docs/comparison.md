# SDK Comparison: mcpkit vs rmcp

This document provides an honest, transparent comparison between `mcpkit` and `rmcp` (the official Rust MCP SDK) to help you choose the right tool for your project.

> **Last verified**: December 2025. SDK ecosystems evolve rapidly—always check the respective repositories for the latest information.

## Executive Summary

| Aspect | mcpkit | rmcp |
|--------|--------|------|
| **Macro Approach** | Single `#[mcp_server]` macro | Multiple macros (`#[tool_router]`, etc.) |
| **Transport Options** | 5 built-in (stdio, WebSocket, HTTP, Unix, Memory) | 2 (stdio, SSE) |
| **Error Handling** | `McpError` + `ToolOutput::error()` | `Result<CallToolResult, Error>` |
| **Maturity** | Pre-1.0 | Established (0.11.x) |
| **Protocol Version** | 2025-11-25 (per docs) | 2024-11-05 (per docs) |

## Code Size Comparison

### Minimal Server: Calculator Tool

#### mcpkit (19 lines core)

```rust
use mcpkit::prelude::*;

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a + b))
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a * b))
    }
}
```

#### rmcp (24 lines core)

```rust
use rmcp::{ErrorData as McpError, model::*, tool_router};
use rmcp::handler::server::tool::ToolRouter;

#[derive(Clone)]
pub struct Calculator {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Calculator {
    pub fn new() -> Self {
        Self { tool_router: Self::tool_router() }
    }

    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            format!("{}", a + b)
        )]))
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            format!("{}", a * b)
        )]))
    }
}
```

### Observations

The examples above illustrate different API design choices:

**mcpkit approach:**
- No router field needed in struct
- No explicit constructor required
- Returns `ToolOutput` directly for simple cases
- Single prelude import

**rmcp approach:**
- Explicit router field for tool registration
- Constructor initializes the router
- Returns `Result<CallToolResult, McpError>`
- Multiple imports

Both approaches are valid. mcpkit prioritizes convenience for simple cases; rmcp provides explicit control. The "right" choice depends on your preferences and requirements.

## Performance Comparison

### Theoretical Analysis

Both SDKs:
- Use the same underlying JSON-RPC protocol
- Serialize to identical wire formats
- Have similar async runtime requirements

Performance differences are negligible for MCP workloads because:
1. **I/O Bound**: MCP is network/IPC bound, not CPU bound
2. **Same Protocol**: Wire format is identical
3. **Macro-Generated**: Both use compile-time code generation

### Benchmark Results (mcpkit)

From our Criterion benchmarks:

| Operation | Time | Notes |
|-----------|------|-------|
| Request serialization (minimal) | ~60ns | JSON-RPC formatting |
| Request serialization (complex) | ~330ns | With nested params |
| Response serialization | ~90ns (simple) to ~7µs (large) | Scales with content |
| Tool lookup | O(1) | HashMap |
| Argument parsing | ~50-250ns | Depends on complexity |
| End-to-end tool call | ~500ns | Excluding I/O |

### Memory

Both SDKs use Rust's ownership system, ensuring no memory leaks by design. mcpkit includes optional connection pooling for long-running servers.

## Feature Comparison

### Transport Support

**mcpkit transports:**

| Transport | Status |
|-----------|--------|
| stdio | Built-in |
| WebSocket | Built-in |
| HTTP/SSE | Built-in |
| Unix sockets | Built-in |
| Memory (testing) | Built-in |

**rmcp transports:** Check [rmcp's repository](https://github.com/modelcontextprotocol/rust-sdk) for current transport support.

### Protocol Features

**mcpkit protocol support:**

| Version | Status |
|---------|--------|
| 2024-11-05 | Supported |
| 2025-03-26 | Supported |
| 2025-06-18 | Supported |
| 2025-11-25 | Supported (default) |

mcpkit includes automatic version negotiation and capability negotiation.

**rmcp protocol support:** Check [rmcp's repository](https://github.com/modelcontextprotocol/rust-sdk) for current protocol version support. As of December 2025, their README references version 2024-11-05.

### mcpkit-Specific Features

These features are available in mcpkit:

| Feature | Description |
|---------|-------------|
| Unified macro | Single `#[mcp_server]` with `#[tool]`, `#[resource]`, `#[prompt]` |
| Built-in schema generation | No external crate required |
| Error suggestions | `ToolOutput::error_with_suggestion()` for LLM recovery hints |
| Typestate builders | Compile-time validation of server configuration |
| Connection pooling | Optional pooling for long-running servers |

Check [rmcp's documentation](https://github.com/modelcontextprotocol/rust-sdk) for its current feature set.

## When to Consider Each SDK

### mcpkit may be a good fit if you:

- Need MCP 2025-11-25 features (Tasks, Elicitation)
- Want multiple transport options (WebSocket, HTTP, Unix sockets)
- Prefer a single macro approach for defining servers
- Need runtime flexibility (async-std, smol support)

### rmcp may be a good fit if you:

- Prefer the official SDK maintained by the MCP community
- Want an established, widely-used implementation
- Have existing rmcp code you don't want to migrate
- Need maximum ecosystem compatibility

### Both SDKs:

- Implement the MCP protocol correctly
- Are wire-compatible with each other
- Produce production-quality Rust code

## Migration Considerations

### From rmcp to mcpkit

- Import changes needed (different module structure)
- Tool definitions need restructuring (different macro approach)
- Error handling patterns differ

See [Migration Guide](migration-from-rmcp.md) for details.

### From mcpkit to rmcp

- Add `tool_router` fields to structs
- Change return types to `Result<CallToolResult, McpError>`
- Add schemars for schema generation

## Other Rust MCP SDKs

The Rust MCP ecosystem includes several implementations beyond mcpkit and rmcp. Here's an overview to help you evaluate alternatives:

| SDK | Protocol Versions | Key Differentiators |
|-----|-------------------|---------------------|
| **[rmcp](https://github.com/modelcontextprotocol/rust-sdk)** | 2024-11-05 (per docs) | Official SDK, wide adoption |
| **[rust-mcp-sdk](https://github.com/rust-mcp-stack/rust-mcp-sdk)** | All versions (default: 2025-06-18) | DNS rebinding protection, OAuth providers, batch messages |
| **[mcp-protocol-sdk](https://docs.rs/mcp-protocol-sdk)** | 2025-06-18 | Audio support, annotations, autocompletion |
| **mcpkit** | All versions (default: 2025-11-25) | Unified macro, runtime-agnostic, Tasks support |

### Choosing an SDK

Each SDK has different strengths:

- **rmcp**: Official SDK with wide ecosystem adoption
- **rust-mcp-sdk**: Multiple OAuth provider integrations (Keycloak, WorkOS, Scalekit)
- **mcp-protocol-sdk**: Focused on 2025-06-18 features
- **mcpkit**: 2025-11-25 support, runtime flexibility, unified macro approach

We encourage you to evaluate based on your specific requirements. All these SDKs implement the same MCP protocol and are wire-compatible.

## Conclusion

Multiple quality Rust MCP SDKs exist, each with different design priorities:

- **rmcp** is the official SDK with established community adoption
- **mcpkit** offers 2025-11-25 protocol support, multiple transports, and runtime flexibility
- **rust-mcp-sdk** and **mcp-protocol-sdk** provide additional options in the ecosystem

Choose based on your requirements: protocol version needs, transport options, runtime constraints, or API preferences. All implement the same underlying protocol and interoperate correctly.

## Appendix: Measuring Code Size

To independently verify these comparisons:

```bash
# Count non-blank, non-comment lines in tool definitions
# Exclude imports and tests

# mcpkit example
cloc --include-lang=Rust examples/minimal-server/src/main.rs

# Compare with an equivalent rmcp implementation
# (Create equivalent file and measure)
```

For fair comparison, count only:
- Struct definitions
- Trait implementations
- Tool method bodies
- Required boilerplate (constructors, fields)

Do NOT count:
- Examples and demonstrations
- Test code
- Comments and documentation
