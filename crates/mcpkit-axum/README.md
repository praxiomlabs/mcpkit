# mcpkit-axum

Axum integration for the Model Context Protocol (MCP).

This crate provides integration between the MCP SDK and the Axum web framework, making it easy to expose MCP servers over HTTP.

## Features

- HTTP POST endpoint for JSON-RPC messages
- Server-Sent Events (SSE) streaming for notifications
- Session management with automatic cleanup
- Protocol version validation
- CORS support

## Usage

```rust
use mcpkit_axum::McpRouter;
use mcpkit_server::ServerHandler;

// Your MCP server handler (must implement ServerHandler, ToolHandler, etc.)
// Note: Clone is NOT required - the handler is wrapped in Arc internally.
struct MyServer;

#[tokio::main]
async fn main() {
    // Simplest approach: use McpRouter for stdio-like ergonomics
    McpRouter::new(MyServer)
        .serve("0.0.0.0:3000")
        .await
        .unwrap();
}
```

### Default Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/mcp` | POST | JSON-RPC messages |
| `/mcp/sse` | GET | Server-Sent Events stream |

### Customizing Paths

```rust
McpRouter::new(MyServer)
    .post_path("/api/mcp")
    .sse_path("/api/mcp/sse")
    .serve("0.0.0.0:3000")
    .await
    .unwrap();
```

### Integration with Existing App

For more control, integrate MCP routes into an existing Axum application:

```rust
use mcpkit_axum::McpRouter;
use mcpkit_server::ServerHandler;
use axum::Router;

struct MyServer;

#[tokio::main]
async fn main() {
    // Create MCP router with custom paths to avoid double-nesting
    let mcp_router = McpRouter::new(MyServer)
        .post_path("/")      // Will become /api/mcp when nested
        .sse_path("/sse");   // Will become /api/mcp/sse when nested

    // Build the full application
    let app = Router::new()
        .nest("/api/mcp", mcp_router.into_router());
        // Add your other routes here

    // Run the server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
