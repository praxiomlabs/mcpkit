# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `ClientBuilder::request_timeout` to configure the per-request response timeout
  (`mcpkit-client`). Defaults to 60 seconds.

### Security

- Updated `Cargo.lock` to patched versions of vulnerable transitive and direct
  dependencies flagged by Dependabot / `cargo audit`:
  `openssl` 0.10.75 → 0.10.80, `rustls-webpki` → 0.103.13, `quinn-proto` →
  0.11.14, `jsonwebtoken` → 10.3.0, `actix-http` → 3.12.1, `bytes` → 1.11.1,
  `time` → 0.3.47, `rsa` → 0.9.10, and `rand` → 0.8.6 / 0.9.3. The remaining
  advisories (`rsa` Marvin timing sidechannel and `rustls-pemfile` unmaintained)
  are dev-only/unfixed and already documented as ignores in `deny.toml`.
- Newline-framed transports now enforce the message-size limit **during** the
  read instead of after, so a peer that streams data without a newline can no
  longer exhaust memory before the cap is checked
  ([#7](https://github.com/praxiomlabs/mcpkit/issues/7)). Covers stdio, spawned
  subprocess, Unix sockets, and Windows named pipes.

### Changed

- **The server now processes requests concurrently** instead of strictly one at
  a time ([#9](https://github.com/praxiomlabs/mcpkit/issues/9)). Requests are
  interleaved on the connection task up to `RuntimeConfig::max_concurrent_requests`
  in flight (default 100); reaching the limit applies backpressure. This also
  makes `max_concurrent_requests` a live setting
  ([#21](https://github.com/praxiomlabs/mcpkit/issues/21)) — set it via
  `ServerRuntime::with_config`.
- **Client requests now time out** instead of waiting indefinitely
  ([#5](https://github.com/praxiomlabs/mcpkit/issues/5)). Each request waits at
  most `request_timeout` (default 60s) for a response and then fails with
  `TransportErrorKind::Timeout`. Clients that issue legitimately long-running
  calls should raise the timeout or use the Tasks API.
- **Extracted LLM orchestration crates to separate [llmtk](https://github.com/praxiomlabs/llmtk) project**
  - The forge orchestration layer (provider, template, memory, embedding, chain, agent, rag, eval)
    has been moved to a dedicated LLM Toolkit workspace to maintain clear separation of concerns
  - mcpkit now focuses solely on MCP protocol implementation
  - See llmtk for LLM provider abstractions, RAG pipelines, agents, and related functionality

### Fixed

- Retry middleware: jitter is now actually randomized, and timeouts are no
  longer retried by default ([#15](https://github.com/praxiomlabs/mcpkit/issues/15)).
  The previous jitter term was always zero (`attempt % 1.0`), so coordinated
  retries didn't spread out; it now uses a real RNG. `DefaultRetryPolicy` no
  longer retries `Timeout` (a timed-out send may already have been delivered, so
  retrying could duplicate a non-idempotent operation) — only connection-level
  errors are retried; supply a custom `RetryPolicy` to opt back in.
- The WebSocket `max_message_size` setting is now actually applied
  ([#13](https://github.com/praxiomlabs/mcpkit/issues/13)). Both the client and
  server build a `tungstenite::WebSocketConfig` from the configured limit and
  pass it via `connect_async_with_config` / `accept_hdr_async_with_config`;
  previously the value was dropped and tungstenite's default was always used.
- The `#[mcp(default = ..., min = ..., max = ...)]` parameter attribute is now
  functional ([#14](https://github.com/praxiomlabs/mcpkit/issues/14)). It was
  documented but a no-op — the parsed attributes were never emitted, so
  generated tool schemas omitted `default`/`minimum`/`maximum`. The macro now
  parses these (and strips the helper attribute, along with parameter doc
  comments, so the impl still compiles) and emits them into the JSON Schema.
- The default in-memory rate limiter now isolates clients per key
  ([#11](https://github.com/praxiomlabs/mcpkit/issues/11)). `InMemoryStore`
  previously used a single global bucket and ignored the key, so one noisy
  client throttled everyone. It now keeps an independent bucket per key, bounded
  by an LRU-evicted map (default 10,000 keys) to cap memory.
- `SpawnedTransport` now actually terminates the child process when dropped
  ([#12](https://github.com/praxiomlabs/mcpkit/issues/12)). The child is spawned
  with `kill_on_drop`, so dropping the transport kills it instead of leaking a
  process; the rustdoc was corrected to match (it previously promised a
  graceful-then-timeout shutdown that wasn't implemented).
- JWT `required_claims` are now actually enforced by the signature-verifying
  validator ([#10](https://github.com/praxiomlabs/mcpkit/issues/10)). Previously
  custom claims were silently ignored and configuring any `required_claims`
  dropped the default `exp`-presence requirement (a per-item
  `set_required_spec_claims` loop that only handled registered claims and
  replaced the set). Required claims are now checked on the decoded token.
- A panicking request handler no longer tears down the whole connection
  ([#9](https://github.com/praxiomlabs/mcpkit/issues/9)). Each request runs with
  panic isolation; a panic is caught and returned as a JSON-RPC internal error,
  and the server keeps serving subsequent requests.
- `Context::cancelled()` no longer busy-spins at 100% CPU while waiting
  ([#8](https://github.com/praxiomlabs/mcpkit/issues/8)). The cancellation
  future now parks on an `event_listener::Event` and is woken by `cancel()`,
  instead of re-waking itself on every poll.
- Connection pool no longer leaks `in_use` capacity
  ([#6](https://github.com/praxiomlabs/mcpkit/issues/6)). A failing connection
  factory now rolls back its reserved slot, and dropping a
  `PooledConnectionGuard` releases its slot, so the pool can no longer drain to
  permanent exhaustion. `in_use`/`peak_in_use` are tracked with atomics so a
  slot can be freed from synchronous (drop) contexts.
- In-flight client requests now fail fast with `ConnectionClosed` when the
  connection drops, instead of hanging until their timeout; pending response
  slots are reclaimed on timeout and disconnect to prevent unbounded growth
  ([#5](https://github.com/praxiomlabs/mcpkit/issues/5))
- Resolved Clippy lints surfaced by newer stable toolchains (`map_unwrap_or`,
  `unnecessary_map_or`, `unnecessary_sort_by`) across `mcpkit-core`,
  `mcpkit-transport`, `mcpkit-server`, and `mcpkit-testing`, restoring a clean
  `clippy -D warnings` on current stable Rust
- Clippy warning for `from_str` method naming in `mcpkit-core::auth::jwt` (renamed to `parse`)
- Clippy warnings in `mcpkit-transport` for single-pattern match expressions

## [0.5.0] - 2025-12-25

### Added

- **gRPC transport** with bidirectional streaming (`mcpkit-transport::grpc`)
  - Full protobuf-based MCP message transport
  - Server and client implementations using tonic
  - Automatic protobuf code generation via prost-build
- **mcpkit-rocket** web framework integration
  - Rocket 0.5 support for MCP servers
  - JSON-RPC endpoint handling
  - Session management with SSE support
- **mcpkit-warp** web framework integration
  - Warp 0.3 support for MCP servers
  - Lightweight alternative to Axum/Actix
  - CORS and session management
- **Framework-specific examples**
  - `rocket-server-example` demonstrating Rocket integration
  - `warp-server-example` demonstrating Warp integration
- **Multi-service distributed architecture example**
  - Gateway pattern with service mesh
  - Tools service and resources service separation
  - Docker Compose and Kubernetes deployment configs
- **Deployment configurations**
  - Docker multi-stage build optimized for production
  - Kubernetes manifests with health checks and resource limits
  - Docker Compose for local development

### Changed

- Updated release workflow to include mcpkit-rocket and mcpkit-warp
- Improved clippy lint configuration for generated protobuf code

### Fixed

- Clippy warnings in generated protobuf code
- Redundant closure warnings in integration tests
- Format string warnings in error formatting

## [0.4.0] - 2025-12-24

### Added

- **`#[mcp_client]` macro** for building MCP clients with handler attributes
  - `#[sampling]` for sampling/create_message handlers
  - `#[elicitation]` for user elicitation handlers
  - `#[roots]` for dynamic root listing
  - Lifecycle hooks: `#[on_connected]`, `#[on_disconnected]`
  - Notification handlers: `#[on_task_progress]`, `#[on_resource_updated]`, etc.
- **Protocol extension infrastructure** (`mcpkit-core::extension`)
  - Extension registry for MCP protocol extensions
  - App discovery and templates support
  - OAuth protected resource discovery endpoints
- **Debug tooling** for protocol inspection (`mcpkit-core::debug`)
  - Session recording and playback
  - Protocol validation utilities
- **Connection pool improvements** with lifecycle management
  - Pre-warming, health checks, and graceful shutdown
  - Configurable idle timeouts and connection limits
- **OpenTelemetry and Prometheus integration** (`mcpkit-transport::telemetry`)
  - Distributed tracing with OpenTelemetry
  - Metrics collection with Prometheus
- **Windows named pipes transport** for Windows IPC
- **Message batching middleware** for improved throughput
- **WASM support** for `wasm32-unknown-unknown` target in mcpkit-core
- **Health check support** in mcpkit-server
- **Async test helpers** in mcpkit-testing
- **Smol runtime example** demonstrating non-Tokio usage
- **Client guide documentation** (`docs/client-guide.md`)
- **1.0 release documentation** with migration guides

### Changed

- Updated prometheus dependency from 0.13 to 0.14 (fixes RUSTSEC-2024-0437)
- Improved test coverage for `#[mcp_client]` macro (18 new tests)

### Removed

- Removed unused `sqlx` dependency from database-server example
- Removed unused `actix-web-actors` dependency from mcpkit-actix

### Fixed

- Fixed `#[non_exhaustive]` on `PoolConfig` and `PoolStats` for future compatibility
- Fixed broken doc link for Windows transport

## [0.3.0] - 2025-12-23

### Added

- **Zero-copy message handling** with `bytes` crate for improved parsing performance
  - New `BufReader::read_line_bytes()` method returns `Bytes` directly
  - `StdioTransport` now uses `serde_json::from_slice` to avoid String allocations
  - Re-exported `Bytes` and `BytesMut` types from mcpkit-transport
- **Filesystem server example** (`examples/filesystem-server/`) demonstrating:
  - Sandboxed file operations with path traversal protection
  - Tools: read_file, write_file, list_directory, search_files, and more
- **Stress testing CI workflow** (`.github/workflows/stress-test.yml`)
  - Criterion benchmarks with performance regression detection
  - Long-running stability tests on schedule
- **Fuzz target** for protocol version parsing (`fuzz_protocol_version`)
- **Developer tooling improvements**:
  - `just install-tools` recipe for automated dev environment setup
  - `just install-tools-minimal` for CI-only tools
  - Updated CONTRIBUTING.md with development tools documentation
- **Integration test script** (`scripts/integration-test.sh`) for comprehensive testing
- **Performance baseline documentation** (`docs/performance-baseline.md`) with Criterion benchmark results
- **Claude Desktop WSL2 guide** (`docs/claude-desktop-wsl2.md`) with verified configuration
- **Client-example improvements**:
  - Now uses filesystem-server for real MCP protocol testing
  - Gracefully handles unsupported methods (resources, prompts)
  - Updated documentation

### Changed

- **Rate limiter optimization**: Replaced manual CAS loop with `fetch_update` for cleaner, more idiomatic code
- **Security advisory handling**: Updated `deny.toml` with documented ignores for:
  - RUSTSEC-2024-0436 (paste via rmcp - dev-dependency only)
- **async-std replaced with smol**: The `async-std-runtime` feature now maps to `smol-runtime`
  - async-std has been discontinued ([RUSTSEC-2025-0052](https://rustsec.org/advisories/RUSTSEC-2025-0052.html))
  - Existing code using `async-std-runtime` will continue to compile (maps to smol)
  - For explicit runtime choice, use `tokio-runtime` (default) or `smol-runtime`

### Removed

- **async-std dependency** removed from mcpkit-transport
  - Feature aliases preserved for backwards compatibility (`async-std` → `smol-runtime`)

### Fixed

- **Filesystem server stdout pollution**: Changed `println!` and tracing to use stderr, keeping stdout clean for JSON-RPC messages
- Various clippy warnings (`map_unwrap_or`, `items_after_statements`, `collapsible_if`)
- cfg-gated imports in websocket server to avoid unused import warnings

## [0.2.5] - 2025-12-17

### Added

- `EventStore` for SSE message resumability in mcpkit-axum and mcpkit-actix (MCP Streamable HTTP spec compliance)
- Re-export `Serialize`, `Deserialize` traits and `json!` macro from mcpkit prelude
- Client message routing integration tests

### Changed

- Removed `Clone` requirement from handler types in mcpkit-actix (API improvement)
- Improved README documentation for mcpkit-axum and mcpkit-actix crates

### Fixed

- **Critical**: Async cancellation bug in `BufReader::read_line()` causing message duplication in `SpawnedTransport`
  - Root cause: Setting `pos=0` before await point caused duplicate reads when futures were cancelled by `tokio::select!`
  - Manifested as "unknown request" warnings when using client with spawned servers
- Justfile `clippy` recipes now use `--workspace` flag to lint all workspace members
- Justfile `examples` recipe now correctly builds workspace packages instead of using `--examples` flag

## [0.2.4] - 2025-12-17

### Added

- `resources/templates/list` support for resource template discovery
- `McpRouter` struct in mcpkit-axum and mcpkit-actix for type-safe route mounting
- Exported routing functions (`route_prompts`, `route_resources`, `route_tools`) from mcpkit-server

### Changed

- Unified HTTP crate APIs: renamed `McpConfig` to `McpRouter` in mcpkit-actix for consistency with mcpkit-axum
- All MCP request methods now route through handler traits for consistent behavior
- HTTP integration ergonomics improved with builder pattern refinements

### Fixed

- Protocol version references updated from 2025-06-18 to 2025-11-25 across all crates
- Crate consistency issues preventing crates.io publishing (missing route_* exports)
- Documentation standardized across all crates

## [0.2.3] - 2025-12-17

### Added

- `From<String>` and `From<&str>` implementations for `ToolOutput` for ergonomic returns
- Expansion tests for resource-only and prompt-only servers
- Tool annotation documentation with usage examples
- Error handling guidance for `ToolOutput::error()` vs `Result<ToolOutput, McpError>`
- Transport availability documentation table with feature flags
- Stateful handler example in minimal-server demonstrating `AtomicU64` usage

### Changed

- Split `error.rs` (1200+ lines) into focused submodules: `types`, `codes`, `context`, `details`, `jsonrpc`, `transport`
- Split `http.rs` (42KB) into submodules: `client`, `server`, `sse`, `config`
- Split `websocket.rs` (36KB) into submodules: `client`, `server`, `config`
- Split `pool.rs` (36KB) into submodules: `config`, `connection`, `manager`
- Added 673+ `#[must_use]` annotations across all crates for clearer API semantics
- Server `initialize` response now uses handler's `server_info()` instead of hardcoded values

### Fixed

- Server name/version attributes from `#[mcp_server]` macro now properly appear in initialize response
- Unused import warnings for feature-gated HTTP headers
- Macro-generated code now uses facade crate paths (`::mcpkit::`) for proper resolution

## [0.2.2] - 2025-12-17

### Fixed

- Eliminated panic path in rate limiter when sliding window exceeds process uptime

### Added

- Troubleshooting guide documentation
- Release checklist for systematic release validation
- Justfile recipes for release workflow (`wip-check`, `panic-audit`, `metadata-check`)
- Code coverage CI job with Codecov integration

### Changed

- Documentation version references updated from 0.1 to 0.2
- Architecture diagram crate names corrected (`mcp-*` to `mcpkit-*`)
- MSRV reference updated in CONTRIBUTING.md (1.75 to 1.85)
- Dockerfile base image updated to `rust:1.85-bookworm`
- Codecov configuration paths updated to current crate structure
- Advisory ignore documented in deny.toml (RUSTSEC-2025-0052)

## [0.2.1] - 2025-12-13

### Added

- `ToolBuilder` annotation methods: `destructive()`, `idempotent()`, `read_only()`
- Warning log when server returns unknown protocol version (falls back to latest)
- Comprehensive test coverage for:
  - Tool annotations and metadata
  - Protocol version edge cases
  - HTTP session recovery
  - Resource template URI matching
  - Async cancellation propagation

### Fixed

- HTTP header casing changed to lowercase for HTTP/2 compatibility (`mcp-session-id`, `mcp-protocol-version`)
- Clarified TODO comment in macro crate (annotations were already implemented)

### Changed

- Updated documentation to reflect lowercase HTTP headers

## [0.2.0] - 2025-12-12

### Added

- Client APIs for Tasks (list, get, cancel)
- Client APIs for Completions (prompt arguments, resource arguments)
- Client resource subscription support (subscribe, unsubscribe)
- Client progress callback handling via `ClientHandler` trait
- Server-level request metrics (`ServerMetrics`)
- Comprehensive error scenario tests
- Middleware interaction tests
- Async cancellation tests
- Justfile for modern development workflow (73 recipes)

### Changed

- Expanded custom transport documentation with Redis example
- Enhanced security documentation with OWASP Top 10 alignment

## [0.1.0] - 2025-12-11

### Added

- Initial release of the Rust MCP SDK
- Unified `#[mcp_server]` macro for defining MCP servers
- `#[tool]` attribute for defining tools with automatic schema generation
- `#[resource]` attribute for defining resource handlers
- `#[prompt]` attribute for defining prompt handlers
- `#[derive(ToolInput)]` for generating JSON Schema from structs
- Full MCP 2025-11-25 protocol support
- Tasks capability for long-running operations
- Multiple transport implementations:
  - Standard I/O (stdio)
  - HTTP with Server-Sent Events (SSE)
  - WebSocket with auto-reconnect
  - Unix domain sockets
  - In-memory transport for testing
- Connection pooling for both transports and clients
- Middleware layer system:
  - Logging middleware
  - Timeout middleware
  - Retry middleware with exponential backoff
  - Metrics middleware
- Typestate pattern for connection lifecycle
- Rich error handling with context chains
- Comprehensive test suite
- Example servers (minimal-server, full-server, database-server)
- Client library with connection pooling
- Server discovery for stdio-based servers
- `mcpkit-testing` crate for test utilities
- Protocol version detection and capability negotiation

[Unreleased]: https://github.com/praxiomlabs/mcpkit/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.5...v0.3.0
[0.2.5]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/praxiomlabs/mcpkit/releases/tag/v0.1.0
