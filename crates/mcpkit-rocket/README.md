# mcpkit-rocket

Rocket framework integration for mcpkit MCP servers.

## Features

- Full MCP protocol support over HTTP
- CORS fairing for cross-origin requests
- Session management with SSE streaming
- Macro-based route generation for type safety

## Usage

```rust
use mcpkit_rocket::{McpRouter, create_mcp_routes};
use rocket::launch;

#[launch]
fn rocket() -> _ {
    let handler = MyHandler::new();

    rocket::build()
        .manage(McpRouter::new(handler).with_cors())
        .mount("/", create_mcp_routes!())
}
```

## Requirements

- Rust 1.85+
- Rocket 0.5+

## License

MIT OR Apache-2.0
