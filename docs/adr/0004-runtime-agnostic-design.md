# ADR 0004: Runtime-Agnostic Design

## Status

Accepted

## Context

The Rust async ecosystem has multiple runtime options:

- **Tokio**: Most popular, feature-rich, larger binary size
- **async-std**: Simpler API, similar to std library
- **smol**: Minimal, composable, small binary size
- **Custom**: Some projects use custom executors

The official rmcp SDK is tightly coupled to Tokio:

```rust
// rmcp requires Tokio
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
```

This creates problems:

- **Vendor lock-in**: Users must use Tokio
- **Binary bloat**: Tokio adds significant binary size
- **Incompatibility**: Can't integrate with async-std projects
- **Embedded limitations**: Tokio may be too heavy for embedded

## Decision

We design the SDK with **runtime agnosticism** as a core principle:

### 1. Core Crate Has No Runtime Dependency

`mcp-core` contains:
- Protocol types (Request, Response, Notification)
- Error types (McpError)
- Capability types
- Typestate connection markers

All synchronous, no async runtime needed.

### 2. Transport Crate Uses Feature Gates

```toml
[features]
default = ["tokio-runtime"]
tokio-runtime = ["tokio"]
async-std-runtime = ["async-std", "async-io"]
smol-runtime = ["smol", "async-io"]
```

### 3. Runtime Abstraction Layer

```rust
// Internal abstraction over runtime-specific types
pub mod runtime {
    #[cfg(feature = "tokio-runtime")]
    pub use tokio::io::{AsyncRead, AsyncWrite};

    #[cfg(feature = "async-std-runtime")]
    pub use async_std::io::{Read as AsyncRead, Write as AsyncWrite};
}
```

### 4. Transport Trait Is Runtime-Agnostic

```rust
pub trait Transport: Send + Sync {
    fn send(&self, message: Message) -> impl Future<Output = Result<(), McpError>> + Send;
    fn recv(&self) -> impl Future<Output = Result<Option<Message>, McpError>> + Send;
    fn close(&self) -> impl Future<Output = Result<(), McpError>> + Send;
}
```

## Consequences

### Positive

- **User choice**: Developers choose their runtime
- **Smaller binaries**: Use smol for minimal size
- **Broader compatibility**: Integrate with any async ecosystem
- **Embedded support**: smol works in constrained environments
- **Future-proof**: New runtimes can be supported

### Negative

- **Feature gate complexity**: Must select correct features
- **Testing overhead**: Test with multiple runtimes
- **Conditional compilation**: More `#[cfg(...)]` attributes
- **Documentation**: Must document feature combinations

### Example Usage

**With Tokio (default):**
```toml
[dependencies]
mcp-transport = "0.1"  # tokio-runtime is default
```

**With async-std:**
```toml
[dependencies]
mcp-transport = { version = "0.1", default-features = false, features = ["async-std-runtime"] }
```

**With smol:**
```toml
[dependencies]
mcp-transport = { version = "0.1", default-features = false, features = ["smol-runtime"] }
```

### Crate Dependency Graph

```
mcp-core (no runtime)
    │
    ├── mcp-macros (no runtime, proc-macro)
    │
    └── mcp-transport (runtime via features)
            │
            ├── mcp-server (inherits runtime)
            │
            └── mcp-client (inherits runtime)
```

### Runtime-Specific Code

When runtime-specific code is needed:

```rust
#[cfg(feature = "tokio-runtime")]
async fn spawn_task<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

#[cfg(feature = "async-std-runtime")]
async fn spawn_task<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    async_std::task::spawn(future);
}
```

## Alternatives Considered

### 1. Tokio Only

```rust
// Just use Tokio everywhere
use tokio::*;
```

**Rejected because:**
- Forces runtime choice on users
- Larger binary size
- Can't integrate with other ecosystems

### 2. Runtime Trait Object

```rust
trait Runtime {
    fn spawn(&self, future: Pin<Box<dyn Future>>);
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future>>;
}
```

**Rejected because:**
- Dynamic dispatch overhead
- Boxing futures is expensive
- Doesn't integrate with existing traits

### 3. Generic Over Runtime

```rust
struct Server<R: Runtime> {
    runtime: R,
}
```

**Rejected because:**
- Infects all types with generic parameter
- Harder API to use
- Complex type signatures

## References

- [Rust Async Book - Executors](https://rust-lang.github.io/async-book/02_execution/01_chapter.html)
- [Tokio vs async-std](https://www.reddit.com/r/rust/comments/lg0a7b/tokio_vs_asyncstd_in_2021/)
- [smol crate](https://docs.rs/smol)
- [Are we async yet?](https://areweasyncyet.rs/)
