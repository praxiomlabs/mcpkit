# mcpkit

[![CI](https://github.com/praxiomlabs/mcpkit/actions/workflows/ci.yml/badge.svg)](https://github.com/praxiomlabs/mcpkit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/mcpkit.svg)](https://crates.io/crates/mcpkit)
[![Documentation](https://docs.rs/mcpkit/badge.svg)](https://docs.rs/mcpkit)
[![License](https://img.shields.io/crates/l/mcpkit.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
[![MCP Protocol](https://img.shields.io/badge/MCP-2025--11--25-green.svg)](https://modelcontextprotocol.io/specification/2025-11-25)

A Rust SDK for the Model Context Protocol (MCP) that simplifies server development through a unified `#[mcp_server]` macro.

## Features

- **Unified `#[mcp_server]` macro** for defining tools, resources, and prompts
- **Runtime-agnostic** async support (Tokio, smol)
- **Typestate builders** for compile-time validation of server configuration
- **Context-based error handling** with `McpError` and `.context()` chains
- **MCP 2025-11-25 protocol** including Tasks, Elicitation, and OAuth 2.1
- **Tower-compatible middleware** via built-in Layer pattern

## Why mcpkit?

mcpkit implements **MCP 2025-11-25** — the latest protocol specification. As of December 2025, the official `rmcp` SDK documentation references protocol version 2024-11-05 (always [verify current status](https://github.com/modelcontextprotocol/rust-sdk)). mcpkit supports the newest MCP features:

| Feature | Added In | Description |
|---------|----------|-------------|
| **Tasks** | 2025-11-25 | Long-running operations with progress tracking and cancellation |
| **Elicitation** | 2025-06-18 | Server-initiated requests for user input |
| **OAuth 2.1** | 2025-03-26 | Modern authentication with mandatory PKCE |
| **Tool Annotations** | 2025-03-26 | `readOnly`, `destructive`, `idempotent` hints for tools |
| **Structured Output** | 2025-06-18 | Type-safe JSON responses with schema validation |

See the [detailed comparison](docs/comparison.md) for a full overview of Rust MCP SDK options.

## Quick Start

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
mcpkit = "0.3"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

Create a simple MCP server:

```rust
use mcpkit::prelude::*;
use mcpkit::transport::stdio::StdioTransport;

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a + b).to_string())
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a * b).to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Create transport - you choose the runtime!
    let transport = StdioTransport::new();

    // Build server with registered handlers
    let server = ServerBuilder::new(Calculator)
        .with_tools(Calculator)
        .build();

    // Serve on the transport
    server.serve(transport).await
}
```

> **Note:** This SDK is runtime-agnostic. You provide the transport, which lets you use
> Tokio, smol, or any other async runtime. The examples use Tokio, but the SDK itself
> doesn't depend on any specific runtime.

## mcpkit Highlights

| Feature | Description |
|---------|-------------|
| **Protocol** | MCP 2025-11-25 (supports all 4 protocol versions) |
| **Macro** | Single `#[mcp_server]` with `#[tool]`, `#[resource]`, `#[prompt]` |
| **Parameters** | Extracted directly from function signatures |
| **Errors** | Unified `McpError` with `.context()` chains |
| **Transports** | stdio, WebSocket, HTTP/SSE, Unix sockets |
| **Middleware** | Built-in Tower-compatible Layer system |
| **Runtime** | Agnostic (Tokio, smol) |

For comparisons with other Rust MCP SDKs (rmcp, rust-mcp-sdk, mcp-protocol-sdk), see the [detailed comparison](docs/comparison.md).

## Crate Structure

```
mcpkit/
├── mcpkit/                     # Facade crate (use this)
├── crates/
│   ├── mcpkit-core/            # Protocol types, traits
│   ├── mcpkit-transport/       # Transport abstractions
│   │   ├── stdio               # Standard I/O transport
│   │   ├── http                # Streamable HTTP transport
│   │   ├── websocket           # WebSocket transport
│   │   └── unix                # Unix domain sockets
│   ├── mcpkit-server/          # Server implementation
│   ├── mcpkit-client/          # Client implementation
│   ├── mcpkit-macros/          # Procedural macros
│   ├── mcpkit-testing/         # Test utilities
│   ├── mcpkit-axum/            # Axum web framework integration
│   └── mcpkit-actix/           # Actix-web framework integration
└── examples/                   # Example servers
```

## Examples

### Minimal Server

```rust
use mcpkit::prelude::*;

struct MyServer;

#[mcp_server(name = "minimal", version = "1.0.0")]
impl MyServer {
    #[tool(description = "Say hello")]
    async fn hello(&self, name: Option<String>) -> ToolOutput {
        let name = name.unwrap_or_else(|| "World".to_string());
        ToolOutput::text(format!("Hello, {}!", name))
    }
}
```

### With Resources

```rust
use mcpkit::prelude::*;

struct ConfigServer;

#[mcp_server(name = "config", version = "1.0.0")]
impl ConfigServer {
    #[resource(
        uri_pattern = "config://app/{key}",
        name = "Configuration",
        mime_type = "application/json"
    )]
    async fn get_config(&self, uri: &str) -> ResourceContents {
        ResourceContents::text(uri, r#"{"debug": true}"#)
    }
}
```

### With Prompts

```rust
use mcpkit::prelude::*;

struct PromptServer;

#[mcp_server(name = "prompts", version = "1.0.0")]
impl PromptServer {
    #[prompt(description = "Review code for issues")]
    async fn code_review(&self, code: String, language: Option<String>) -> GetPromptResult {
        let lang = language.unwrap_or_else(|| "unknown".to_string());
        GetPromptResult {
            description: Some("Code review prompt".to_string()),
            messages: vec![
                PromptMessage::user(format!(
                    "Please review the following {} code:\n```{}\n{}```",
                    lang, lang, code
                ))
            ],
        }
    }
}
```

## Transports

The SDK is runtime-agnostic. You choose the transport and the async runtime.

### Standard I/O

```rust
use mcpkit_transport::stdio::StdioTransport;

let transport = StdioTransport::new();
```

### HTTP (Streamable)

```rust
use mcpkit_transport::http::HttpTransport;

let transport = HttpTransport::new(HttpTransportConfig::new("http://localhost:8080"));
```

### WebSocket

```rust
use mcpkit_transport::websocket::WebSocketTransport;

let transport = WebSocketTransport::new(WebSocketConfig::new("ws://localhost:9000"));
```

### Unix Domain Socket (Unix only)

```rust
#[cfg(unix)]
use mcpkit_transport::unix::UnixTransport;

#[cfg(unix)]
let transport = UnixTransport::new("/tmp/mcp.sock");
```

## Middleware

```rust
use mcpkit_transport::stdio::StdioTransport;
use mcpkit_transport::middleware::{LoggingLayer, TimeoutLayer, LayerStack};
use std::time::Duration;
use log::Level;

let transport = StdioTransport::new();
let stack = LayerStack::new(transport)
    .with(LoggingLayer::new(Level::Debug))
    .with(TimeoutLayer::new(Duration::from_secs(30)));
```

## Error Handling

```rust
use mcpkit::prelude::*;

fn process() -> Result<(), McpError> {
    let result = something_risky()
        .context("while processing request")?;
    Ok(())
}
```

## Protocol Version

This SDK implements MCP protocol version **2025-11-25**.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Documentation

- [Getting Started](docs/getting-started.md)
- [Working with Tools](docs/tools.md)
- [Working with Resources](docs/resources.md)
- [Working with Prompts](docs/prompts.md)
- [Error Handling](docs/error-handling.md)
- [Using Middleware](docs/middleware.md)
- [Transport Options](docs/transports.md)
- [Architecture Decision Records](docs/adr/)

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) before submitting a Pull Request.

## Security

For security issues, please see our [Security Policy](SECURITY.md).
