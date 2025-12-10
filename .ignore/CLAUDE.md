# Rust MCP SDK - Project Instructions

## Project Overview

This is the Rust MCP SDK, a production-grade implementation of the Model Context Protocol.
The implementation follows the plan in `PLAN.md` (copied from `~/.claude/plans/elegant-snacking-treasure.md`).

**CRITICAL: All work on this project MUST follow that plan exactly. No deviations. No interpretations. No shortcuts.**

**The plan has been validated by research.** See `RESEARCH_FINDINGS.md` for evidence-based justification of all major design decisions.

## Why This SDK Exists

The official Rust MCP SDK is Tokio-only, blocking users who need:
- async-std or smol for embedded/WASM/resource-constrained environments
- Single-threaded async without Arc overhead
- Flexible runtime selection

**Source:** [GitHub Issue #379](https://github.com/modelcontextprotocol/rust-sdk/issues/379)

This SDK fills that gap with proper runtime agnosticism.

## Core Requirements (Non-Negotiable)

### 1. Runtime Agnosticism
- Must support Tokio, async-std, AND smol
- NO direct usage of `tokio::` types in core abstractions
- Use `futures` crate primitives for runtime-agnostic code
- Feature flags: `tokio-runtime`, `async-std-runtime`, `smol-runtime`
- Each runtime needs ACTUAL IMPLEMENTATION, not just feature flags

**Why:** Per [Rust Async Book](https://rust-lang.github.io/async-book/08_ecosystem/00_chapter.html):
> "Libraries exposing async APIs should not depend on a specific executor or reactor, unless they need to spawn tasks or define their own async I/O or timer futures."

### 2. Borrowing-Friendly Context
The Context type MUST use lifetime parameters, NOT Arc:
```rust
pub struct Context<'a> {
    pub request_id: &'a RequestId,
    pub progress_token: Option<&'a ProgressToken>,
    pub client_caps: &'a ClientCapabilities,
    pub server_caps: &'a ServerCapabilities,
    peer: &'a dyn Peer,
    cancel: CancellationToken,
}
```
This enables zero `'static` requirements on handlers.

**Why:** Lifetime-based context allows:
- Single-threaded async without Arc overhead
- `!Send` types in handlers (important for some runtimes)
- Users who need spawning can wrap in Arc themselves - more flexible than forcing Arc on everyone

### 3. ServerBuilder Typestate
Must track registered handlers at the type level:
```rust
pub struct ServerBuilder<H, Tools, Resources, Prompts, Tasks> {
    handler: H,
    tools: Tools,
    resources: Resources,
    prompts: Prompts,
    tasks: Tasks,
    middleware: Vec<Box<dyn Layer>>,
}
```

**Why:** Per [Typestate Pattern](https://cliffle.com/blog/rust-typestate/):
> "Typestates move properties of state into the type level, allowing the compiler to check ahead-of-time."

This catches "capability not registered" errors at compile time, not runtime.

### 4. Middleware Layer System
Must implement Tower-compatible middleware:
```rust
pub trait TransportLayer<T: Transport> {
    type Transport: Transport;
    fn layer(&self, inner: T) -> Self::Transport;
}

pub struct LayerStack<T> { inner: T }
pub struct LoggingLayer { pub level: Level }
pub struct TimeoutLayer { pub send: Duration, pub recv: Duration }
pub struct RetryLayer { pub max_attempts: u32, pub backoff: ExponentialBackoff }
pub struct MetricsLayer { pub registry: Registry }
```

**Why:** Per [Tower Service Trait](https://docs.rs/tower-service/latest/tower_service/trait.Service.html):
- Tower is runtime-agnostic (no runtime mandated at trait level)
- Industry standard for Rust middleware (Axum, Hyper, Tonic use it)
- Composable and reusable

### 5. Full Macro Implementation
These macros must be FULLY FUNCTIONAL, not stubs:
- `#[mcp_server]` - Generates all trait implementations
- `#[tool]` - Full parameter extraction from signature
- `#[resource]` - URI pattern matching, resource handling
- `#[prompt]` - Prompt template handling
- `#[derive(ToolInput)]` - JSON Schema generation from struct
- `#[mcp(default, range)]` - Parameter validation attributes

### 6. TaskManager Runtime
Full async task tracking implementation:
```rust
pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<TaskId, TaskState>>>,
    peer: Arc<dyn Peer>,
}
```
With methods: create, handle, get, cancel, list

## Required File Structure

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
│   │       ├── state.rs            # Typestate connection (from Section 1.3)
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
│   ├── mcp-transport/src/
│   │   ├── lib.rs
│   │   ├── traits.rs
│   │   ├── stdio.rs
│   │   ├── http.rs             # SSE streaming
│   │   ├── websocket.rs        # First-class, ping/pong
│   │   ├── unix.rs             # Unix domain sockets
│   │   ├── memory.rs
│   │   ├── pool.rs             # Connection pooling
│   │   └── middleware/
│   │       ├── mod.rs
│   │       ├── logging.rs
│   │       ├── timeout.rs
│   │       ├── retry.rs
│   │       └── metrics.rs
│   │
│   ├── mcp-server/src/
│   │   ├── lib.rs
│   │   ├── handler.rs
│   │   ├── builder.rs          # 5 type parameters
│   │   ├── server.rs           # Server runtime
│   │   ├── context.rs          # Context<'a> with lifetimes
│   │   ├── router.rs           # Method routing
│   │   ├── state.rs            # Server-side state management
│   │   └── capability/
│   │       ├── mod.rs
│   │       ├── tools.rs
│   │       ├── resources.rs
│   │       ├── prompts.rs
│   │       ├── tasks.rs        # TaskManager, TaskHandle
│   │       ├── sampling.rs
│   │       ├── elicitation.rs
│   │       └── completions.rs
│   │
│   ├── mcp-client/src/
│   │   ├── lib.rs
│   │   ├── client.rs
│   │   ├── handler.rs
│   │   ├── discovery.rs
│   │   └── pool.rs             # Connection pool
│   │
│   ├── mcp-macros/src/
│   │   ├── lib.rs
│   │   ├── server.rs
│   │   ├── tool.rs
│   │   ├── resource.rs         # FUNCTIONAL implementation
│   │   ├── prompt.rs           # FUNCTIONAL implementation
│   │   ├── derive.rs           # FUNCTIONAL ToolInput derive
│   │   ├── codegen.rs
│   │   └── error.rs
│   │
│   └── mcp-testing/src/
│       ├── lib.rs
│       ├── mock.rs
│       ├── fixtures.rs
│       └── assertions.rs
│
├── mcp/src/
│   ├── lib.rs
│   └── prelude.rs              # SEPARATE file
│
├── examples/
│   ├── minimal-server/
│   ├── full-server/
│   ├── http-server/            # Required
│   ├── websocket-server/       # Required
│   ├── client-example/         # Required
│   └── with-middleware/        # Required
│
└── tests/
    ├── integration/            # Required
    └── protocol_compliance/    # Required
```

## Implementation Order (Follow Exactly)

### Sprint 1: Foundation
1. Workspace structure
2. mcp-core: error types, protocol types, JSON-RPC
3. Typestate connection model
4. Comprehensive tests for core types

### Sprint 2: Transport
5. Transport trait, stdio transport
6. HTTP/SSE transport
7. WebSocket transport
8. Unix socket transport
9. Middleware layer system (ALL: logging, timeout, retry, metrics)

### Sprint 3: Server
10. Handler traits, builder, routing
11. All capability handlers (tools, resources, prompts)
12. Tasks (full protocol support with TaskManager)
13. Sampling, elicitation, completions

### Sprint 4: Macros
14. #[mcp_server] macro
15. #[tool], #[resource], #[prompt] attributes (ALL FUNCTIONAL)
16. #[derive(ToolInput)] for complex parameters
17. Rich compile-time error messages

### Sprint 5: Client & Polish
18. mcp-client: client handler, discovery, pool
19. mcp-testing: mocks, fixtures
20. Facade crate with prelude
21. ALL examples (minimal, full, HTTP, WebSocket, client, middleware)
22. 100% public API documentation

## Success Criteria

- [ ] All MCP 2025-11-25 protocol features implemented
- [ ] Tasks fully working (rmcp's biggest gap)
- [ ] 66%+ boilerplate reduction vs rmcp
- [ ] Zero `'static` requirements on context
- [ ] Runtime-agnostic (Tokio, async-std, smol) - ALL THREE WORKING
- [ ] 100% public API documentation
- [ ] All examples compile and run
- [ ] Protocol compliance tests pass

## What NOT To Do

1. Do NOT use `Arc` where the plan specifies lifetime references
2. Do NOT create stub implementations that "pass through"
3. Do NOT skip runtime implementations (all three must work)
4. Do NOT merge files the plan says should be separate
5. Do NOT skip the middleware system
6. Do NOT leave macros non-functional
7. Do NOT deviate from the file structure
8. Do NOT skip examples or tests

## Verification Process

Before marking ANY task complete:
1. Compare implementation against plan's code snippets CHARACTER BY CHARACTER
2. Verify file exists at EXACT path specified in plan
3. Verify all methods/traits match plan signatures
4. Run tests to confirm functionality
5. Check that no `tokio::` appears in runtime-agnostic code

## Current Status Tracking

See `IMPLEMENTATION_STATUS.md` for detailed status of each component.
