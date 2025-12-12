# mcpkit-macros

Procedural macros for the Model Context Protocol (MCP) SDK.

This crate provides the unified `#[mcp_server]` macro that simplifies MCP server development by eliminating boilerplate.

## Overview

The macro system provides:

| Macro | Purpose |
|-------|---------|
| `#[mcp_server]` | Transform an impl block into a full MCP server |
| `#[tool]` | Mark a method as an MCP tool |
| `#[resource]` | Mark a method as an MCP resource handler |
| `#[prompt]` | Mark a method as an MCP prompt handler |

## Usage

```rust
use mcpkit::prelude::*;

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a + b).to_string())
    }

    /// Multiply two numbers
    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a * b).to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    Calculator.serve_stdio().await
}
```

## Attributes

### `#[mcp_server]`

| Attribute | Required | Description |
|-----------|----------|-------------|
| `name` | Yes | Server name |
| `version` | Yes | Server version (can use `env!("CARGO_PKG_VERSION")`) |
| `instructions` | No | Usage instructions sent to clients |
| `capabilities` | No | List of capabilities to advertise |

### `#[tool]`

| Attribute | Required | Description |
|-----------|----------|-------------|
| `description` | Yes | Tool description for clients |
| `name` | No | Override the method name |
| `destructive` | No | Mark tool as destructive |
| `idempotent` | No | Mark tool as idempotent |

## Generated Code

The `#[mcp_server]` macro generates:

1. `impl ServerHandler` with `server_info()` and `capabilities()`
2. `impl ToolHandler` with `list_tools()` and `call_tool()` (if any `#[tool]` methods)
3. `impl ResourceHandler` (if any `#[resource]` methods)
4. `impl PromptHandler` (if any `#[prompt]` methods)
5. A `serve_stdio()` convenience method

## Code Reduction

This single macro replaces multiple manual implementations, significantly reducing boilerplate compared to implementing handler traits directly.

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
