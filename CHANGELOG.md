# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- **async-std deprecation notice**: Updated `docs/runtimes.md` with RUSTSEC-2025-0052 advisory warning
- **Security advisory handling**: Updated `deny.toml` with documented ignores for:
  - RUSTSEC-2023-0071 (rsa via sqlx-mysql - not used, sqlite only)
  - RUSTSEC-2024-0436 (paste via rmcp - dev-dependency only)

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

[Unreleased]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.5...HEAD
[0.2.5]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/praxiomlabs/mcpkit/releases/tag/v0.1.0
