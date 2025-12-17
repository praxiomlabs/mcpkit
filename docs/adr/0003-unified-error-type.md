# ADR 0003: Unified Error Type

## Status

Accepted

## Context

The official rmcp SDK (as of December 2025) uses multiple nested error types:

```rust
// Three separate error types
enum ServerError { ... }
enum TransportError { ... }
enum ProtocolError { ... }

// Often combined with anyhow or custom wrappers
type Result<T> = std::result::Result<T, Box<dyn Error>>;
```

This approach has drawbacks:

- **Error type proliferation**: Multiple error types to handle
- **Difficult matching**: Deep nesting makes pattern matching verbose
- **Lost context**: Errors lose context as they propagate
- **Inconsistent handling**: Different parts use different approaches
- **JSON-RPC mapping complexity**: Converting to wire format is manual

## Decision

We implement a **single unified `McpError` enum** with:

1. **All error categories** in one type
2. **Context chaining** similar to `anyhow`
3. **Automatic JSON-RPC error code mapping**
4. **Rich diagnostic information** via `miette`
5. **Recovery classification** for LLM retry decisions

```rust
#[derive(Error, Diagnostic, Debug)]
pub enum McpError {
    // JSON-RPC standard errors
    Parse { message: String, source: Option<BoxError> },
    InvalidRequest { message: String, source: Option<BoxError> },
    MethodNotFound { method: String, available: Vec<String> },
    InvalidParams { method: String, message: String, ... },
    Internal { message: String, source: Option<BoxError> },

    // Transport errors
    Transport { kind: TransportErrorKind, message: String, context: TransportContext, ... },

    // Domain errors
    ToolExecution { tool: String, message: String, is_recoverable: bool, ... },
    ResourceNotFound { uri: String },
    ResourceAccessDenied { uri: String, reason: Option<String> },

    // Connection errors
    ConnectionFailed { message: String, source: Option<BoxError> },
    SessionExpired { session_id: String },
    HandshakeFailed { ... },

    // Operation errors
    Timeout { operation: String, duration: Duration },
    Cancelled { operation: String, reason: Option<String> },

    // Context wrapper
    WithContext { context: String, source: Box<McpError> },
}
```

## Consequences

### Positive

- **Single type to handle**: One `McpError` for everything
- **Flat matching**: No deep nesting required
- **Rich context**: `context()` and `with_context()` methods
- **Automatic JSON-RPC codes**: `error.code()` returns correct code
- **Recovery information**: `error.is_recoverable()` for LLMs
- **Beautiful diagnostics**: `miette` integration for terminal output
- **Type-safe construction**: Builder methods prevent mistakes

### Negative

- **Large enum**: Many variants in one type
- **Potential bloat**: Each variant adds to enum size
- **All errors visible**: Can't hide implementation details

### API Design

**Context Chaining:**
```rust
use mcpkit_core::error::{McpError, McpResultExt};

fn process() -> Result<(), McpError> {
    read_config()
        .context("Failed to load configuration")?;
    Ok(())
}

fn read_config() -> Result<Config, McpError> {
    let content = std::fs::read_to_string("config.json")
        .map_err(|e| McpError::internal_with_source("File read failed", e))?;

    serde_json::from_str(&content)
        .map_err(|e| McpError::parse_with_source("Invalid JSON", e))
        .with_context(|| "Failed to parse config.json")?
}
```

**JSON-RPC Conversion:**
```rust
let error = McpError::method_not_found("unknown_method");
let json_rpc: JsonRpcError = error.into();
// { "code": -32601, "message": "Method not found: unknown_method", ... }
```

**Recovery Classification:**
```rust
let error = McpError::invalid_params("search", "Query cannot be empty");
if error.is_recoverable() {
    // LLM can try with different parameters
}
```

### Mitigations

- Use `#[non_exhaustive]` to allow future variants
- Provide constructor methods to hide variant details
- Document error categories clearly
- Group related variants with comments

## Alternatives Considered

### 1. Multiple Error Types + anyhow

```rust
fn process() -> anyhow::Result<()> {
    // Mix of typed and dynamic errors
}
```

**Rejected because:**
- Loses type information
- Can't implement `McpResultExt` cleanly
- JSON-RPC mapping requires dynamic checks

### 2. Error Trait Objects

```rust
trait McpError: Error + Send + Sync {
    fn code(&self) -> i32;
    fn is_recoverable(&self) -> bool;
}
```

**Rejected because:**
- Can't match on specific errors
- Dynamic dispatch overhead
- Harder to extend

### 3. Result Extension Type

```rust
struct McpResult<T> {
    result: Result<T, McpError>,
    context: Vec<String>,
}
```

**Rejected because:**
- Non-standard Result type
- `?` operator doesn't work naturally
- Additional complexity

## References

- [Error Handling in Rust](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [anyhow crate](https://docs.rs/anyhow)
- [thiserror crate](https://docs.rs/thiserror)
- [miette crate](https://docs.rs/miette)
- [JSON-RPC 2.0 Error Codes](https://www.jsonrpc.org/specification#error_object)
