# Architecture Overview

This document provides a high-level overview of the mcpkit architecture, explaining how the various crates interact and the design patterns used.

## Crate Dependency Graph

```
                    ┌────────────────┐
                    │   mcpkit     │  (Unified facade)
                    └───────┬────────┘
                            │
         ┌──────────────────┼──────────────────┐
         │                  │                  │
         ▼                  ▼                  ▼
┌────────────────┐  ┌────────────────┐  ┌────────────────┐
│  mcp-client    │  │   mcp-server   │  │  mcp-testing   │
└───────┬────────┘  └───────┬────────┘  └───────┬────────┘
        │                   │                   │
        └─────────┬─────────┴───────────────────┘
                  │
                  ▼
         ┌────────────────┐
         │  mcp-transport │
         └───────┬────────┘
                 │
         ┌───────┴────────┐
         │                │
         ▼                ▼
┌────────────────┐  ┌────────────────┐
│   mcp-core     │  │  mcp-macros    │
└────────────────┘  └────────────────┘
```

## Crate Responsibilities

### `mcpkit-core`

The foundational crate containing:

- **Protocol Types**: JSON-RPC message structures (`Request`, `Response`, `Notification`)
- **MCP Types**: Tools, Resources, Prompts, Content types
- **Capability System**: Server/client capability negotiation
- **Error Types**: Unified `McpError` with JSON-RPC error codes
- **OAuth Types**: RFC 9728, RFC 8414, RFC 7636 implementations

This crate has minimal dependencies and can be used independently for type definitions.

### `mcpkit-transport`

Provides transport layer abstractions:

- **Transport Trait**: Async send/receive interface
- **Implementations**:
  - `StdioTransport` - Standard input/output (default for MCP)
  - `WebSocketTransport` - WebSocket connections
  - `HttpTransport` - HTTP/SSE (Streamable HTTP)
  - `MemoryTransport` - In-process testing
  - `UnixTransport` - Unix domain sockets
  - `SpawnedTransport` - Child process management
- **Middleware**: Rate limiting, logging, telemetry
- **Connection Pooling**: Multi-connection management

### `mcpkit-server`

Server-side functionality:

- **Server Builder**: Typestate pattern for safe configuration
- **Handler Traits**: `ToolHandler`, `ResourceHandler`, `PromptHandler`
- **Router**: Automatic request dispatching
- **Context**: Request-scoped state with capability checking
- **State Management**: Connection tracking, protocol state

### `mcpkit-client`

Client-side functionality:

- **Client Builder**: Configuration and connection setup
- **Handler Trait**: `ClientHandler` for server-initiated requests
- **Request/Response**: Type-safe MCP method invocations
- **Capability Validation**: Ensure server supports requested features

### `mcpkit-macros`

Procedural macros for ergonomic APIs:

- `#[tool]` - Define tools from functions
- `#[resource]` - Define resources from functions
- `#[prompt]` - Define prompts from functions
- Schema generation from Rust types
- Debug expansion via `debug_expand` attribute

### `mcpkit-testing`

Testing utilities:

- Mock transports
- Assertion helpers
- Test fixtures

### `mcpkit`

Unified facade crate that re-exports all functionality:

```rust
use mcpkit::prelude::*;
// Access to all types, traits, and macros
```

## Design Patterns

### Typestate Pattern

Builders use typestate to ensure valid configurations at compile time:

```rust
// ServerBuilder enforces required fields via type parameters
let server = ServerBuilder::new("my-server", "1.0.0")  // RequiresConfig state
    .with_transport(transport)                          // RequiresHandlers state
    .with_tool_handler(handler)                        // Ready state
    .build();                                          // Only available in Ready state
```

See [ADR-0002](./adr/0002-typestate-pattern.md) for details.

### Unified Error Type

All errors flow through `McpError`, which maps to JSON-RPC error codes:

```rust
pub enum McpError {
    ParseError(String),           // -32700
    InvalidRequest(String),       // -32600
    MethodNotFound(String),       // -32601
    InvalidParams(String),        // -32602
    InternalError(String),        // -32603
    // ... MCP-specific errors
}
```

See [ADR-0003](./adr/0003-unified-error-type.md) for details.

### Handler Traits

Capabilities are defined via traits with default implementations:

```rust
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn list_tools(&self, ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(Vec::new())  // Default: no tools
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError>;
}
```

### Context-Driven Access

Request handlers receive a `Context` providing:

- Request metadata (ID, method)
- Client/server capabilities
- Progress token for notifications
- Peer communication channel

```rust
async fn my_tool(args: Value, ctx: &Context<'_>) -> Result<ToolOutput, McpError> {
    // Check if client supports a feature
    if ctx.client_capabilities().has_sampling() {
        // Can request LLM sampling
    }

    // Send progress notification
    ctx.notify_progress(0.5, "Halfway done").await?;

    Ok(ToolOutput::text("Done"))
}
```

## Message Flow

### Client → Server Request

```
┌────────────┐        ┌─────────────┐        ┌────────────┐
│   Client   │        │  Transport  │        │   Server   │
└─────┬──────┘        └──────┬──────┘        └─────┬──────┘
      │                      │                      │
      │  Request (JSON-RPC)  │                      │
      │─────────────────────>│                      │
      │                      │  Deserialize         │
      │                      │─────────────────────>│
      │                      │                      │
      │                      │        Router        │
      │                      │        ↓             │
      │                      │   Handler Trait     │
      │                      │        ↓             │
      │                      │     Context         │
      │                      │        ↓             │
      │                      │  Response (Result)  │
      │                      │<─────────────────────│
      │  Response (JSON-RPC) │                      │
      │<─────────────────────│                      │
```

### Initialization Handshake

```
Client                          Server
   │                               │
   │──── initialize ──────────────>│
   │                               │
   │<─── InitializeResult ─────────│
   │     (negotiated capabilities) │
   │                               │
   │──── notifications/initialized>│
   │                               │
   │<──── Ready for requests ──────│
```

### Protocol Version Negotiation

Version negotiation follows the MCP specification:

1. Client sends preferred version in `initialize`
2. Server responds with same version if supported, or counter-offer
3. Client validates server's version

Supported versions:
- `2025-11-25` (latest)
- `2024-11-05` (original MCP spec)

See [protocol-versions.md](./protocol-versions.md) for details.

## Feature Flags

The SDK uses Cargo features for optional functionality:

| Feature | Default | Description |
|---------|---------|-------------|
| `server` | Yes | Server-side functionality |
| `client` | Yes | Client-side functionality |
| `tokio-runtime` | Yes | Tokio async runtime support |
| `websocket` | No | WebSocket transport |
| `http` | No | HTTP/SSE transport |
| `full` | No | All optional features |

## Security Architecture

### OAuth 2.1 Integration

The SDK implements OAuth 2.1 per the MCP specification:

```
┌─────────┐     ┌───────────────┐     ┌─────────────┐
│ Client  │────>│ Authorization │────>│ MCP Server  │
│         │     │    Server     │     │  (Resource) │
└─────────┘     └───────────────┘     └─────────────┘
     │                 │                     │
     │  1. Discover    │                     │
     │────────────────>│                     │
     │                 │                     │
     │  2. Authorize   │                     │
     │────────────────>│                     │
     │                 │                     │
     │  3. Token       │                     │
     │<────────────────│                     │
     │                 │                     │
     │  4. Request with Bearer Token         │
     │──────────────────────────────────────>│
```

### Transport Security

- TLS support for WebSocket and HTTP transports
- Origin validation for browser clients
- Rate limiting middleware
- Input size limits

## Testing Architecture

### Test Pyramid

```
         ╱╲          E2E Tests (Claude Desktop)
        ╱  ╲
       ╱────╲        Integration Tests (transport_e2e.rs)
      ╱      ╲
     ╱────────╲      Component Tests (tools_integration.rs)
    ╱          ╲
   ╱────────────╲    Unit Tests (per-module)
  ╱              ╲
 ╱════════════════╲  Fuzzing (fuzz/)
```

### Fuzzing Targets

The `fuzz/` directory contains libFuzzer targets for:

- JSON-RPC message parsing
- Request/response handling
- Protocol token parsing

Fuzzing runs nightly via GitHub Actions.

## Performance Considerations

### Connection Pooling

For multi-connection scenarios, use the transport pool:

```rust
let pool = TransportPool::builder()
    .max_connections(100)
    .idle_timeout(Duration::from_secs(300))
    .build();
```

### Async I/O

All I/O operations are async by default. The SDK currently requires Tokio but is designed for future runtime agnosticism (see [ADR-0004](./adr/0004-runtime-agnostic-design.md)).

### Memory Efficiency

- Zero-copy parsing where possible
- Streaming support for large responses
- Lazy schema generation

## Related Documentation

- [Getting Started](./getting-started.md)
- [Tools Guide](./tools.md)
- [Resources Guide](./resources.md)
- [Transport Guide](./transports.md)
- [Error Handling](./error-handling.md)
- [Security](./security.md)
- [Protocol Versions](./protocol-versions.md)
- [Architecture Decision Records](./adr/README.md)
