# mcpkit Roadmap to 1.0

This document outlines the path to a stable 1.0 release of mcpkit.

## Current Status: v0.7.0 (unreleased)

mcpkit implements every method the MCP 2025-11-25 schema defines, across stdio,
WebSocket, Unix, in-memory and the four HTTP framework adapters.

All 1.0 criteria below are met. Method coverage, structural conformance and the
behavioural checks a schema diff cannot make are each verified in CI against the
vendored spec — see the tables for what is checked and how.

## 1.0 Release Criteria

### Core Requirements (Must Have)

| Requirement | Status | Notes |
|-------------|--------|-------|
| Full MCP 2025-11-25 compliance | ✅ Complete | All 31 schema-defined methods, diffed against the vendored schema in CI. Structural conformance covers every `$def` with a same-named type — 75 of 145, the rest being envelopes, unions and aliases with no 1:1 Rust type — with no outstanding field gaps |
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
| Test coverage | ✅ Complete | 1,446 tests incl. spec-anchored and behavioural conformance suites |
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
| No security vulnerabilities | ✅ Complete | `cargo deny check advisories` clean. RUSTSEC-2026-0185 (quinn-proto) resolved by upgrading to 0.11.16; RUSTSEC-2023-0071 is deliberately ignored with rationale in `deny.toml` |
| Performance benchmarks | ✅ Complete | Criterion benchmarks vs rmcp 2.2 |
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

### Next protocol revision

A revision after `2025-11-25` is in flight upstream. As of 2026-07-26 it has **no
published schema** — `modelcontextprotocol/modelcontextprotocol` carries the four
released revisions plus `draft` — so mcpkit supporting all four published
versions is current, not behind.

Two things to watch:

- `rmcp` 2.2.0 already accepts a `2026-07-28` protocol version and gates
  behaviour on it (SEP-2164: `INVALID_PARAMS` for peers negotiating it or newer).
- The upstream `draft` schema is a substantial restructure, not an increment —
  21 methods against 2025-11-25's 31, adding `server/discover`,
  `subscriptions/listen` and `notifications/subscriptions/acknowledged`.

Decision needed: track the next revision early behind a feature, as rmcp is
doing, or wait for publication. `spec/` and the `schema-check` CI job are already
structured to vendor a second revision alongside the current one.

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
