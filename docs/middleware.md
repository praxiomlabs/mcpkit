# Using Middleware

The Rust MCP SDK includes a Tower-compatible middleware system for adding cross-cutting concerns like logging, timeouts, retries, and metrics.

## Available Middleware

### LoggingLayer

Log all messages sent and received:

```rust
use mcpkit_transport::middleware::LoggingLayer;
use tracing::Level;

let transport = StdioTransport::new();
let logged = LoggingLayer::new(Level::DEBUG).layer(transport);
```

### TimeoutLayer

Add timeouts to operations:

```rust
use mcpkit_transport::middleware::TimeoutLayer;
use std::time::Duration;

let transport = StdioTransport::new();
let with_timeout = TimeoutLayer::new(Duration::from_secs(30)).layer(transport);
```

### RetryLayer

Automatically retry failed operations:

```rust
use mcpkit_transport::middleware::RetryLayer;

let transport = HttpTransport::new(config);
let with_retry = RetryLayer::default().layer(transport);
```

With custom configuration:

```rust
use mcpkit_transport::middleware::{RetryLayer, ExponentialBackoff};

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
use mcpkit_transport::middleware::MetricsLayer;

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
use mcpkit_transport::stdio::StdioTransport;
use mcpkit_transport::middleware::{LoggingLayer, TimeoutLayer, RetryLayer};
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
use mcpkit_transport::middleware::LayerStack;

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
use mcpkit_transport::middleware::Layer;
use mcpkit_transport::Transport;
use mcpkit_core::protocol::Message;
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
use mcpkit_transport::middleware::TimeoutLayer;

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
use mcpkit::prelude::*;
use mcpkit_server::ServerBuilder;
use mcpkit_transport::websocket::{WebSocketTransport, WebSocketConfig};
use mcpkit_transport::middleware::{
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

## Advanced Composition Patterns

### Conditional Middleware

Apply middleware based on conditions:

```rust
use mcpkit_transport::middleware::{LayerStack, LoggingLayer, TimeoutLayer};

fn build_transport(debug: bool) -> impl Transport {
    let base = StdioTransport::new();
    let mut stack = LayerStack::new(base);

    if debug {
        stack = stack.with(LoggingLayer::new(Level::TRACE));
    }

    stack
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .build()
}
```

### Environment-Based Configuration

Configure middleware from environment:

```rust
fn configure_middleware() -> LayerStack<StdioTransport> {
    let transport = StdioTransport::new();
    let log_level = std::env::var("LOG_LEVEL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(Level::INFO);

    let timeout_secs: u64 = std::env::var("TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let max_retries: usize = std::env::var("MAX_RETRIES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);

    LayerStack::new(transport)
        .with(LoggingLayer::new(log_level))
        .with(TimeoutLayer::new(Duration::from_secs(timeout_secs)))
        .with(RetryLayer::new(max_retries, Default::default()))
        .build()
}
```

### Middleware Factories

Create reusable middleware configurations:

```rust
/// Production-ready middleware stack
pub fn production_stack<T: Transport>(transport: T) -> impl Transport {
    LayerStack::new(transport)
        .with(LoggingLayer::new(Level::INFO))
        .with(MetricsLayer::new())
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(RetryLayer::new(3, ExponentialBackoff::default()))
        .build()
}

/// Development stack with verbose logging
pub fn development_stack<T: Transport>(transport: T) -> impl Transport {
    LayerStack::new(transport)
        .with(LoggingLayer::new(Level::TRACE))
        .with(TimeoutLayer::new(Duration::from_secs(120)))  // Longer for debugging
        .build()
}

/// Minimal stack for testing
pub fn test_stack<T: Transport>(transport: T) -> impl Transport {
    LayerStack::new(transport)
        .with(TimeoutLayer::new(Duration::from_secs(5)))
        .build()
}
```

### Sharing State Across Middleware

Use `Arc` for shared state:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

struct SharedMetrics {
    requests: AtomicU64,
    errors: AtomicU64,
}

let metrics = Arc::new(SharedMetrics::default());

// Clone Arc for each middleware that needs it
let logging_metrics = Arc::clone(&metrics);
let retry_metrics = Arc::clone(&metrics);

let stack = LayerStack::new(transport)
    .with(LoggingLayerWithMetrics::new(logging_metrics))
    .with(RetryLayerWithMetrics::new(retry_metrics))
    .build();

// Access metrics elsewhere
println!("Total requests: {}", metrics.requests.load(Ordering::Relaxed));
```

### Branching Middleware Stacks

Different stacks for different transports:

```rust
enum TransportKind {
    Stdio,
    WebSocket(String),
    Http(String),
}

fn build_transport(kind: TransportKind) -> Box<dyn Transport> {
    match kind {
        TransportKind::Stdio => {
            let t = StdioTransport::new();
            Box::new(production_stack(t))
        }
        TransportKind::WebSocket(url) => {
            let config = WebSocketConfig::new(&url)
                .with_reconnect(true);
            let t = WebSocketTransport::new(config);
            // WebSocket gets extra retry layer
            Box::new(
                LayerStack::new(t)
                    .with(LoggingLayer::new(Level::INFO))
                    .with(RetryLayer::new(5, ExponentialBackoff::aggressive()))
                    .with(TimeoutLayer::new(Duration::from_secs(30)))
                    .build()
            )
        }
        TransportKind::Http(url) => {
            let config = HttpTransportConfig::new(&url);
            let t = HttpTransport::new(config);
            // HTTP has shorter timeouts, more retries
            Box::new(
                LayerStack::new(t)
                    .with(LoggingLayer::new(Level::INFO))
                    .with(RetryLayer::new(10, ExponentialBackoff::conservative()))
                    .with(TimeoutLayer::new(Duration::from_secs(10)))
                    .build()
            )
        }
    }
}
```

### Middleware Composition with Tower

For Tower-compatible middleware:

```rust
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer as TowerTimeout;

// Combine MCP middleware with Tower middleware
let tower_stack = ServiceBuilder::new()
    .layer(TowerTimeout::new(Duration::from_secs(30)))
    .layer(tower_http::trace::TraceLayer::new_for_http())
    .service(base_service);
```

## Middleware Interaction Patterns

### Request Lifecycle

Understanding how a request flows through the stack:

```
Request → Retry → Timeout → Logging → Transport
                                         ↓
Response ← Retry ← Timeout ← Logging ← Transport
```

Each layer can:
1. **Transform** the request before passing down
2. **Short-circuit** and return early
3. **Transform** the response before passing up
4. **Catch errors** and handle or re-throw

### Error Propagation

```rust
// Timeout layer converts timeouts to McpError::Timeout
// Retry layer catches errors and may retry
// If retry exhausted, original or wrapped error propagates up

let stack = LayerStack::new(transport)
    .with(TimeoutLayer::new(Duration::from_secs(5)))   // May emit Timeout
    .with(RetryLayer::new(3, Default::default()))       // Catches Timeout, retries
    .build();

// Caller sees either:
// - Success after retries
// - Final error after all retries exhausted
```

### Metrics Aggregation

Collect metrics at multiple layers:

```rust
let transport_metrics = Arc::new(Metrics::new());
let app_metrics = Arc::clone(&transport_metrics);

let stack = LayerStack::new(transport)
    .with(MetricsLayer::with_metrics(Arc::clone(&transport_metrics)))  // Transport-level
    .with(TimeoutLayer::new(Duration::from_secs(30)))
    .with(RetryLayer::new(3, Default::default()))
    .with(MetricsLayer::with_metrics(Arc::clone(&transport_metrics)))  // App-level
    .build();

// transport_metrics now has both layers' data
```

## Best Practices

1. **Order Carefully**: Put logging innermost to see all traffic
2. **Set Reasonable Timeouts**: Don't set too short or too long
3. **Limit Retries**: Usually 2-3 retries is sufficient
4. **Monitor Metrics**: Use metrics to tune configuration
5. **Log Appropriately**: Use DEBUG for development, INFO for production
6. **Handle Errors**: Middleware errors should be informative
7. **Use Factories**: Create reusable middleware configurations for consistency
8. **Share State Carefully**: Use `Arc` for cross-middleware state
9. **Test Independently**: Test each middleware layer in isolation
10. **Document Order**: Comment why middleware is ordered a specific way
