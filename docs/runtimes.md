# Async Runtime Support

The Rust MCP SDK is designed to be runtime-agnostic, allowing you to choose the async runtime that best fits your project's needs.

## Supported Runtimes

| Runtime | Feature Flag | Status | Binary Size | Best For |
|---------|--------------|--------|-------------|----------|
| **Tokio** | `tokio-runtime` (default) | Fully supported | Larger | Production servers, full feature set |
| **smol** | `smol-runtime` | Supported | Smallest | Embedded, minimal deployments |

## Configuration

### Tokio (Default)

Tokio is the default runtime and requires no additional configuration:

```toml
[dependencies]
mcpkit-transport = "0.4"
mcpkit-server = "0.4"
tokio = { version = "1", features = ["full"] }
```

### smol

For minimal binary size, use smol:

```toml
[dependencies]
mcpkit-transport = { version = "0.4", default-features = false, features = ["smol-runtime"] }
mcpkit-server = { version = "0.4", default-features = false, features = ["smol-runtime"] }
smol = "2"
```

Example usage:

```rust
use mcpkit_server::ServerBuilder;
use mcpkit_transport::stdio::StdioTransport;

fn main() -> Result<(), mcpkit_core::error::McpError> {
    smol::block_on(async {
        let transport = StdioTransport::new();
        let server = ServerBuilder::new(MyServer)
            .build();
        server.serve(transport).await
    })
}
```

## Runtime Abstractions

The SDK provides runtime-agnostic abstractions through the `mcpkit_transport::runtime` module:

### Mutex and Synchronization

```rust
use mcpkit_transport::runtime::{AsyncMutex, AsyncRwLock, AsyncSemaphore};

// These work identically across all runtimes
let mutex = AsyncMutex::new(my_data);
let guard = mutex.lock().await;
```

### Channels

```rust
use mcpkit_transport::runtime::{channel, Sender, Receiver};

// Bounded MPSC channel that works with any runtime
let (tx, rx) = channel::<Message>(100);
```

### Sleep and Timeout

```rust
use mcpkit_transport::runtime::{sleep, timeout, TimeoutError};
use std::time::Duration;

// Sleep works on any runtime
sleep(Duration::from_secs(1)).await;

// Timeout wraps any future
match timeout(Duration::from_secs(5), some_future).await {
    Ok(result) => println!("Got result: {:?}", result),
    Err(TimeoutError) => println!("Operation timed out"),
}
```

### Spawning Tasks

```rust
use mcpkit_transport::runtime::spawn;

// Spawn a background task on any runtime
spawn(async {
    // background work
});
```

## Feature-Gated Code

When writing code that needs to be runtime-specific:

```rust
#[cfg(feature = "tokio-runtime")]
fn tokio_specific() {
    // Tokio-specific code
}

#[cfg(feature = "smol-runtime")]
fn smol_specific() {
    // smol-specific code
}
```

## HTTP and WebSocket Transport

The HTTP and WebSocket transports currently require Tokio due to dependencies on `axum`, `hyper`, and `tokio-tungstenite`:

```toml
[features]
http = ["reqwest", "axum", "hyper", "tokio-runtime"]
websocket = ["tokio-tungstenite", "tokio-runtime"]
```

For non-Tokio runtimes, use:
- Stdio transport (`StdioTransport`)
- Spawned process transport (`SpawnedTransport`)
- Memory transport for testing (`MemoryTransport`)

## Binary Size Comparison

Approximate release binary sizes for a minimal MCP server:

| Runtime | Binary Size | Notes |
|---------|-------------|-------|
| Tokio | ~3.5 MB | Full async runtime |
| smol | ~1.8 MB | Minimal runtime |

*Sizes vary based on enabled features and optimization settings.*

## Troubleshooting

### "conflicting implementations" errors

Ensure you only enable one runtime feature at a time. If you're using workspace dependencies, check that all crates use the same runtime feature.

### "unresolved import" errors

Make sure you've enabled the corresponding runtime feature in all dependent crates.

### smol: futures not progressing

Ensure you've called `smol::block_on()` or are running inside an executor.

## Runtime Selection Guide

Use this decision tree to choose the right runtime for your project:

### Choose Tokio if:

- You need HTTP or WebSocket transports
- You're building a production server that handles many concurrent connections
- You want the most mature async ecosystem with extensive documentation
- Binary size is not a critical concern
- You need advanced features like `tokio::select!` or `tokio::join!`

### Choose smol if:

- You want the smallest possible binary size
- You're building embedded or resource-constrained applications
- You only need stdio or memory transports
- You prefer a simpler, more lightweight runtime
- You're targeting WASM (smol has better WASM compatibility)
- You're learning async Rust (smol is conceptually simpler)

### Quick Reference

```
┌─────────────────────────────────────────────────────────────────┐
│                  Do you need HTTP/WebSocket?                     │
└───────────────────────────┬─────────────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            │                               │
           Yes                              No
            │                               │
            v                               v
    ┌───────────────┐           ┌───────────────────────────────┐
    │  Use Tokio    │           │ Is binary size critical?      │
    └───────────────┘           └───────────────┬───────────────┘
                                                │
                                ┌───────────────┴───────────────┐
                                │                               │
                               Yes                              No
                                │                               │
                                v                               v
                        ┌───────────────┐           ┌───────────────┐
                        │   Use smol    │           │   Use Tokio   │
                        └───────────────┘           │   (default)   │
                                                    └───────────────┘
```

## Example: smol Server

See the `examples/smol-server` for a complete working example:

```bash
cargo run -p smol-server
```

Key patterns for smol:

```rust
// Instead of #[tokio::main]
fn main() -> Result<(), McpError> {
    smol::block_on(async {
        // Your async code here
    })
}

// Instead of tokio::time::sleep
smol::Timer::after(Duration::from_secs(1)).await;

// Instead of tokio::spawn
smol::spawn(async { /* background task */ }).detach();

// Instead of tokio::join!
use futures_lite::future;
let (a, b) = future::zip(future_a, future_b).await;
```

## See Also

- [ADR 0004: Runtime-Agnostic Design](./adr/0004-runtime-agnostic-design.md)
- [Transport Documentation](./transports.md)
- [Performance Guide](./performance.md)
- [smol Server Example](../examples/smol-server/)
