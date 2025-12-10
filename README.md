# Rust MCP SDK

[![CI](https://github.com/anthropics/rust-mcp-sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/anthropics/rust-mcp-sdk/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/mcp.svg)](https://crates.io/crates/mcp)
[![Documentation](https://docs.rs/mcp/badge.svg)](https://docs.rs/mcp)
[![License](https://img.shields.io/crates/l/mcp.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/MSRV-1.75-blue.svg)](https://blog.rust-lang.org/2023/12/28/Rust-1.75.0.html)

A production-grade Rust SDK for the Model Context Protocol (MCP) that dramatically reduces boilerplate compared to rmcp through a unified `#[mcp_server]` macro.

## Features

- **66% less boilerplate** via unified `#[mcp_server]` macro
- **Runtime-agnostic** async support (Tokio, async-std, smol)
- **Type-safe state machines** via typestate pattern for connection lifecycle
- **Rich error handling** with context chains
- **Full MCP 2025-11-25 protocol coverage** including Tasks (which rmcp lacks)
- **First-class middleware** via Tower-compatible Layer pattern

## Quick Start

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
mcp = { path = "mcp" }  # or from crates.io once published
mcp-server = { path = "crates/mcp-server" }
mcp-transport = { path = "crates/mcp-transport" }
tokio = { version = "1.0", features = ["full"] }
serde_json = "1"
```

Create a simple MCP server:

```rust
use mcp::prelude::*;
use mcp_server::ServerBuilder;
use mcp_transport::stdio::StdioTransport;

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
> Tokio, async-std, smol, or any other async runtime. The examples use Tokio, but the
> SDK itself doesn't depend on any specific runtime.

## Comparison with rmcp

| Aspect | rmcp | This SDK |
|--------|------|----------|
| Macros | 4 interdependent | 1 unified `#[mcp_server]` |
| Boilerplate | Manual router wiring | Zero initialization |
| Parameters | `Parameters<T>` wrapper | Direct from signature |
| Error types | 3 nested layers | 1 unified `McpError` |
| Tasks | Not implemented | Full support |
| WebSocket | Custom implementation | First-class |
| Middleware | Manual/Tower separate | Built-in Layer system |
| Runtime | Tokio-only | Runtime-agnostic |

## Crate Structure

```
rust-mcp-sdk/
├── mcp/                    # Facade crate (use this!)
├── crates/
│   ├── mcp-core/           # Protocol types, traits (no async runtime)
│   ├── mcp-transport/      # Transport abstractions
│   │   ├── stdio           # Standard I/O transport
│   │   ├── http            # Streamable HTTP transport
│   │   ├── websocket       # WebSocket transport
│   │   └── unix            # Unix domain sockets
│   ├── mcp-server/         # Server implementation
│   ├── mcp-client/         # Client implementation
│   ├── mcp-macros/         # Procedural macros
│   └── mcp-testing/        # Test utilities
└── examples/               # Example servers
```

## Examples

### Minimal Server

```rust
use mcp::prelude::*;

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
use mcp::prelude::*;

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
use mcp::prelude::*;

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
use mcp_transport::stdio::StdioTransport;

let transport = StdioTransport::new();
```

### HTTP (Streamable)

```rust
use mcp_transport::http::HttpTransport;

let transport = HttpTransport::new(HttpTransportConfig::new("http://localhost:8080"));
```

### WebSocket

```rust
use mcp_transport::websocket::WebSocketTransport;

let transport = WebSocketTransport::new(WebSocketConfig::new("ws://localhost:9000"));
```

### Unix Domain Socket (Unix only)

```rust
#[cfg(unix)]
use mcp_transport::unix::UnixTransport;

#[cfg(unix)]
let transport = UnixTransport::new("/tmp/mcp.sock");
```

## Middleware

```rust
use mcp_transport::stdio::StdioTransport;
use mcp_transport::middleware::{LoggingLayer, TimeoutLayer, LayerStack};
use std::time::Duration;
use log::Level;

let transport = StdioTransport::new();
let stack = LayerStack::new(transport)
    .with(LoggingLayer::new(Level::Debug))
    .with(TimeoutLayer::new(Duration::from_secs(30)));
```

## Error Handling

```rust
use mcp::prelude::*;

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
