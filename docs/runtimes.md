# Async Runtime Support

The Rust MCP SDK is designed to be runtime-agnostic, allowing you to choose the async runtime that best fits your project's needs.

## Supported Runtimes

| Runtime | Feature Flag | Status | Binary Size | Best For |
|---------|--------------|--------|-------------|----------|
| **Tokio** | `tokio-runtime` (default) | Fully supported | Larger | Production servers, full feature set |
| **async-std** | `async-std-runtime` | ⚠️ Deprecated | Medium | Legacy codebases only |
| **smol** | `smol-runtime` | Supported | Smallest | Embedded, minimal deployments |

> **⚠️ async-std Deprecation Notice**
>
> async-std has been marked as discontinued ([RUSTSEC-2025-0052](https://rustsec.org/advisories/RUSTSEC-2025-0052.html)).
> While the SDK still supports it for existing codebases, **new projects should use Tokio or smol**.
>
> - For production servers: Use **Tokio** (default)
> - For minimal binary size: Use **smol**
> - For existing async-std codebases: Migration to Tokio or smol is recommended

## Configuration

### Tokio (Default)

Tokio is the default runtime and requires no additional configuration:

```toml
[dependencies]
mcpkit-transport = "0.2"
mcpkit-server = "0.2"
tokio = { version = "1", features = ["full"] }
```

### async-std (Deprecated)

> **⚠️ Deprecated:** async-std is discontinued. Consider migrating to Tokio or smol for new projects.

For existing async-std codebases, disable default features and enable `async-std-runtime`:

```toml
[dependencies]
mcpkit-transport = { version = "0.2", default-features = false, features = ["async-std-runtime"] }
mcpkit-server = { version = "0.2", default-features = false, features = ["async-std-runtime"] }
async-std = { version = "1", features = ["attributes"] }
```

Example usage:

```rust
use mcpkit_server::ServerBuilder;
use mcpkit_transport::stdio::StdioTransport;

#[async_std::main]
async fn main() -> Result<(), mcpkit_core::error::McpError> {
    let transport = StdioTransport::new();
    let server = ServerBuilder::new(MyServer)
        .build();
    server.serve(transport).await
}
```

**Migration recommendation:** Replace `async-std` with `tokio` using the same API patterns. Most code changes are minimal (change `#[async_std::main]` to `#[tokio::main]`).

### smol

For minimal binary size, use smol:

```toml
[dependencies]
mcpkit-transport = { version = "0.2", default-features = false, features = ["smol-runtime"] }
mcpkit-server = { version = "0.2", default-features = false, features = ["smol-runtime"] }
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

#[cfg(feature = "async-std-runtime")]
fn async_std_specific() {
    // async-std-specific code
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
| async-std | ~2.8 MB | Simpler runtime |
| smol | ~1.8 MB | Minimal runtime |

*Sizes vary based on enabled features and optimization settings.*

## Troubleshooting

### "conflicting implementations" errors

Ensure you only enable one runtime feature at a time. If you're using workspace dependencies, check that all crates use the same runtime feature.

### "unresolved import" errors

Make sure you've enabled the corresponding runtime feature in all dependent crates.

### async-std: "no reactor running"

Ensure you're using `#[async_std::main]` or calling `async_std::task::block_on()`.

### smol: futures not progressing

Ensure you've called `smol::block_on()` or are running inside an executor.

## See Also

- [ADR 0004: Runtime-Agnostic Design](./adr/0004-runtime-agnostic-design.md)
- [Transport Documentation](./transports.md)
- [Performance Guide](./performance.md)
