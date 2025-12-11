# Performance Guide

This document provides guidance on memory characteristics, connection pool sizing, and performance optimization for the Rust MCP SDK.

## Memory Characteristics

### Per-Request Memory

Each MCP request/response cycle allocates memory for:

- **Request parsing**: ~200-500 bytes for a minimal request, ~1-2KB for complex requests with nested parameters
- **Response serialization**: ~100-500 bytes for simple responses, scales linearly with content size
- **JSON Values**: Each `serde_json::Value` node has ~40 bytes overhead plus the actual data

### Tool Handler Memory

Tool handlers allocate memory for:

- **Tool registry**: ~500 bytes per tool definition (includes schema)
- **Argument parsing**: ~100 bytes per parameter during deserialization
- **Result creation**: ~100 bytes for `ToolOutput` wrapper plus content

### Long-Running Server Considerations

For servers expected to run continuously:

1. **No Memory Leaks by Design**: The SDK uses Rust's ownership system to ensure memory is freed when requests complete. There are no intentional caches that grow unboundedly.

2. **Bounded Buffers**: Transport implementations use bounded channels and buffers to prevent memory exhaustion under load.

3. **Request Isolation**: Each request processes independently; memory from one request does not affect others.

4. **Tool Registry**: The tool registry is built once at startup and remains constant. It does not grow during operation.

### Memory Optimization Tips

1. **Reuse Allocations**: For tools that process large amounts of data, consider using `Vec::with_capacity()` when you know the expected size.

2. **Stream Large Responses**: For tools returning large datasets, consider pagination or streaming rather than loading everything into memory.

3. **Limit Input Sizes**: Use the `max_input_size` tool attribute to reject overly large inputs early:
   ```rust
   #[tool(description = "Process data", max_input_size = 1048576)] // 1MB limit
   async fn process(&self, data: String) -> ToolOutput { ... }
   ```

4. **Avoid Unnecessary Clones**: The SDK uses references where possible, but tool handlers should minimize unnecessary cloning of large values.

## Connection Pool Sizing

### WebSocket Pool

The WebSocket connection pool (`crates/mcp-transport/src/pool.rs`) manages reusable connections.

#### Recommended Settings

| Workload | Min Connections | Max Connections | Idle Timeout |
|----------|-----------------|-----------------|--------------|
| Low (1-10 req/s) | 1 | 5 | 60s |
| Medium (10-100 req/s) | 2 | 20 | 30s |
| High (100+ req/s) | 5 | 50 | 15s |

#### Configuration Example

```rust
let pool = ConnectionPool::builder()
    .min_connections(2)
    .max_connections(20)
    .idle_timeout(Duration::from_secs(30))
    .build();
```

#### Sizing Guidelines

1. **Min Connections**: Set to your expected baseline concurrent requests. Having pre-warmed connections reduces latency for initial requests.

2. **Max Connections**: Set based on available memory and target server capacity. Each WebSocket connection uses ~8-16KB of memory when idle.

3. **Idle Timeout**: Shorter timeouts save memory but increase connection setup latency. Balance based on request patterns.

### Server Concurrent Requests

For servers handling concurrent tool calls:

```rust
let server = ServerBuilder::new(handler)
    .max_concurrent_requests(100)  // Default is reasonable for most use cases
    .request_timeout(Duration::from_secs(30))
    .build();
```

## Benchmarking

### Running Benchmarks

The SDK includes Criterion benchmarks for performance testing:

```bash
# Run all benchmarks
cargo bench --package mcpkit-benches

# Run specific benchmark suite
cargo bench --package mcpkit-benches --bench serialization
cargo bench --package mcpkit-benches --bench tool_invocation
cargo bench --package mcpkit-benches --bench transport
cargo bench --package mcpkit-benches --bench memory

# Quick validation with fewer samples
cargo bench --package mcpkit-benches -- --sample-size 10
```

### Benchmark Suites

#### Serialization (`benches/serialization.rs`)

Measures JSON-RPC message serialization/deserialization:
- Request serialization: ~60-330ns depending on complexity
- Response serialization: ~90ns (simple) to ~7µs (large with 100 items)
- Round-trip: ~1.5µs (request) to ~30µs (large response)

#### Tool Invocation (`benches/tool_invocation.rs`)

Measures tool handler overhead:
- Tool lookup: O(1) HashMap lookup
- Argument parsing: ~50ns (simple) to ~250ns (typed)
- End-to-end tool call: ~500ns including serialization

#### Transport (`benches/transport.rs`)

Measures channel and transport throughput:
- mpsc channel: ~10-50ns per message
- Memory transport: ~100-200ns round-trip

#### Memory (`benches/memory.rs`)

Measures memory allocation patterns:
- Request/response cycle: ~1µs per request
- Batch processing scales linearly
- Vector pre-allocation provides 2-3x speedup

### Interpreting Results

Criterion generates HTML reports in `target/criterion/report/index.html` with:
- Time distribution histograms
- Throughput calculations
- Regression detection across runs

## Performance Monitoring

### Logging Performance Metrics

Enable tracing for performance insights:

```rust
use tracing_subscriber::prelude::*;

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(tracing_subscriber::filter::LevelFilter::DEBUG)
    .init();
```

Key spans to monitor:
- `mcp_server::handle_request` - Total request processing time
- `mcp_server::call_tool` - Tool execution time
- `mcp_transport::send` / `recv` - Transport I/O time

### Metrics Collection

For production monitoring, integrate with your metrics system:

```rust
use std::time::Instant;

#[tool(description = "Example with metrics")]
async fn my_tool(&self, input: String) -> ToolOutput {
    let start = Instant::now();

    let result = self.do_work(&input).await;

    metrics::histogram!("tool.my_tool.duration_ms", start.elapsed().as_millis() as f64);
    metrics::counter!("tool.my_tool.calls", 1);

    result
}
```

## Best Practices

### Request Handling

1. **Fail Fast**: Validate inputs at the start of tool handlers to avoid wasted work.

2. **Bounded Timeouts**: Always set timeouts for external operations:
   ```rust
   tokio::time::timeout(Duration::from_secs(30), external_call()).await
   ```

3. **Graceful Degradation**: Return informative errors rather than panicking.

### Transport Selection

| Use Case | Recommended Transport |
|----------|----------------------|
| CLI tools, local usage | Stdio |
| Web integrations | WebSocket |
| REST API integration | HTTP/SSE |
| High-throughput internal | Memory (for testing) |

### Resource Cleanup

The SDK handles cleanup automatically, but for custom resources:

```rust
impl Drop for MyHandler {
    fn drop(&mut self) {
        // Cleanup custom resources
    }
}
```

## Troubleshooting Performance Issues

### High Memory Usage

1. Check for unbounded collections in tool handlers
2. Verify large responses are paginated
3. Review tool input sizes

### Slow Request Processing

1. Profile tool handlers with `tracing`
2. Check for blocking operations in async code
3. Verify connection pool is properly sized

### Connection Issues

1. Check pool exhaustion (increase max_connections)
2. Review idle timeout settings
3. Monitor connection error rates

## See Also

- [Architecture](architecture.md) - System design overview
- [Transports](transports.md) - Transport configuration
- [Security](security.md) - Security best practices
