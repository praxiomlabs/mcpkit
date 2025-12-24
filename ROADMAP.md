# mcpkit Roadmap to 1.0

This document outlines the path to a stable 1.0 release of mcpkit.

## Current Status: v0.3.0 (Pre-release)

mcpkit is currently in active development with a stable API surface. The SDK implements the full MCP 2025-11-25 specification and is suitable for production use, though the API may still change before 1.0.

## 1.0 Release Criteria

### Core Requirements (Must Have)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Full MCP 2025-11-25 compliance | ✅ Complete | All protocol features implemented |
| Protocol version negotiation | ✅ Complete | Supports 4 protocol versions |
| OAuth 2.1 + PKCE support | ✅ Complete | RFC 9728, 8414, 7636 compliant |
| Tasks (async operations) | ✅ Complete | Full task lifecycle support |
| Elicitation | ✅ Complete | Server-initiated user input |
| Tool/Resource/Prompt handlers | ✅ Complete | Full MCP primitives |
| Multiple transport support | ✅ Complete | stdio, HTTP/SSE, WebSocket, Unix |
| Client SDK | ✅ Complete | Connection management, retries |
| Server SDK | ✅ Complete | Handler traits, routing |
| Axum integration | ✅ Complete | Router, SSE, OAuth discovery |
| Actix-web integration | ✅ Complete | Router, SSE, OAuth discovery |
| Extension infrastructure | ✅ Complete | Structured extension support |
| Comprehensive documentation | ✅ Complete | 28+ doc files, ADRs |
| Test coverage | ✅ Complete | 100+ tests, integration tests |
| Fuzzing | ✅ Complete | 6 fuzz targets, CI integration |
| Zero clippy warnings | ✅ Complete | Strict lint configuration |

### Stability Requirements

| Requirement | Status | Notes |
|-------------|--------|-------|
| API stability commitment | ✅ Complete | [docs/api-stability.md](docs/api-stability.md) |
| MSRV policy | ✅ Complete | Rust 1.85+ (Edition 2024) |
| Semver compliance | ✅ Complete | Following cargo guidelines |
| Migration guide from 0.x | ✅ Complete | [docs/migration-to-1.0.md](docs/migration-to-1.0.md) |

### Quality Requirements

| Requirement | Status | Notes |
|-------------|--------|-------|
| No security vulnerabilities | ✅ Complete | Regular `cargo audit` |
| Performance benchmarks | ✅ Complete | Criterion benchmarks vs rmcp |
| Memory safety | ✅ Complete | `#![deny(unsafe_code)]` |
| Error handling consistency | ✅ Complete | Unified McpError type |

## Post-1.0 Roadmap

### 1.1 - Enhanced Extensions

- Official MCP Apps extension (SEP-1865) implementation
- Domain-specific extension templates (healthcare, finance)
- Extension discovery mechanism

### 1.2 - Performance & Scalability

- Connection pooling improvements
- Message batching optimization
- Streaming response improvements

### 1.3 - Developer Experience

- `mcpkit` CLI tool for scaffolding
- Integration test harness improvements
- Debug/trace tooling

### Future Considerations

- WebTransport support (when spec stabilizes)
- QUIC transport exploration
- Multi-tenant server patterns
- Cluster/distributed server support

## Version History

| Version | Date | Highlights |
|---------|------|------------|
| 0.3.0 | Dec 2024 | MCP 2025-11-25, Tasks, Elicitation, OAuth |
| 0.2.0 | Nov 2024 | MCP 2025-06-18, Structured output |
| 0.1.0 | Oct 2024 | Initial release, MCP 2024-11-05 |

## Contributing to 1.0

We welcome contributions toward the 1.0 release. Priority areas:

1. **Documentation improvements** - Tutorials, examples, API docs
2. **Test coverage** - Edge cases, error conditions
3. **Real-world usage feedback** - API ergonomics, pain points
4. **Performance profiling** - Identify bottlenecks

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Timeline

No specific timeline is set for 1.0. Release will occur when all criteria above are met and the API has stabilized through community usage. We follow a "release when ready" philosophy rather than time-based releases.

---

*Last updated: December 2024*
