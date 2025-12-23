# Getting Started with mcpkit

This guide walks you through creating your first MCP server using the Rust MCP SDK.

## Prerequisites

- Rust 1.85 or later (see [MSRV policy](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/))
- Basic familiarity with async Rust (Tokio)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
mcpkit = "0.3"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

> **Note:** The `mcpkit` facade crate re-exports everything you need. For finer control,
> you can depend on individual crates: `mcpkit-core`, `mcpkit-server`, `mcpkit-transport`, etc.

## Your First MCP Server

Let's create a simple calculator server that exposes add and multiply tools.

### Step 1: Define Your Server

```rust
use mcpkit::prelude::*;

struct Calculator;
```

### Step 2: Add the MCP Server Macro

The `#[mcp_server]` macro transforms your impl block into a full MCP server:

```rust
#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a + b))
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a * b))
    }
}
```

### Step 3: Create the Main Function

```rust
use mcpkit::transport::stdio::StdioTransport;

#[tokio::main]
async fn main() -> Result<(), McpError> {
    let transport = StdioTransport::new();

    let server = ServerBuilder::new(Calculator)
        .with_tools(Calculator)
        .build();

    server.serve(transport).await
}
```

> **Note:** `ServerBuilder` is re-exported via `mcpkit::prelude::*`, and transports are
> available under `mcpkit::transport`.

### Complete Example

```rust
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a + b))
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a * b))
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    let transport = StdioTransport::new();

    let server = ServerBuilder::new(Calculator)
        .with_tools(Calculator)
        .build();

    server.serve(transport).await
}
```

## Running Your Server

```bash
cargo run
```

The server will listen on stdin/stdout for JSON-RPC messages.

## Testing with Claude Desktop

1. Build your server: `cargo build --release`
2. Add it to Claude Desktop's configuration (usually `~/.config/claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "calculator": {
      "command": "/path/to/your/target/release/calculator"
    }
  }
}
```

3. Restart Claude Desktop
4. Try asking Claude: "Using the calculator, what is 42 + 17?"

## Next Steps

- [Adding Resources](./resources.md) - Expose data to AI assistants
- [Adding Prompts](./prompts.md) - Create reusable prompt templates
- [Error Handling](./error-handling.md) - Handle errors gracefully
- [Using Middleware](./middleware.md) - Add logging, timeouts, and retries
- [WebSocket Transport](./transports.md#websocket) - Use WebSocket instead of stdio
