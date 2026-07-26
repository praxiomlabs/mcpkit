# mcpkit Roadmap to 1.0

This document outlines the path to a stable 1.0 release of mcpkit.

## Current Status: v0.7.0 (unreleased)

mcpkit implements every method the MCP 2025-11-25 schema defines, across stdio,
WebSocket, Unix, in-memory and the four HTTP framework adapters.

Most 1.0 criteria below are met. Two are not yet, and are tracked as blockers
rather than marked complete: an outstanding advisory in a transitive dependency,
and structural spec conformance that has been sampled rather than verified
exhaustively. See the tables for which is which.

## 1.0 Release Criteria

### Core Requirements (Must Have)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Full MCP 2025-11-25 compliance | ⚠️ Partly verified | All 31 schema-defined methods implemented and diffed against the vendored schema in CI. Structural (field-level) conformance is sampled, not exhaustive — 7 of 145 `$defs` checked |
| Protocol version negotiation | ✅ Complete | All 4 published versions; table-driven conformance tests |
| OAuth 2.1 + PKCE support | ✅ Complete | RFC 9728, 8414, 7636 compliant |
| Tasks (async operations) | ✅ Complete | Full task lifecycle support |
| Elicitation | ✅ Complete | Server-initiated user input |
| Tool/Resource/Prompt handlers | ✅ Complete | Full MCP primitives |
| Multiple transport support | ✅ Complete | stdio, HTTP/SSE, WebSocket, Unix, gRPC |
| Client SDK | ✅ Complete | Connection management, retries |
| Server SDK | ✅ Complete | Handler traits, routing |
| Axum integration | ✅ Complete | Router, SSE, OAuth discovery |
| Actix-web integration | ✅ Complete | Router, SSE, OAuth discovery |
| Rocket integration | ✅ Complete | Router, SSE, session management |
| Warp integration | ✅ Complete | Router, SSE, CORS support |
| Extension infrastructure | ✅ Complete | Structured extension support |
| Comprehensive documentation | ✅ Complete | 34 doc files, 6 ADRs |
| Test coverage | ✅ Complete | 1,437 tests incl. spec-anchored conformance suites |
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
| No security vulnerabilities | ❌ Blocked | RUSTSEC-2026-0185 (quinn-proto, 7.5 high) is open and unsuppressed; fix is an upgrade to >=0.11.15. RUSTSEC-2023-0071 is deliberately ignored with rationale in `deny.toml` |
| Performance benchmarks | ✅ Complete | Criterion benchmarks vs rmcp (dev-dep pinned at 1.7; rmcp is now 2.x) |
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
| 0.6.0 | 2026-06-18 | Concurrency & panic isolation, JWT/OAuth & transport hardening, macro fixes (#5–#24) |
| 0.5.0 | 2025-12-25 | gRPC transport, Rocket/Warp integrations, deployment configs |
| 0.4.0 | 2025-12-24 | `#[mcp_client]` macro, protocol extensions, debug tooling |
| 0.3.0 | 2025-12-23 | Zero-copy messages, filesystem server, stress testing |
| 0.2.0 | 2025-12-12 | Client APIs, Tasks, server metrics |
| 0.1.0 | 2025-12-11 | Initial release, MCP 2025-11-25 protocol |

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

*Last updated: 2026-07-26*
