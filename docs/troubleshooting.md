# Troubleshooting Guide

This guide covers common issues and their solutions when using the Rust MCP SDK.

## Build Issues

### Compilation Errors

#### "cannot find macro `mcp_server`"

**Problem**: The `#[mcp_server]` macro is not recognized.

**Solution**: Ensure you have the macros feature enabled:

```toml
[dependencies]
mcpkit = { version = "0.4", features = ["server"] }
```

#### "the trait `ServerHandler` is not implemented"

**Problem**: Missing trait implementation after using `#[mcp_server]`.

**Solution**: Ensure the macro is applied to an `impl` block, not a struct:

```rust
// Correct
#[mcp_server(name = "my-server", version = "1.0.0")]
impl MyServer {
    // ...
}

// Incorrect - macro on struct
#[mcp_server(name = "my-server", version = "1.0.0")]
struct MyServer;
```

#### "MSRV too low"

**Problem**: Compilation fails with errors about unstable features.

**Solution**: Update to Rust 1.85 or later. Check your version:

```bash
rustc --version
rustup update stable
```

### Dependency Conflicts

#### Multiple versions of `tokio`

**Problem**: Build fails or behaves unexpectedly due to duplicate Tokio versions.

**Solution**: Check for duplicate dependencies:

```bash
cargo tree -d
```

Pin your Tokio version to match mcpkit's:

```toml
[dependencies]
tokio = { version = "1.43", features = ["full"] }
```

## Runtime Issues

### Connection Problems

#### "Connection refused" when connecting

**Problem**: Client cannot connect to server.

**Possible causes and solutions**:

1. **Server not running**: Start the server before connecting
2. **Wrong address/port**: Verify connection parameters
3. **Firewall blocking**: Check firewall rules for the port
4. **Unix socket permissions**: Ensure socket path is accessible

```rust
// Debug connection issues
let config = HttpTransportConfig::new("http://localhost:8080")
    .with_timeout(Duration::from_secs(30));

match HttpTransport::new(config) {
    Ok(transport) => { /* connected */ }
    Err(e) => eprintln!("Connection failed: {e:?}"),
}
```

#### "Transport closed unexpectedly"

**Problem**: Connection drops during operation.

**Solutions**:

1. Check server logs for errors
2. Verify network stability
3. Increase timeouts for long operations
4. Enable automatic reconnection for WebSocket transport:

```rust
let config = WebSocketConfig::new("ws://localhost:8080")
    .with_auto_reconnect(true)
    .with_reconnect_delay(Duration::from_secs(1));
```

### Protocol Errors

#### "Protocol version mismatch"

**Problem**: Client and server use incompatible MCP versions.

**Solution**: Check supported versions and negotiate properly:

```rust
// Server: Specify supported versions
let server = ServerBuilder::new(handler)
    .with_protocol_version(ProtocolVersion::V2025_11_25)
    .build();

// Client: Check server capabilities after connection
let caps = client.server_capabilities();
if caps.has_tools() {
    // Server supports tools
}
```

#### "Method not found"

**Problem**: Server doesn't recognize the requested method.

**Possible causes**:

1. **Capability not enabled**: Server doesn't advertise the capability
2. **Wrong method name**: Check spelling (e.g., `tools/call` not `tool/call`)
3. **Missing handler**: Handler not registered with the server

```rust
// Ensure capabilities are enabled
let server = ServerBuilder::new(handler)
    .with_tools(tool_handler)  // Enable tools
    .build();
```

#### "Invalid params"

**Problem**: Tool invocation fails due to parameter issues.

**Solutions**:

1. Check parameter types match the schema
2. Ensure required parameters are provided
3. Validate JSON format:

```rust
// Debug parameter parsing
#[tool(description = "Example tool")]
async fn my_tool(&self, name: String, count: u32) -> ToolOutput {
    // Parameters are automatically validated
    // If parsing fails, error is returned to client
    ToolOutput::text(format!("Name: {name}, Count: {count}"))
}
```

### Timeout Issues

#### "Request timed out"

**Problem**: Operations take too long and time out.

**Solutions**:

1. Increase timeout for long operations:

```rust
let config = HttpTransportConfig::new(url)
    .with_timeout(Duration::from_secs(120));
```

2. Use progress reporting for long operations:

```rust
#[tool(description = "Long running operation")]
async fn process(&self, ctx: &Context) -> Result<ToolOutput, McpError> {
    for i in 0..100 {
        ctx.report_progress(i as f64, Some(100.0), Some("Processing...")).await?;
        // ... do work
    }
    Ok(ToolOutput::text("Done"))
}
```

3. Implement cancellation handling:

```rust
async fn long_task(&self, ctx: &Context) -> Result<ToolOutput, McpError> {
    loop {
        if ctx.is_cancelled() {
            return Err(McpError::cancelled("Operation cancelled by client"));
        }
        // ... do work
    }
}
```

## Transport-Specific Issues

### stdio Transport

#### "Broken pipe" errors

**Problem**: Communication fails with subprocess.

**Solutions**:

1. Ensure the child process doesn't close stdin/stdout
2. Don't write to stderr during normal operation (use logging instead)
3. Check that both ends use the same line delimiters

#### JSON parsing errors

**Problem**: Messages fail to parse.

**Solutions**:

1. Ensure each message is on a single line (no embedded newlines)
2. Messages must be valid JSON-RPC 2.0 format
3. Check for debug output mixed with protocol messages

### HTTP/SSE Transport

#### "SSL certificate verification failed"

**Problem**: HTTPS connection fails due to certificate issues.

**Solution**: For development/testing only:

```rust
// WARNING: Only use in development
let config = HttpTransportConfig::new("https://localhost:8443")
    .with_danger_accept_invalid_certs(true);
```

For production, ensure proper certificate configuration.

#### SSE connection keeps disconnecting

**Problem**: Server-Sent Events stream closes unexpectedly.

**Solutions**:

1. Check server timeout settings
2. Implement keepalive/ping mechanism
3. Handle reconnection gracefully:

```rust
// Enable automatic reconnection
let config = HttpTransportConfig::new(url)
    .with_reconnect_on_error(true);
```

### WebSocket Transport

#### "WebSocket handshake failed"

**Problem**: Cannot establish WebSocket connection.

**Solutions**:

1. Verify the URL uses `ws://` or `wss://` scheme
2. Check that the server supports WebSocket upgrades
3. Verify any required headers are set:

```rust
let config = WebSocketConfig::new("wss://api.example.com/mcp")
    .with_header("Authorization", "Bearer token");
```

### Unix Socket Transport

#### "Permission denied"

**Problem**: Cannot connect to or create Unix socket.

**Solutions**:

1. Check socket file permissions
2. Ensure directory exists and is writable
3. Remove stale socket files:

```rust
let socket_path = "/tmp/my-mcp.sock";

// Clean up stale socket
if std::path::Path::new(socket_path).exists() {
    std::fs::remove_file(socket_path)?;
}
```

## Performance Issues

### High Memory Usage

**Problem**: Server consumes excessive memory.

**Solutions**:

1. Limit concurrent connections:

```rust
let pool = ConnectionPool::builder()
    .max_connections(10)
    .build();
```

2. Implement streaming for large responses
3. Use appropriate data structures (avoid cloning large data)

### Slow Response Times

**Problem**: Requests take too long to process.

**Solutions**:

1. Profile your tool implementations
2. Use async I/O for external calls
3. Implement caching where appropriate
4. Check middleware overhead:

```rust
// Measure middleware timing
let metrics = server.metrics();
for (method, stats) in metrics.per_method_stats() {
    println!("{method}: avg={:.2}ms", stats.avg_latency_ms);
}
```

## Debugging Tips

### Enable Tracing

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .init();

// Set RUST_LOG=mcpkit=debug for detailed logs
```

### Inspect Wire Protocol

For stdio transport, you can log all messages:

```rust
use mcpkit::transport::middleware::LoggingLayer;

let transport = StdioTransport::new();
let logged = LoggingLayer::new().layer(transport);
```

### Test with Mock Transport

Use the memory transport for testing:

```rust
use mcpkit_testing::MemoryTransport;

let (client_transport, server_transport) = MemoryTransport::pair();
// Use these for testing without network/process overhead
```

## Getting Help

If you can't resolve your issue:

1. Search existing [GitHub Issues](https://github.com/praxiomlabs/mcpkit/issues)
2. Check the [error handling guide](./error-handling.md) for error-specific help
3. Open a new issue with:
   - Rust version (`rustc --version`)
   - mcpkit version
   - Minimal reproduction case
   - Full error message and backtrace

## Related Documentation

- [Error Handling](./error-handling.md) - Understanding error types
- [Transports](./transports.md) - Transport configuration details
- [Security](./security.md) - Security considerations
- [Performance](./performance.md) - Performance optimization
