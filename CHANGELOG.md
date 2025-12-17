# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/praxiomlabs/mcpkit/releases/tag/v0.1.0
