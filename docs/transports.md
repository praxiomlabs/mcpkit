# Transport Options

The Rust MCP SDK supports multiple transport mechanisms for different deployment scenarios.

## Standard I/O (stdio)

The most common transport for subprocess-based MCP servers:

```rust
use mcpkit_transport::stdio::StdioTransport;

let transport = StdioTransport::new();
```

### Use Cases
- Claude Desktop integration
- Local CLI tools
- Subprocess communication

### Configuration

```rust
use mcpkit_transport::stdio::StdioTransportBuilder;

let transport = StdioTransportBuilder::new()
    .buffer_size(8192)
    .build();
```

## HTTP with Server-Sent Events

For web-based deployments:

```rust
use mcpkit_transport::http::{HttpTransport, HttpTransportConfig};

// Client
let config = HttpTransportConfig::new("http://localhost:8080/mcp");
let transport = HttpTransport::new(config);

// Server
use mcpkit_transport::http::HttpTransportListener;

let listener = HttpTransportListener::bind("0.0.0.0:8080").await?;
while let Some(transport) = listener.accept().await? {
    tokio::spawn(async move {
        server.serve(transport).await
    });
}
```

### Configuration Options

```rust
use mcpkit_transport::http::HttpTransportBuilder;

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
use mcpkit_transport::websocket::{WebSocketTransport, WebSocketConfig};

let config = WebSocketConfig::new("ws://localhost:9000");
let transport = WebSocketTransport::new(config);
```

### Server-Side

```rust
use mcpkit_transport::websocket::WebSocketListener;

let listener = WebSocketListener::bind("0.0.0.0:9000").await?;
while let Some(transport) = listener.accept().await? {
    tokio::spawn(handle_connection(transport));
}
```

### Configuration

```rust
use mcpkit_transport::websocket::WebSocketConfig;

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
use mcpkit_transport::websocket::{WebSocketTransport, ExponentialBackoff};

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
use mcpkit_transport::websocket::ConnectionState;

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
use mcpkit_transport::unix::{UnixTransport, UnixSocketConfig};

let config = UnixSocketConfig::new("/tmp/mcp.sock");
let transport = UnixTransport::new(config);
```

### Server-Side

```rust
#[cfg(unix)]
use mcpkit_transport::unix::UnixListener;

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

## Spawned Process Transport

For managing MCP servers as child processes:

```rust
use mcpkit_transport::spawn::{SpawnedTransport, SpawnedTransportBuilder};

// Spawn an MCP server as a child process
let transport = SpawnedTransportBuilder::new("my-mcp-server")
    .arg("--config")
    .arg("config.json")
    .env("DEBUG", "true")
    .working_directory("/path/to/server")
    .build()
    .await?;

// Use like any other transport
let response = transport.send(request).await?;

// Process is killed when transport is dropped
```

### Configuration Options

```rust
let transport = SpawnedTransportBuilder::new("npx")
    .args(["mcp-server-sqlite", "--db", "database.db"])
    .env("NODE_ENV", "production")
    .inherit_env(true)  // Inherit parent environment
    .kill_on_drop(true) // Kill child when transport drops (default)
    .build()
    .await?;
```

### Process Management

```rust
// Check if process is still running
if transport.is_running() {
    println!("Server is running");
}

// Get process ID
if let Some(pid) = transport.pid() {
    println!("Server PID: {}", pid);
}

// Manually terminate (normally happens on drop)
transport.kill().await?;
```

### Use Cases

- Running Node.js MCP servers from Rust
- Managing Python MCP server subprocesses
- Testing MCP servers in isolation
- Claude Desktop-style server management

## In-Memory Transport

For testing:

```rust
use mcpkit_transport::memory::MemoryTransport;

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
use mcpkit_transport::pool::{Pool, PoolConfig};

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

Implementing custom transports allows integration with any communication layer. This section provides a comprehensive guide to building production-ready transports.

### The Transport Trait

```rust
use mcpkit_transport::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::future::Future;

pub trait Transport: Send + Sync {
    /// Error type for transport operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send a message over the transport
    fn send(&self, msg: Message) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Receive a message (returns None on clean close)
    fn recv(&self) -> impl Future<Output = Result<Option<Message>, Self::Error>> + Send;

    /// Close the transport gracefully
    fn close(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Check if the transport is connected
    fn is_connected(&self) -> bool;

    /// Get transport metadata
    fn metadata(&self) -> TransportMetadata;
}
```

### Complete Implementation Example

Here's a full example implementing a Redis-based transport:

```rust
use mcpkit_transport::{Transport, TransportMetadata, TransportError};
use mcpkit_core::protocol::Message;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A transport that uses Redis pub/sub for message passing.
pub struct RedisTransport {
    client: redis::Client,
    pubsub: Mutex<Option<redis::aio::PubSub>>,
    channel: String,
    connected: Arc<AtomicBool>,
    metadata: TransportMetadata,
}

impl RedisTransport {
    /// Create a new Redis transport.
    pub async fn connect(
        url: &str,
        channel: &str,
    ) -> Result<Self, TransportError> {
        let client = redis::Client::open(url)
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to create Redis client: {}", e),
            })?;

        let mut pubsub = client.get_async_pubsub().await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to connect: {}", e),
            })?;

        pubsub.subscribe(channel).await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to subscribe: {}", e),
            })?;

        Ok(Self {
            client,
            pubsub: Mutex::new(Some(pubsub)),
            channel: channel.to_string(),
            connected: Arc::new(AtomicBool::new(true)),
            metadata: TransportMetadata::new("redis")
                .remote_addr(url)
                .connected_now(),
        })
    }
}

impl Transport for RedisTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let json = serde_json::to_string(&msg)?;

        let mut conn = self.client.get_multiplexed_async_connection().await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to get connection: {}", e),
            })?;

        redis::cmd("PUBLISH")
            .arg(&self.channel)
            .arg(&json)
            .query_async(&mut conn)
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to publish: {}", e),
            })?;

        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let mut guard = self.pubsub.lock().await;
        let pubsub = guard.as_mut().ok_or(TransportError::ConnectionClosed)?;

        match pubsub.on_message().next().await {
            Some(msg) => {
                let payload: String = msg.get_payload()
                    .map_err(|e| TransportError::Deserialization {
                        message: format!("Failed to get payload: {}", e),
                    })?;

                let message: Message = serde_json::from_str(&payload)?;
                Ok(Some(message))
            }
            None => {
                self.connected.store(false, Ordering::SeqCst);
                Ok(None)
            }
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::SeqCst);

        let mut guard = self.pubsub.lock().await;
        if let Some(mut pubsub) = guard.take() {
            let _ = pubsub.unsubscribe(&self.channel).await;
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn metadata(&self) -> TransportMetadata {
        self.metadata.clone()
    }
}
```

### Custom Error Types

You can use `TransportError` or define your own error type:

```rust
use thiserror::Error;
use mcpkit_core::error::McpError;

#[derive(Error, Debug)]
pub enum MyTransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Not connected")]
    NotConnected,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// Convert to McpError for integration with the SDK
impl From<MyTransportError> for McpError {
    fn from(err: MyTransportError) -> Self {
        use mcpkit_core::error::{TransportDetails, TransportErrorKind, TransportContext};

        let kind = match &err {
            MyTransportError::ConnectionFailed(_) => TransportErrorKind::ConnectionFailed,
            MyTransportError::NotConnected => TransportErrorKind::ConnectionFailed,
            MyTransportError::Serialization(_) => TransportErrorKind::InvalidMessage,
            MyTransportError::Io(_) => TransportErrorKind::ReadFailed,
        };

        McpError::Transport(Box::new(TransportDetails {
            kind,
            message: err.to_string(),
            context: TransportContext::default(),
            source: Some(Box::new(err)),
        }))
    }
}
```

### Implementing TransportListener

For server-side transports that accept connections:

```rust
use mcpkit_transport::{Transport, TransportListener, TransportError};

pub struct MyListener {
    inner: tokio::net::TcpListener,
}

impl MyListener {
    pub async fn bind(addr: &str) -> Result<Self, TransportError> {
        let inner = tokio::net::TcpListener::bind(addr).await?;
        Ok(Self { inner })
    }
}

impl TransportListener for MyListener {
    type Transport = MyTransport;
    type Error = TransportError;

    async fn accept(&self) -> Result<Self::Transport, Self::Error> {
        let (stream, addr) = self.inner.accept().await?;
        Ok(MyTransport::from_stream(stream, addr))
    }

    fn local_addr(&self) -> Option<String> {
        self.inner.local_addr()
            .ok()
            .map(|a| a.to_string())
    }
}
```

### Testing Custom Transports

Use the `MemoryTransport` pattern for testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_receive() {
        // For network transports, consider using testcontainers
        // or mocking the underlying connection

        let transport = MyTransport::connect("test://localhost").await.unwrap();

        let msg = Message::Notification(Notification::new("test/ping"));
        transport.send(msg.clone()).await.unwrap();

        let received = transport.recv().await.unwrap();
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn test_close_behavior() {
        let transport = MyTransport::connect("test://localhost").await.unwrap();

        assert!(transport.is_connected());
        transport.close().await.unwrap();
        assert!(!transport.is_connected());

        // Send after close should fail
        let msg = Message::Notification(Notification::new("test"));
        let result = transport.send(msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_metadata() {
        let transport = MyTransport::connect("test://localhost").await.unwrap();
        let meta = transport.metadata();

        assert_eq!(meta.transport_type, "my-transport");
        assert!(meta.connected_at.is_some());
    }
}
```

### Adding Middleware Support

Wrap your transport with telemetry or other middleware:

```rust
use mcpkit_transport::telemetry::{TelemetryTransport, TelemetryConfig};

// Wrap any transport with telemetry
let base_transport = MyTransport::connect("...").await?;
let transport = TelemetryTransport::new(
    base_transport,
    TelemetryConfig::default(),
);

// Now all messages are tracked with metrics
```

### Thread Safety Requirements

Transports must be `Send + Sync`. Key patterns:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SafeTransport {
    // Use Arc for shared state
    state: Arc<TransportState>,
    // Use async Mutex for async operations
    reader: Mutex<Reader>,
    // Use atomic for simple flags
    connected: AtomicBool,
}

// This pattern allows concurrent send/recv from different tasks
impl Transport for SafeTransport {
    // ...
}
```

### Performance Considerations

1. **Minimize lock contention**: Use separate locks for send and receive paths
2. **Buffer appropriately**: Batch small messages when possible
3. **Handle backpressure**: Implement flow control for high-throughput scenarios
4. **Use zero-copy where possible**: Avoid unnecessary allocations

```rust
impl Transport for HighPerfTransport {
    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        // Pre-serialize outside any locks
        let bytes = serde_json::to_vec(&msg)?;

        // Brief lock only for the actual write
        let mut writer = self.writer.lock().await;
        writer.write_all(&bytes).await?;
        writer.flush().await?;

        Ok(())
    }
}
```

## Complete Example: Multi-Transport Server

```rust
use mcpkit::prelude::*;
use mcpkit_server::ServerBuilder;
use mcpkit_transport::{
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
