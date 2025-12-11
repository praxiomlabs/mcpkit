# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial release of the Rust MCP SDK
- Unified `#[mcp_server]` macro for defining MCP servers
- `#[tool]` attribute for defining tools with automatic schema generation
- `#[resource]` attribute for defining resource handlers
- `#[prompt]` attribute for defining prompt handlers
- `#[derive(ToolInput)]` for generating JSON Schema from structs
- Full MCP 2025-11-25 protocol support
- Tasks capability (not available in rmcp)
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
- Example servers (minimal-server, full-server)

### Changed

- N/A (initial release)

### Deprecated

- N/A (initial release)

### Removed

- N/A (initial release)

### Fixed

- N/A (initial release)

### Security

- N/A (initial release)

## [0.1.0] - 2025-12-11

Initial release.
