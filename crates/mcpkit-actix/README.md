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
use mcpkit_actix::{McpConfig, handle_mcp_post, handle_sse};
use mcpkit_server::ServerHandler;
use actix_web::{web, App, HttpServer};

// Your MCP server handler (must implement ServerHandler)
struct MyServer;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Create MCP config with your handler
    let config = McpConfig::new(MyServer);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .route("/mcp", web::post().to(handle_mcp_post::<MyServer>))
            .route("/mcp/sse", web::get().to(handle_sse::<MyServer>))
    })
    .bind("0.0.0.0:3000")?
    .run()
    .await
}
```

## Exports

| Export | Purpose |
|--------|---------|
| `McpConfig` | Configuration for MCP HTTP endpoints |
| `handle_mcp_post` | Handler for POST requests |
| `handle_sse` | Handler for SSE streaming |
| `Session` | Individual client session |
| `SessionManager` | Manages active sessions |
| `SessionStore` | Storage for session data |

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
