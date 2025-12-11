# SDK Comparison: mcpkit vs rmcp

This document provides an honest, transparent comparison between `mcpkit` and `rmcp` (the official Rust MCP SDK) to help you choose the right tool for your project.

## Executive Summary

| Aspect | mcpkit | rmcp |
|--------|--------------|------|
| **Code Size** | ~15% less boilerplate | Baseline |
| **Macro Approach** | Single unified macro | Multiple specialized macros |
| **Transport Options** | 5 transports | 2 transports |
| **Error Handling** | Two-tier (recoverable + fatal) | Single result type |
| **Maturity** | New (pre-1.0) | Established |
| **Best For** | Ergonomic tool development | Proven, minimal API |

**Important Note**: The previously claimed "66% less boilerplate" is overstated. Realistic measurements show approximately 15-25% reduction for typical use cases.

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

### Analysis

| Metric | mcpkit | rmcp | Difference |
|--------|--------------|------|------------|
| Core tool code lines | 19 | 24 | -21% |
| Required struct fields | 0 | 1 (tool_router) | -100% |
| Constructor boilerplate | 0 lines | 3 lines | -100% |
| Return type verbosity | `ToolOutput` | `Result<CallToolResult, McpError>` | Simpler |
| Imports | 1 | 3 | -67% |

The reduction comes primarily from:
1. No `tool_router` field requirement
2. No explicit constructor needed
3. Simpler return type for success cases
4. Unified prelude import

### Where rmcp Has Less Code

For complex scenarios with custom error handling:

```rust
// rmcp - error handling is more explicit
async fn risky_op(&self) -> Result<CallToolResult, McpError> {
    self.do_work()?;  // ? operator works directly
    Ok(CallToolResult::success(vec![...]))
}

// mcpkit - requires wrapping
async fn risky_op(&self) -> Result<ToolOutput, McpError> {
    self.do_work()?;
    Ok(ToolOutput::text("done"))
}
```

Both are similar for complex error handling scenarios.

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

### Memory Comparison

| Metric | mcpkit | rmcp (estimated) |
|--------|--------------|------------------|
| Per-request overhead | ~200-500 bytes | Similar |
| Per-tool registry cost | ~500 bytes | Similar |
| Connection pool | Configurable | N/A (no built-in pool) |

Both use Rust's ownership system, ensuring no memory leaks by design.

## Feature Comparison

### Transport Support

| Transport | mcpkit | rmcp |
|-----------|--------------|------|
| stdio | Yes | Yes |
| WebSocket | Yes | No |
| HTTP/SSE | Yes | SSE only |
| Unix sockets | Yes | No |
| Memory (testing) | Yes | No |

### Protocol Features

| Feature | mcpkit | rmcp |
|---------|--------------|------|
| 2024-11-05 version | Yes | Yes |
| 2025-03-26 version | Yes | Yes |
| 2025-06-18 version | Yes | Yes |
| Version negotiation | Automatic | Manual |
| Capability negotiation | Built-in | Built-in |

### Developer Experience

| Feature | mcpkit | rmcp |
|---------|--------------|------|
| Unified macro | `#[mcp_server]` | Multiple (`#[tool_router]`, `#[tool]`) |
| Schema generation | Built-in | Via schemars |
| Error suggestions | Yes (`ToolOutput::error_with_suggestion`) | No |
| Typestate builders | Yes | No |
| Connection pooling | Yes | No |

## Tradeoffs

### Choose mcpkit If:

1. **You want simpler tool definitions** - Single macro, direct parameters
2. **You need multiple transports** - WebSocket, HTTP, Unix sockets built-in
3. **You value error suggestions** - Help LLMs recover from errors
4. **You're building long-running servers** - Connection pooling, memory management

### Choose rmcp If:

1. **You prefer the official SDK** - Maintained by Anthropic/community
2. **You want minimal dependencies** - Focused, smaller crate
3. **You're already using it** - Migration has cost
4. **You need maximum ecosystem compatibility** - More examples/resources

### Neither Is Clearly "Better"

Both SDKs:
- Implement the same MCP protocol correctly
- Are wire-compatible with each other
- Have similar runtime performance
- Are production-quality Rust code

## Migration Considerations

### From rmcp to mcpkit

Effort: **Medium** (1-2 hours for small projects)

- Import changes are straightforward
- Tool definitions need restructuring
- Error handling patterns differ slightly

See [Migration Guide](migration-from-rmcp.md) for details.

### From mcpkit to rmcp

Effort: **Medium**

- Add `tool_router` fields to structs
- Change return types to `Result<CallToolResult, McpError>`
- Add schemars for schema generation

## Conclusion

**mcpkit** provides a more ergonomic API with additional transports and features, but the "66% less boilerplate" claim in documentation should be revised to a more accurate "15-25% reduction for typical cases."

**rmcp** is a solid, well-maintained official SDK that prioritizes simplicity and minimal API surface.

Both are excellent choices. Pick based on your specific needs rather than benchmarks or code size metrics—these differences are marginal in practice.

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
