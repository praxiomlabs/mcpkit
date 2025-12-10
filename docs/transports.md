# Transport Options

The Rust MCP SDK supports multiple transport mechanisms for different deployment scenarios.

## Standard I/O (stdio)

The most common transport for subprocess-based MCP servers:

```rust
use mcp_transport::stdio::StdioTransport;

let transport = StdioTransport::new();
```

### Use Cases
- Claude Desktop integration
- Local CLI tools
- Subprocess communication

### Configuration

```rust
use mcp_transport::stdio::StdioTransportBuilder;

let transport = StdioTransportBuilder::new()
    .buffer_size(8192)
    .build();
```

## HTTP with Server-Sent Events

For web-based deployments:

```rust
use mcp_transport::http::{HttpTransport, HttpTransportConfig};

// Client
let config = HttpTransportConfig::new("http://localhost:8080/mcp");
let transport = HttpTransport::new(config);

// Server
use mcp_transport::http::HttpTransportListener;

let listener = HttpTransportListener::bind("0.0.0.0:8080").await?;
while let Some(transport) = listener.accept().await? {
    tokio::spawn(async move {
        server.serve(transport).await
    });
}
```

### Configuration Options

```rust
use mcp_transport::http::HttpTransportBuilder;

let transport = HttpTransportBuilder::new("http://localhost:8080")
    .timeout(Duration::from_secs(30))
    .header("Authorization", "Bearer token123")
    .build();
```

### Features
- Request/response over HTTP POST
- Server-Sent Events for server-to-client messages
- Automatic reconnection
- Custom headers support

## WebSocket

For bidirectional real-time communication:

```rust
use mcp_transport::websocket::{WebSocketTransport, WebSocketConfig};

let config = WebSocketConfig::new("ws://localhost:9000");
let transport = WebSocketTransport::new(config);
```

### Server-Side

```rust
use mcp_transport::websocket::WebSocketListener;

let listener = WebSocketListener::bind("0.0.0.0:9000").await?;
while let Some(transport) = listener.accept().await? {
    tokio::spawn(handle_connection(transport));
}
```

### Configuration

```rust
use mcp_transport::websocket::WebSocketConfig;

let config = WebSocketConfig::new("wss://example.com/mcp")
    .with_reconnect(true)
    .with_max_reconnect_attempts(5)
    .with_reconnect_delay(Duration::from_secs(1))
    .with_ping_interval(Duration::from_secs(30));

let transport = WebSocketTransport::new(config);
```

### Auto-Reconnect

WebSocket transport supports automatic reconnection:

```rust
use mcp_transport::websocket::{WebSocketTransport, ExponentialBackoff};

let config = WebSocketConfig::new("ws://localhost:9000")
    .with_reconnect(true)
    .with_backoff(ExponentialBackoff::new(
        Duration::from_millis(100),  // initial
        Duration::from_secs(30),     // max
        2.0,                         // multiplier
    ));
```

### Connection State

Monitor connection state:

```rust
use mcp_transport::websocket::ConnectionState;

let state = transport.connection_state();
match state {
    ConnectionState::Connected => println!("Connected"),
    ConnectionState::Disconnected => println!("Disconnected"),
    ConnectionState::Reconnecting(attempt) => {
        println!("Reconnecting (attempt {})", attempt)
    }
}
```

## Unix Domain Sockets

For local IPC on Unix systems:

```rust
#[cfg(unix)]
use mcp_transport::unix::{UnixTransport, UnixSocketConfig};

let config = UnixSocketConfig::new("/tmp/mcp.sock");
let transport = UnixTransport::new(config);
```

### Server-Side

```rust
#[cfg(unix)]
use mcp_transport::unix::UnixListener;

let listener = UnixListener::bind("/tmp/mcp.sock")?;
while let Some(transport) = listener.accept().await? {
    tokio::spawn(handle_connection(transport));
}
```

### Features
- Lower latency than TCP
- File permission-based security
- No network overhead
- Unix/Linux/macOS only

## In-Memory Transport

For testing:

```rust
use mcp_transport::memory::MemoryTransport;

let (client_transport, server_transport) = MemoryTransport::pair();

// Use in tests
tokio::spawn(async move {
    server.serve(server_transport).await
});

let response = client_transport.send(request).await?;
```

## Connection Pooling

Reuse connections for efficiency:

```rust
use mcp_transport::pool::{Pool, PoolConfig};

let config = PoolConfig::new()
    .max_connections(10)
    .idle_timeout(Duration::from_secs(300))
    .connection_timeout(Duration::from_secs(5));

let pool = Pool::new(config, || async {
    WebSocketTransport::connect("ws://localhost:9000").await
});

// Get a connection from the pool
let conn = pool.get().await?;
let response = conn.send(request).await?;
// Connection returns to pool when dropped
```

### Pool Statistics

```rust
let stats = pool.stats();
println!("Active: {}", stats.active_connections);
println!("Idle: {}", stats.idle_connections);
println!("Total created: {}", stats.total_created);
println!("Total reused: {}", stats.total_reused);
```

## Choosing a Transport

| Transport | Use Case | Latency | Setup Complexity |
|-----------|----------|---------|------------------|
| **stdio** | Desktop apps, CLI | Low | Simple |
| **HTTP/SSE** | Web services, REST APIs | Medium | Medium |
| **WebSocket** | Real-time apps, persistent connections | Low | Medium |
| **Unix Socket** | Local services, high performance | Very Low | Simple (Unix only) |
| **Memory** | Testing | None | Simple |

## Custom Transports

Implement the `Transport` trait for custom transports:

```rust
use mcp_transport::traits::Transport;
use mcp_core::protocol::Message;
use mcp_core::error::McpError;

struct MyCustomTransport {
    // Your state
}

impl Transport for MyCustomTransport {
    async fn send(&self, message: Message) -> Result<(), McpError> {
        // Send the message
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, McpError> {
        // Receive a message
        Ok(None)
    }

    async fn close(&self) -> Result<(), McpError> {
        // Clean up
        Ok(())
    }
}
```

## Complete Example: Multi-Transport Server

```rust
use mcp::prelude::*;
use mcp_server::ServerBuilder;
use mcp_transport::{
    stdio::StdioTransport,
    websocket::{WebSocketListener, WebSocketConfig},
};

struct MyServer;

#[mcp_server(name = "multi-transport", version = "1.0.0")]
impl MyServer {
    #[tool(description = "Ping")]
    async fn ping(&self) -> ToolOutput {
        ToolOutput::text("pong")
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--stdio") => {
            // Run as stdio server
            let transport = StdioTransport::new();
            let server = ServerBuilder::new(MyServer)
                .with_tools(MyServer)
                .build();
            server.serve(transport).await
        }
        Some("--websocket") => {
            // Run as WebSocket server
            let listener = WebSocketListener::bind("0.0.0.0:9000").await?;
            println!("WebSocket server listening on ws://0.0.0.0:9000");

            while let Some(transport) = listener.accept().await? {
                let server = ServerBuilder::new(MyServer)
                    .with_tools(MyServer)
                    .build();
                tokio::spawn(async move {
                    if let Err(e) = server.serve(transport).await {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Ok(())
        }
        _ => {
            println!("Usage: server [--stdio | --websocket]");
            Ok(())
        }
    }
}
```

## Best Practices

1. **Match Transport to Use Case**: stdio for desktop, WebSocket for real-time
2. **Enable Reconnection**: For network transports, enable auto-reconnect
3. **Use Connection Pooling**: For multiple requests to same server
4. **Set Appropriate Timeouts**: Prevent hanging connections
5. **Handle Disconnections**: Gracefully handle transport failures
6. **Secure Production**: Use wss:// and https:// in production
