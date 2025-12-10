# Rust MCP SDK: Superior Implementation Plan

## Executive Summary

Build a new Rust MCP SDK from scratch at `~/rust-mcp-sdk` that dramatically improves upon rmcp through:
- **66% less boilerplate** via unified `#[mcp_server]` macro
- **Runtime-agnostic** async support (Tokio, async-std, smol)
- **Type-safe state machines** via typestate pattern for connection lifecycle
- **Rich error handling** with context chains and miette diagnostics
- **Full MCP 2025-11-25 protocol coverage** including Tasks (which rmcp lacks)
- **First-class middleware** via Tower-compatible Layer pattern

**Target:** Production AI applications (primary), SDK/library authors (secondary)
**MSRV:** Rust 1.75+ (enables native async fn in traits)

---

## Project Structure

```
~/rust-mcp-sdk/
├── Cargo.toml                      # Workspace root
├── README.md                       # Project documentation
├── CHANGELOG.md                    # Version history
├── LICENSE-MIT / LICENSE-APACHE    # Dual licensing
├── crates/
│   ├── mcp-core/                   # Protocol types, traits (no async runtime)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── protocol.rs         # JSON-RPC 2.0 types
│   │       ├── message.rs          # Request/Response/Notification
│   │       ├── capability.rs       # Capability flags
│   │       ├── error.rs            # Unified McpError with thiserror
│   │       ├── types/
│   │       │   ├── mod.rs
│   │       │   ├── tool.rs         # Tool, ToolResult, CallToolResult
│   │       │   ├── resource.rs     # Resource, ResourceContents
│   │       │   ├── prompt.rs       # Prompt, PromptResult
│   │       │   ├── task.rs         # Task, TaskStatus (NEW vs rmcp)
│   │       │   ├── sampling.rs     # Sampling types
│   │       │   ├── elicitation.rs  # Elicitation types
│   │       │   └── content.rs      # Content (text, image, audio)
│   │       └── schema.rs           # JSON Schema utilities
│   │
│   ├── mcp-transport/              # Transport abstractions
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs           # Transport, TransportListener traits
│   │       ├── stdio.rs            # Standard I/O transport
│   │       ├── http.rs             # Streamable HTTP transport
│   │       ├── websocket.rs        # WebSocket transport (first-class)
│   │       ├── unix.rs             # Unix domain socket (cfg unix)
│   │       ├── memory.rs           # In-process transport (testing)
│   │       ├── pool.rs             # Connection pooling
│   │       └── middleware/
│   │           ├── mod.rs
│   │           ├── logging.rs
│   │           ├── timeout.rs
│   │           ├── retry.rs
│   │           └── metrics.rs
│   │
│   ├── mcp-server/                 # Server implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── handler.rs          # Composable handler traits
│   │       ├── builder.rs          # Fluent ServerBuilder
│   │       ├── server.rs           # Server runtime
│   │       ├── context.rs          # RequestContext (borrowing-friendly)
│   │       ├── router.rs           # Method routing
│   │       ├── state.rs            # Typestate connection management
│   │       └── capability/
│   │           ├── mod.rs
│   │           ├── tools.rs
│   │           ├── resources.rs
│   │           ├── prompts.rs
│   │           ├── tasks.rs        # Full Tasks implementation
│   │           ├── sampling.rs
│   │           ├── elicitation.rs
│   │           └── completions.rs
│   │
│   ├── mcp-client/                 # Client implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs           # MCP client
│   │       ├── handler.rs          # ClientHandler trait
│   │       ├── discovery.rs        # Server discovery
│   │       └── pool.rs             # Client connection pool
│   │
│   ├── mcp-macros/                 # Procedural macros
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # #[mcp_server], #[mcp_client]
│   │       ├── server.rs           # Server macro implementation
│   │       ├── tool.rs             # #[tool] attribute
│   │       ├── resource.rs         # #[resource] attribute
│   │       ├── prompt.rs           # #[prompt] attribute
│   │       ├── derive.rs           # #[derive(ToolInput)]
│   │       ├── codegen.rs          # Code generation utilities
│   │       └── error.rs            # Rich compile-time errors
│   │
│   └── mcp-testing/                # Test utilities
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── mock.rs             # Mock server/client
│           ├── fixtures.rs         # Test fixtures
│           └── assertions.rs       # Custom assertions
│
├── mcp/                            # Facade crate (re-exports)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── prelude.rs
│
├── examples/
│   ├── minimal-server/
│   ├── full-server/
│   ├── http-server/
│   ├── websocket-server/
│   ├── client-example/
│   └── with-middleware/
│
└── tests/
    ├── integration/
    └── protocol_compliance/
```

---

## Phase 1: Foundation (mcp-core)

### 1.1 Error System

**File:** `crates/mcp-core/src/error.rs`

```rust
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum McpError {
    #[error("Parse error: {message}")]
    #[diagnostic(code(mcp::protocol::parse))]
    Parse { message: String, #[source] source: Option<BoxError> },

    #[error("Invalid params for '{method}': {message}")]
    #[diagnostic(code(mcp::protocol::invalid_params))]
    InvalidParams {
        method: String,
        message: String,
        param_path: Option<String>,
        expected: Option<String>,
    },

    #[error("Method not found: {method}")]
    MethodNotFound { method: String, available: Vec<String> },

    #[error("Transport error: {kind}")]
    Transport { kind: TransportErrorKind, #[source] source: Option<BoxError> },

    #[error("Tool '{tool}' failed: {message}")]
    ToolExecution { tool: String, message: String, is_recoverable: bool },

    #[error("{context}")]
    #[diagnostic(transparent)]
    WithContext { context: String, #[source] source: Box<McpError> },
    // ... other variants
}

// Context extension trait
pub trait McpResultExt<T> {
    fn context<C: Into<String>>(self, ctx: C) -> Result<T, McpError>;
    fn with_context<C, F: FnOnce() -> C>(self, f: F) -> Result<T, McpError> where C: Into<String>;
}
```

### 1.2 Protocol Types

**File:** `crates/mcp-core/src/types/tool.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: serde_json::Value,
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAnnotations {
    pub title: Option<String>,
    pub read_only_hint: Option<bool>,
    pub destructive_hint: Option<bool>,
    pub idempotent_hint: Option<bool>,
    pub open_world_hint: Option<bool>,
}

// Simplified result type for tool handlers
pub enum ToolOutput {
    Success(CallToolResult),
    RecoverableError { message: String, suggestion: Option<String> },
}
```

### 1.3 Typestate Connection

**File:** `crates/mcp-core/src/state.rs`

```rust
pub mod state {
    pub struct Disconnected;
    pub struct Connected;
    pub struct Initializing;
    pub struct Ready;
    pub struct Closing;
}

pub struct Connection<S> {
    inner: ConnectionInner,
    _state: PhantomData<S>,
}

impl Connection<state::Connected> {
    pub async fn initialize(self, params: InitParams) -> Result<Connection<state::Initializing>, McpError>;
}

impl Connection<state::Initializing> {
    pub async fn complete(self) -> Result<Connection<state::Ready>, McpError>;
}

impl Connection<state::Ready> {
    pub async fn request<R: McpRequest>(&self, req: R) -> Result<R::Response, McpError>;
    pub async fn shutdown(self) -> Result<Connection<state::Closing>, McpError>;
}
```

---

## Phase 2: Transport Layer (mcp-transport)

### 2.1 Runtime-Agnostic Transport Trait

**File:** `crates/mcp-transport/src/traits.rs`

```rust
use std::future::Future;

/// Core transport trait - runtime agnostic via associated types
pub trait Transport: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn send(&self, msg: JsonRpcMessage) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn recv(&self) -> impl Future<Output = Result<Option<JsonRpcMessage>, Self::Error>> + Send;
    fn close(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn is_connected(&self) -> bool;
    fn metadata(&self) -> TransportMetadata;
}

/// Listener for server-side transports
pub trait TransportListener: Send + Sync {
    type Transport: Transport;
    type Error: std::error::Error + Send + Sync + 'static;

    fn accept(&self) -> impl Future<Output = Result<Self::Transport, Self::Error>> + Send;
    fn local_addr(&self) -> Option<String>;
}
```

### 2.2 Middleware Layer Pattern

**File:** `crates/mcp-transport/src/middleware/mod.rs`

```rust
pub trait TransportLayer<T: Transport> {
    type Transport: Transport;
    fn layer(&self, inner: T) -> Self::Transport;
}

pub struct LayerStack<T> {
    inner: T,
}

impl<T: Transport> LayerStack<T> {
    pub fn new(transport: T) -> Self { Self { inner: transport } }

    pub fn with<L: TransportLayer<T>>(self, layer: L) -> LayerStack<L::Transport> {
        LayerStack { inner: layer.layer(self.inner) }
    }
}

// Built-in middleware
pub struct LoggingLayer { pub level: Level }
pub struct TimeoutLayer { pub send: Duration, pub recv: Duration }
pub struct RetryLayer { pub max_attempts: u32, pub backoff: ExponentialBackoff }
pub struct MetricsLayer { pub registry: Registry }
```

### 2.3 Built-in Transports

| Transport | File | Features |
|-----------|------|----------|
| Stdio | `stdio.rs` | Newline-delimited JSON, stderr logging |
| HTTP | `http.rs` | SSE streaming, session management |
| WebSocket | `websocket.rs` | Ping/pong, auto-reconnect |
| Unix | `unix.rs` | Unix domain sockets (cfg unix) |
| Memory | `memory.rs` | In-process channels for testing |

---

## Phase 3: Server Implementation (mcp-server)

### 3.1 Composable Handler Traits

**File:** `crates/mcp-server/src/handler.rs`

```rust
/// Minimal required trait
pub trait ServerHandler: Send + Sync {
    fn server_info(&self) -> ServerInfo;
    fn on_initialized(&self, _ctx: &Context) -> impl Future<Output = ()> + Send { async {} }
}

/// Optional capability traits - implement what you need
pub trait ToolHandler: Send + Sync {
    fn list_tools(&self, ctx: &Context) -> impl Future<Output = Result<Vec<Tool>, McpError>> + Send;
    fn call_tool(&self, name: &str, args: Value, ctx: &Context)
        -> impl Future<Output = Result<ToolOutput, McpError>> + Send;
}

pub trait ResourceHandler: Send + Sync { /* ... */ }
pub trait PromptHandler: Send + Sync { /* ... */ }
pub trait TaskHandler: Send + Sync { /* ... */ }  // NEW - rmcp doesn't have this
pub trait SamplingHandler: Send + Sync { /* ... */ }
pub trait ElicitationHandler: Send + Sync { /* ... */ }
pub trait CompletionHandler: Send + Sync { /* ... */ }
```

### 3.2 Borrowing-Friendly Context

**File:** `crates/mcp-server/src/context.rs`

```rust
/// Request context - passed by reference, NO 'static requirement
pub struct Context<'a> {
    pub request_id: &'a RequestId,
    pub progress_token: Option<&'a ProgressToken>,
    pub client_caps: &'a ClientCapabilities,
    pub server_caps: &'a ServerCapabilities,
    peer: &'a dyn Peer,
    cancel: CancellationToken,
}

impl<'a> Context<'a> {
    pub async fn notify<N: McpNotification>(&self, n: N) -> Result<(), McpError>;
    pub async fn progress(&self, current: u64, total: Option<u64>, msg: Option<&str>) -> Result<(), McpError>;
    pub fn is_cancelled(&self) -> bool;
    pub fn cancelled(&self) -> impl Future<Output = ()> + '_;
}
```

### 3.3 Server Builder

**File:** `crates/mcp-server/src/builder.rs`

```rust
pub struct ServerBuilder<H, Tools, Resources, Prompts, Tasks> {
    handler: H,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    tasks: Tasks,
    middleware: Vec<Box<dyn Layer>>,
}

impl<H: ServerHandler> ServerBuilder<H, (), (), (), ()> {
    pub fn new(handler: H) -> Self;
}

impl<H, T, R, P, K> ServerBuilder<H, T, R, P, K> {
    pub fn with_tools<TH: ToolHandler>(self, h: TH) -> ServerBuilder<H, TH, R, P, K>;
    pub fn with_resources<RH: ResourceHandler>(self, h: RH) -> ServerBuilder<H, T, RH, P, K>;
    pub fn with_tasks<KH: TaskHandler>(self, h: KH) -> ServerBuilder<H, T, R, P, KH>;
    pub fn layer<L: Layer>(self, l: L) -> Self;
    pub fn build(self) -> Server<...>;
    pub async fn serve<Tr: Transport>(self, tr: Tr) -> Result<RunningServer, McpError>;
}
```

---

## Phase 4: Macro System (mcp-macros)

### 4.1 Unified `#[mcp_server]` Macro

**Before (rmcp - 4 macros, manual wiring):**
```rust
#[derive(Clone)]
struct MyServer { tool_router: ToolRouter<Self> }

#[tool_router]
impl MyServer {
    fn new() -> Self { Self { tool_router: Self::tool_router() } }

    #[tool(description = "Add numbers")]
    async fn add(&self, Parameters(p): Parameters<AddInput>) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::text((p.a + p.b).to_string()))
    }
}

#[tool_handler]
impl ServerHandler for MyServer { /* ... */ }
```

**After (new SDK - 1 macro, zero boilerplate):**
```rust
struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text((a + b).to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    Calculator.serve_stdio().await
}
```

**Code reduction: 66%**

### 4.2 Macro Attributes

```rust
#[mcp_server(
    name = "server-name",
    version = "1.0.0",                    // or env!("CARGO_PKG_VERSION")
    instructions = "Usage instructions",
    capabilities(tools, resources, prompts, tasks),
    debug_expand = false,                 // Print generated code
)]

#[tool(
    description = "Required description",
    name = "override_name",               // Optional, defaults to fn name
    destructive = false,                  // Annotation hints
    idempotent = true,
)]

#[resource(
    uri_pattern = "myserver://data/{id}",
    name = "Resource Name",
    mime_type = "application/json",
)]

#[prompt(description = "Prompt description")]
```

### 4.3 Direct Parameter Extraction

```rust
// Parameters extracted from function signature (no wrapper types!)
#[tool(description = "Search items")]
async fn search(
    &self,
    /// The search query (becomes JSON Schema description)
    query: String,
    /// Max results (1-100)
    #[mcp(default = 10, range(1, 100))]
    limit: usize,
    /// Optional category filter
    category: Option<String>,
    #[context] ctx: &Context,  // Explicit context access
) -> ToolOutput {
    // ...
}
```

### 4.4 Rich Compile-Time Errors

```
error: unknown attribute `descripion`
  --> src/server.rs:15:8
   |
15 | #[tool(descripion = "Typo")]
   |        ^^^^^^^^^^
   |
   = help: did you mean `description`?
   = note: valid attributes: description, name, destructive, idempotent
```

---

## Phase 5: Full Protocol Coverage

### 5.1 Tasks Implementation (NEW - rmcp doesn't have this)

**File:** `crates/mcp-server/src/capability/tasks.rs`

```rust
pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<TaskId, TaskState>>>,
    peer: Arc<dyn Peer>,
}

impl TaskManager {
    pub async fn create(&self, tool: &str) -> TaskId;
    pub fn handle(&self, id: TaskId) -> TaskHandle;
    pub async fn get(&self, id: &TaskId) -> Option<TaskState>;
    pub async fn cancel(&self, id: &TaskId) -> Result<(), McpError>;
    pub async fn list(&self) -> Vec<TaskSummary>;
}

pub struct TaskHandle {
    task_id: TaskId,
    tx: mpsc::Sender<TaskUpdate>,
}

impl TaskHandle {
    pub async fn running(&self);
    pub async fn progress(&self, current: u64, total: Option<u64>, msg: Option<&str>);
    pub async fn complete(&self, result: Value);
    pub async fn error(&self, err: ErrorData);
}
```

### 5.2 Feature Coverage Matrix

| Feature | mcp-core | mcp-server | mcp-client | rmcp Status |
|---------|----------|------------|------------|-------------|
| JSON-RPC 2.0 | ✅ | ✅ | ✅ | ✅ |
| Tools | ✅ | ✅ | ✅ | ✅ |
| Resources | ✅ | ✅ | ✅ | ✅ |
| Prompts | ✅ | ✅ | ✅ | ✅ |
| **Tasks** | ✅ | ✅ | ✅ | ❌ Missing |
| Sampling | ✅ | ✅ | ✅ | ✅ |
| Elicitation | ✅ | ✅ | ✅ | ✅ |
| Completions | ✅ | ✅ | ✅ | ✅ |
| Stdio | - | ✅ | ✅ | ✅ |
| HTTP/SSE | - | ✅ | ✅ | ✅ |
| **WebSocket** | - | ✅ | ✅ | 🔧 Custom |
| **Unix Socket** | - | ✅ | ✅ | 🔧 Custom |
| **Middleware** | - | ✅ | ✅ | ❌ Manual |
| **Runtime-agnostic** | ✅ | ✅ | ✅ | ❌ Tokio-only |

---

## Implementation Order

### Sprint 1: Foundation
1. [ ] Create workspace structure at `~/rust-mcp-sdk`
2. [ ] Implement `mcp-core`: error types, protocol types, JSON-RPC
3. [ ] Implement typestate connection model
4. [ ] Write comprehensive tests for core types

### Sprint 2: Transport
5. [ ] Implement `mcp-transport`: Transport trait, stdio transport
6. [ ] Add HTTP/SSE transport
7. [ ] Add WebSocket transport (first-class, not custom)
8. [ ] Add Unix socket transport
9. [ ] Implement middleware layer system (logging, timeout, retry)

### Sprint 3: Server
10. [ ] Implement `mcp-server`: handler traits, builder, routing
11. [ ] Implement all capability handlers (tools, resources, prompts)
12. [ ] Implement Tasks (full protocol support)
13. [ ] Implement sampling, elicitation, completions

### Sprint 4: Macros
14. [ ] Implement `mcp-macros`: #[mcp_server] macro
15. [ ] Implement #[tool], #[resource], #[prompt] attributes
16. [ ] Implement #[derive(ToolInput)] for complex parameters
17. [ ] Add rich compile-time error messages

### Sprint 5: Client & Polish
18. [ ] Implement `mcp-client`: client handler, discovery
19. [ ] Implement `mcp-testing`: mocks, fixtures
20. [ ] Create facade crate with prelude
21. [ ] Write examples (minimal, full, HTTP, WebSocket)
22. [ ] Documentation: 100% public API coverage

---

## Key Differentiators from rmcp

| Aspect | rmcp | New SDK |
|--------|------|---------|
| Macros | 4 interdependent | 1 unified `#[mcp_server]` |
| Boilerplate | Manual router wiring | Zero initialization |
| Parameters | `Parameters<T>` wrapper | Direct from signature |
| Error types | 3 nested layers | 1 unified `McpError` |
| Error context | Lost through stack | Full context chains |
| Lifetime | `'static` on send | Borrowing-friendly |
| Tasks | Not implemented | Full support |
| WebSocket | Custom implementation | First-class |
| Middleware | Manual/Tower separate | Built-in Layer system |
| Async runtime | Tokio-only | Runtime-agnostic |
| Documentation | 30.78% coverage | 100% target |
| Files | 2250-line model.rs | ~150 lines per file |

---

## Critical Files to Create First

1. `~/rust-mcp-sdk/Cargo.toml` - Workspace configuration
2. `~/rust-mcp-sdk/crates/mcp-core/src/error.rs` - Unified error system
3. `~/rust-mcp-sdk/crates/mcp-core/src/protocol.rs` - JSON-RPC types
4. `~/rust-mcp-sdk/crates/mcp-transport/src/traits.rs` - Transport abstraction
5. `~/rust-mcp-sdk/crates/mcp-server/src/handler.rs` - Handler traits
6. `~/rust-mcp-sdk/crates/mcp-macros/src/lib.rs` - Macro entry point

---

## Success Criteria

- [ ] All MCP 2025-11-25 protocol features implemented
- [ ] Tasks fully working (rmcp's biggest gap)
- [ ] 66%+ boilerplate reduction vs rmcp
- [ ] Zero `'static` requirements on context
- [ ] Runtime-agnostic (Tokio, async-std, smol)
- [ ] 100% public API documentation
- [ ] All examples compile and run
- [ ] Protocol compliance tests pass
