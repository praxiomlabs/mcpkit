# mcpkit-actix

Actix-web integration for the Model Context Protocol (MCP).

This crate provides integration between the MCP SDK and the Actix-web framework, making it easy to expose MCP servers over HTTP.

## Features

- HTTP POST endpoint for JSON-RPC messages
- Server-Sent Events (SSE) streaming for notifications
- Session management with automatic cleanup
- Protocol version validation
- CORS support

## Usage

```rust
use mcpkit_actix::{McpRouter, McpState};
use mcpkit_server::ServerHandler;

// Your MCP server handler (must implement ServerHandler)
struct MyServer;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Simplest approach: use McpRouter for stdio-like ergonomics
    McpRouter::new(MyServer)
        .with_cors()
        .with_logging()
        .serve("0.0.0.0:3000")
        .await
}
```

### Integration with Existing App

For more control, integrate MCP routes into an existing Actix-web application:

```rust
use mcpkit_actix::{McpRouter, handle_mcp_post, handle_sse, McpState};
use mcpkit_server::ServerHandler;
use actix_web::{web, App, HttpServer};

struct MyServer;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let router = McpRouter::new(MyServer);

    HttpServer::new(move || {
        App::new()
            .configure(router.configure_app())
            // Add your other routes here
    })
    .bind("0.0.0.0:3000")?
    .run()
    .await
}
```

## Exports

| Export | Purpose |
|--------|---------|
| `McpRouter` | Router builder for MCP endpoints |
| `McpState` | Shared state for MCP handlers |
| `handle_mcp_post` | Handler for POST requests |
| `handle_sse` | Handler for SSE streaming |
| `Session` | Individual client session |
| `SessionManager` | Manages SSE broadcast channels |
| `SessionStore` | Storage for HTTP session data |

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
