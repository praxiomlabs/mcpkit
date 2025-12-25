# mcpkit-warp

Warp framework integration for mcpkit MCP servers.

## Features

- Full MCP protocol support over HTTP
- Filter-based architecture
- CORS support with configurable origins
- Session management with SSE streaming

## Usage

```rust
use mcpkit_warp::McpRouter;

#[tokio::main]
async fn main() {
    let handler = MyHandler::new();
    let router = McpRouter::new(handler).with_cors();

    // Serve with CORS
    warp::serve(router.into_filter())
        .run(([0, 0, 0, 0], 3000))
        .await;
}
```

## Requirements

- Rust 1.85+
- Warp 0.3+

## License

MIT OR Apache-2.0
