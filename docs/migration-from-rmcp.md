# Migration Guide: rmcp to mcpkit

This guide helps you migrate from the `rmcp` crate (the official Rust MCP SDK) to `mcpkit`. Both SDKs implement the same MCP protocol and are wire-compatible.

> **Note**: This guide reflects rmcp's API as of December 2025. Check [rmcp's documentation](https://github.com/modelcontextprotocol/rust-sdk) for the latest API details.

## Quick Comparison

| Feature | rmcp | mcpkit |
|---------|------|--------------|
| Tool definition | `#[tool(...)` on impl methods | `#[tool]` on standalone functions |
| Schema generation | `schemars` crate | Built-in JSON Schema |
| Server builder | Custom builder | Typestate builder |
| Error handling | `CallToolResult` | `ToolOutput` + `McpError` |
| Protocol versions | 2024-11-05 | 2024-11-05, 2025-11-25 |
| Transport | stdio, SSE | stdio, WebSocket, HTTP, Unix, Memory |

## Dependency Changes

### Before (rmcp)

```toml
[dependencies]
rmcp = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
schemars = "1"
```

### After (mcpkit)

```toml
[dependencies]
mcpkit = "0.3"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
# schemars no longer required - schemas are built-in
```

## Import Changes

### Before (rmcp)

```rust
use rmcp::{
    Error as McpError,
    model::{
        CallToolResult, Content, ServerCapabilities, ServerInfo,
        Tool, ToolAnnotations,
    },
    server::{Server, ServerHandler},
    tool,
};
```

### After (mcpkit)

```rust
use mcpkit::prelude::*;
// Or individual imports:
use mcpkit::{
    error::McpError,
    capability::{ServerCapabilities, ServerInfo},
    types::{Tool, ToolAnnotations, Content, CallToolResult, ToolOutput},
    server::{ServerBuilder, ToolHandler},
    tool,
};
```

## Tool Definition

### Before (rmcp)

```rust
use rmcp::{tool, model::CallToolResult, server::ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
struct CalculatorInput {
    #[schemars(description = "First number")]
    a: f64,
    #[schemars(description = "Second number")]
    b: f64,
    #[schemars(description = "Operation to perform")]
    operation: String,
}

struct MyServer;

#[tool(name = "calculator", description = "Perform calculations")]
impl MyServer {
    async fn calculator(&self, input: CalculatorInput) -> CallToolResult {
        let result = match input.operation.as_str() {
            "add" => input.a + input.b,
            "sub" => input.a - input.b,
            _ => return CallToolResult::error("Unknown operation"),
        };
        CallToolResult::text(result.to_string())
    }
}
```

### After (mcpkit)

```rust
use mcpkit::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CalculatorInput {
    a: f64,
    b: f64,
    operation: String,
}

#[tool(
    name = "calculator",
    description = "Perform calculations",
    params(
        a(description = "First number"),
        b(description = "Second number"),
        operation(description = "Operation to perform")
    )
)]
async fn calculator(input: CalculatorInput, _ctx: &Context<'_>) -> Result<ToolOutput, McpError> {
    let result = match input.operation.as_str() {
        "add" => input.a + input.b,
        "sub" => input.a - input.b,
        _ => return Ok(ToolOutput::error("Unknown operation")),
    };
    Ok(ToolOutput::text(result.to_string()))
}
```

Key differences:
- Tools are standalone functions, not impl methods
- Schema descriptions are in the macro, not `schemars`
- Returns `Result<ToolOutput, McpError>` instead of `CallToolResult`
- Receives `Context` for accessing capabilities

## Server Setup

### Before (rmcp)

```rust
use rmcp::server::{Server, ServerHandler};
use rmcp::transport::stdio::StdioTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = Server::builder()
        .name("my-server")
        .version("1.0.0")
        .handler(MyHandler::new())
        .build();

    let transport = StdioTransport::new();
    server.run(transport).await?;
    Ok(())
}
```

### After (mcpkit)

```rust
use mcpkit::prelude::*;
use mcpkit_transport::StdioTransport;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = StdioTransport::new();

    let server = ServerBuilder::new("my-server", "1.0.0")
        .with_transport(transport)
        .with_tool_handler(MyToolHandler::new())
        .build();

    server.run().await?;
    Ok(())
}
```

Key differences:
- Transport is passed to builder, not `run()`
- Explicit handler registration by type (tool, resource, prompt)
- Typestate builder ensures all required fields

## Handler Implementation

### Before (rmcp)

```rust
use rmcp::server::ServerHandler;
use rmcp::model::{CallToolResult, Tool};
use async_trait::async_trait;

#[async_trait]
impl ServerHandler for MyServer {
    async fn list_tools(&self) -> Vec<Tool> {
        vec![/* tools */]
    }

    async fn call_tool(&self, name: &str, args: serde_json::Value) -> CallToolResult {
        match name {
            "my_tool" => { /* ... */ }
            _ => CallToolResult::error("Unknown tool"),
        }
    }
}
```

### After (mcpkit)

```rust
use mcpkit::prelude::*;
use mcpkit_server::handler::ToolHandler;

#[async_trait]
impl ToolHandler for MyToolHandler {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![/* tools */])
    }

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "my_tool" => { /* ... */ }
            _ => Err(McpError::method_not_found(name)),
        }
    }
}
```

Key differences:
- Handlers receive `Context` parameter
- Returns `Result` instead of direct value
- Separate traits for tools, resources, prompts

## Error Handling

### Before (rmcp)

```rust
// Tool errors returned as CallToolResult
CallToolResult::error("Something went wrong")

// Protocol errors via Error enum
Err(Error::InvalidParams("missing field".into()))
```

### After (mcpkit)

```rust
// Tool errors via ToolOutput (shown to LLM)
Ok(ToolOutput::error("Something went wrong"))

// Tool errors with suggestion
Ok(ToolOutput::error_with_suggestion(
    "Invalid query",
    "Try using quotation marks"
))

// Protocol errors via McpError
Err(McpError::invalid_params("tools/call", "missing field"))
```

Key differences:
- Clear separation between recoverable (ToolOutput::error) and protocol (McpError) errors
- Error suggestions help LLMs self-correct
- Structured error types map to JSON-RPC codes

## Capabilities

### Before (rmcp)

```rust
let caps = ServerCapabilities {
    tools: Some(ToolsCapability { list_changed: true }),
    ..Default::default()
};
```

### After (mcpkit)

```rust
// Builder pattern
let caps = ServerCapabilities::new()
    .with_tools_and_changes()
    .with_resources_and_subscriptions()
    .with_prompts();

// Or direct construction
let caps = ServerCapabilities {
    tools: Some(ToolCapability { list_changed: Some(true) }),
    ..Default::default()
};
```

## Protocol Version Compatibility

Both SDKs are wire-compatible. mcpkit supports:

- `2024-11-05` (rmcp's version)
- `2025-11-25` (latest)

Version negotiation happens automatically during initialization.

## Transport Differences

### rmcp Transports

- `StdioTransport`
- `SseTransport` (SSE only)

### mcpkit Transports

- `StdioTransport`
- `WebSocketTransport`
- `HttpTransport` (full HTTP + SSE)
- `UnixTransport`
- `MemoryTransport` (for testing)
- `SpawnedTransport` (child process)

## Step-by-Step Migration

### 1. Update Dependencies

Replace `rmcp` with `mcpkit`:

```toml
[dependencies]
- rmcp = "0.1"
+ mcpkit = "0.3"
```

Remove `schemars` if only used for tool schemas.

### 2. Update Imports

Replace rmcp imports with mcpkit equivalents:

```rust
- use rmcp::*;
+ use mcpkit::prelude::*;
```

### 3. Convert Tools

Change tool definitions from impl methods to standalone functions:

```rust
// Before: impl method with #[tool] on impl block
// After: standalone fn with #[tool] macro
```

### 4. Update Handlers

Split `ServerHandler` into separate `ToolHandler`, `ResourceHandler`, `PromptHandler`.

### 5. Update Error Returns

Change `CallToolResult::error(...)` to `Ok(ToolOutput::error(...))`.

### 6. Test Compatibility

Run tests to verify wire format compatibility:

```rust
#[test]
fn test_rmcp_compatibility() {
    // Verify your server responds correctly to rmcp client requests
}
```

## Common Migration Issues

### Schema Differences

If you see schema validation errors, ensure field descriptions are correctly specified in the `#[tool]` macro `params(...)` section.

### Missing Context

If your tools need capability information, use the new `Context` parameter:

```rust
async fn my_tool(args: Args, ctx: &Context<'_>) -> Result<ToolOutput, McpError> {
    if ctx.client_capabilities().has_sampling() {
        // Client supports LLM sampling
    }
}
```

### Error Type Changes

Update error handling from `Error` to `McpError`:

```rust
- Err(Error::InvalidParams(...))
+ Err(McpError::invalid_params("method", ...))
```

## Getting Help

- [GitHub Issues](https://github.com/praxiomlabs/mcpkit/issues)
- [Documentation](https://docs.rs/mcpkit)
- [MCP Specification](https://modelcontextprotocol.io)
