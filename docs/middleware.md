# Using Middleware

The Rust MCP SDK includes a Tower-compatible middleware system for adding cross-cutting concerns like logging, timeouts, retries, and metrics.

## Available Middleware

### LoggingLayer

Log all messages sent and received:

```rust
use mcp_transport::middleware::LoggingLayer;
use tracing::Level;

let transport = StdioTransport::new();
let logged = LoggingLayer::new(Level::DEBUG).layer(transport);
```

### TimeoutLayer

Add timeouts to operations:

```rust
use mcp_transport::middleware::TimeoutLayer;
use std::time::Duration;

let transport = StdioTransport::new();
let with_timeout = TimeoutLayer::new(Duration::from_secs(30)).layer(transport);
```

### RetryLayer

Automatically retry failed operations:

```rust
use mcp_transport::middleware::RetryLayer;

let transport = HttpTransport::new(config);
let with_retry = RetryLayer::default().layer(transport);
```

With custom configuration:

```rust
use mcp_transport::middleware::{RetryLayer, ExponentialBackoff};

let retry = RetryLayer::new(
    3,  // max retries
    ExponentialBackoff::new(
        Duration::from_millis(100),  // initial delay
        Duration::from_secs(10),     // max delay
        2.0,                         // multiplier
    ),
);
let with_retry = retry.layer(transport);
```

### MetricsLayer

Collect performance metrics:

```rust
use mcp_transport::middleware::MetricsLayer;

let transport = WebSocketTransport::new(config);
let with_metrics = MetricsLayer::new().layer(transport);

// Later, retrieve metrics
let stats = with_metrics.stats();
println!("Messages sent: {}", stats.messages_sent);
println!("Average latency: {:?}", stats.avg_latency);
```

## Composing Middleware

Stack multiple middleware layers:

```rust
use mcp_transport::stdio::StdioTransport;
use mcp_transport::middleware::{LoggingLayer, TimeoutLayer, RetryLayer};
use std::time::Duration;
use tracing::Level;

let transport = StdioTransport::new();

// Apply layers (innermost first)
let stack = RetryLayer::default()
    .layer(TimeoutLayer::new(Duration::from_secs(30))
        .layer(LoggingLayer::new(Level::INFO)
            .layer(transport)));
```

Or use the builder pattern:

```rust
use mcp_transport::middleware::LayerStack;

let transport = StdioTransport::new();
let stack = LayerStack::new(transport)
    .with(LoggingLayer::new(Level::DEBUG))
    .with(TimeoutLayer::new(Duration::from_secs(30)))
    .with(RetryLayer::default())
    .build();
```

## Middleware Order

Order matters! Middleware is applied from inside out:

```rust
// This order:
let stack = RetryLayer::layer(
    TimeoutLayer::layer(
        LoggingLayer::layer(transport)
    )
);

// Means:
// 1. Logging sees all messages (innermost)
// 2. Timeout applies to logged operations
// 3. Retry wraps timeout (retries on timeout)
```

Recommended order (inner to outer):
1. **Logging** - See all traffic
2. **Metrics** - Measure actual operations
3. **Timeout** - Bound operation time
4. **Retry** - Retry timed-out operations

## Custom Middleware

Create custom middleware by implementing `Layer`:

```rust
use mcp_transport::middleware::Layer;
use mcp_transport::Transport;
use mcp_core::protocol::Message;
use std::pin::Pin;
use std::future::Future;

struct RateLimitLayer {
    max_requests_per_second: u32,
}

impl RateLimitLayer {
    pub fn new(max_rps: u32) -> Self {
        Self { max_requests_per_second: max_rps }
    }
}

impl<T: Transport> Layer<T> for RateLimitLayer {
    type Service = RateLimitedTransport<T>;

    fn layer(&self, inner: T) -> Self::Service {
        RateLimitedTransport {
            inner,
            max_rps: self.max_requests_per_second,
            // ... rate limiting state
        }
    }
}

struct RateLimitedTransport<T> {
    inner: T,
    max_rps: u32,
    // Rate limiting state...
}

impl<T: Transport> Transport for RateLimitedTransport<T> {
    // Implement Transport trait with rate limiting
}
```

## Per-Request Configuration

Some middleware supports per-request configuration:

```rust
use mcp_transport::middleware::TimeoutLayer;

// Global timeout
let transport = TimeoutLayer::new(Duration::from_secs(30))
    .layer(base_transport);

// Override for specific request
let response = transport
    .with_timeout(Duration::from_secs(60))
    .send(request)
    .await?;
```

## Error Handling in Middleware

Middleware can transform errors:

```rust
// TimeoutLayer converts timeout to McpError::Timeout
let result = with_timeout.send(request).await;
match result {
    Err(McpError::Timeout { operation, duration }) => {
        println!("Operation {} timed out after {:?}", operation, duration);
    }
    _ => {}
}

// RetryLayer may succeed after retries
let result = with_retry.send(request).await;
// Error only if all retries failed
```

## Complete Example

```rust
use mcp::prelude::*;
use mcp_server::ServerBuilder;
use mcp_transport::websocket::{WebSocketTransport, WebSocketConfig};
use mcp_transport::middleware::{
    LayerStack, LoggingLayer, TimeoutLayer, RetryLayer, MetricsLayer,
};
use std::time::Duration;
use tracing::Level;

struct MyServer;

#[mcp_server(name = "my-server", version = "1.0.0")]
impl MyServer {
    #[tool(description = "Do something")]
    async fn action(&self) -> ToolOutput {
        ToolOutput::text("Done!")
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Initialize tracing
    tracing_subscriber::init();

    // Create base transport
    let config = WebSocketConfig::new("ws://localhost:9000");
    let transport = WebSocketTransport::new(config);

    // Apply middleware stack
    let stack = LayerStack::new(transport)
        .with(LoggingLayer::new(Level::DEBUG))
        .with(MetricsLayer::new())
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(RetryLayer::new(3, Default::default()))
        .build();

    // Build server with middleware
    let server = ServerBuilder::new(MyServer)
        .with_tools(MyServer)
        .build();

    // Serve
    server.serve(stack).await
}
```

## Best Practices

1. **Order Carefully**: Put logging innermost to see all traffic
2. **Set Reasonable Timeouts**: Don't set too short or too long
3. **Limit Retries**: Usually 2-3 retries is sufficient
4. **Monitor Metrics**: Use metrics to tune configuration
5. **Log Appropriately**: Use DEBUG for development, INFO for production
6. **Handle Errors**: Middleware errors should be informative
