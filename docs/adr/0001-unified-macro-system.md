# ADR 0001: Unified Macro System

## Status

Accepted

## Context

The official Rust MCP SDK (rmcp), as of December 2025, requires developers to use 4 separate, interdependent macros to define an MCP server:

1. Manual router field in struct
2. `#[tool_router]` macro
3. `#[tool_handler]` macro
4. Manual `new()` constructor with router initialization

This approach has several drawbacks:

- **High boilerplate**: ~30-40 lines of code for a simple server
- **Easy to make mistakes**: Forgetting any piece breaks compilation
- **Cognitive overhead**: Understanding 4 macros and their interactions
- **Maintenance burden**: Changes require updates in multiple places

## Decision

We implement a **single unified `#[mcp_server]` macro** that:

1. Scans the impl block for `#[tool]`, `#[resource]`, `#[prompt]` attributes
2. Generates all necessary trait implementations automatically
3. Creates routing logic without manual wiring
4. Extracts parameter schemas from function signatures

### Example Comparison

**rmcp (before):**
```rust
#[derive(Clone)]
struct Calculator {
    router: ToolRouter,
}

impl Calculator {
    pub fn new() -> Self {
        let mut router = ToolRouter::new();
        router.add_tool("add", Self::add);
        Self { router }
    }
}

#[tool_handler]
impl Calculator {
    #[tool(description = "Add two numbers")]
    async fn add(&self, params: Parameters<AddParams>) -> ToolResult {
        let params = params.into_inner();
        Ok(ToolOutput::text((params.a + params.b).to_string()))
    }
}

#[derive(Deserialize, JsonSchema)]
struct AddParams { a: f64, b: f64 }
```

**This SDK (after):**
```rust
struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a + b).to_string())
    }
}
```

## Consequences

### Positive

- **Reduced boilerplate** for typical servers
- **Lower barrier to entry** for new developers
- **Single point of documentation** for the macro
- **Consistent patterns** across tools, resources, and prompts
- **Direct parameter extraction** from function signatures

### Negative

- **Macro complexity**: The macro implementation is more complex
- **Debugging difficulty**: Macro errors can be harder to diagnose
- **Magic**: Less explicit about what's generated

### Mitigations

- Provide `debug_expand = true` option to see generated code
- Include comprehensive error messages in macro
- Document what code is generated
- Allow manual trait implementation as escape hatch

## References

- [rmcp documentation](https://github.com/modelcontextprotocol/rust-sdk)
- [Rust macro best practices](https://doc.rust-lang.org/reference/procedural-macros.html)
- [Tower middleware pattern](https://docs.rs/tower/)
