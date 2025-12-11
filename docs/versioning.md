# Versioning and Stability

This document describes the versioning policy, deprecation process, and stability guarantees for the Rust MCP SDK.

## Versioning Policy

The Rust MCP SDK follows [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** (x.0.0): Breaking changes to public API
- **MINOR** (0.x.0): New features, backward-compatible additions
- **PATCH** (0.0.x): Bug fixes, security patches, documentation

### Pre-1.0 Stability

During the 0.x.y phase, the API is considered unstable:

- Minor version bumps (0.x.0) may include breaking changes
- Patch versions (0.0.x) remain backward compatible
- We aim to minimize churn, but cannot guarantee API stability

**Current Status:** 0.1.x (Alpha)

### Crate Versioning

All crates in the workspace are versioned together:

| Crate | Version |
|-------|---------|
| `mcpkit` | 0.1.x |
| `mcpkit-core` | 0.1.x |
| `mcpkit-server` | 0.1.x |
| `mcpkit-client` | 0.1.x |
| `mcpkit-transport` | 0.1.x |
| `mcpkit-macros` | 0.1.x |
| `mcpkit-testing` | 0.1.x |

This simplifies dependency management and ensures compatibility.

## Stability Tiers

### Tier 1: Stable (after 1.0)

These APIs are covered by semver guarantees:

- `#[tool]`, `#[resource]`, `#[prompt]` macro syntax
- `McpError` error type and variants
- `ToolOutput` return types
- `ServerBuilder` and `ClientBuilder` APIs
- Transport trait implementations
- JSON-RPC wire format

### Tier 2: Unstable (marked with `#[doc(hidden)]` or feature flags)

These may change without major version bumps:

- Internal implementation details
- Feature-gated experimental features
- Items marked `#[doc(hidden)]`
- Private modules

### Tier 3: Internal

No stability guarantees:

- Private functions and types
- Test utilities
- Build scripts

## Deprecation Process

### Standard Deprecation (Major Features)

When deprecating a major feature or API:

1. **Announce**: Mention in release notes with migration path
2. **Mark**: Add `#[deprecated]` attribute with version and reason
3. **Warn**: Emit compiler warnings for one minor release
4. **Remove**: Remove in the next major release

```rust
#[deprecated(since = "0.3.0", note = "Use `ServerBuilder::new()` instead")]
pub fn create_server() -> Server { ... }
```

### Rapid Deprecation (Security/Critical)

For security issues or critical bugs:

1. **Immediate fix**: Release patch with secure alternative
2. **Deprecate**: Mark old API deprecated immediately
3. **Document**: Security advisory with migration guide
4. **Remove**: May remove in next minor version with notice

### Deprecation Timeline

| Phase | Pre-1.0 | Post-1.0 |
|-------|---------|----------|
| Warning period | 1 minor release | 2 minor releases |
| Removal | Next minor | Next major |
| Security issues | Immediate | Immediate |

## Migration Guides

We provide migration guides for:

- All breaking changes in minor releases (pre-1.0)
- All major version upgrades (post-1.0)
- Protocol version updates

See [migration-from-rmcp.md](./migration-from-rmcp.md) for migrating from rmcp.

## Road to 1.0

### 1.0 Requirements

Before declaring 1.0 stable, we will:

1. **Production validation**: Real-world usage by multiple adopters
2. **API stabilization**: No breaking changes for 3+ minor releases
3. **Protocol compliance**: Full MCP specification support
4. **Documentation**: Complete API docs and guides
5. **Test coverage**: >80% code coverage, comprehensive integration tests
6. **Performance**: Benchmarked and optimized for production use
7. **Security audit**: Independent security review

### Estimated Timeline

| Milestone | Target | Status |
|-----------|--------|--------|
| 0.1.0 Alpha | Q4 2024 | In progress |
| 0.2.0 Beta | Q1 2025 | Planned |
| 0.3.0 RC | Q2 2025 | Planned |
| 1.0.0 Stable | Q3 2025 | Planned |

Note: Dates are estimates and subject to change based on feedback and testing.

## Supported Rust Versions

### Minimum Supported Rust Version (MSRV)

The current MSRV is **Rust 1.75**.

MSRV policy:
- MSRV increases are treated as minor version bumps
- We support the 4 most recent stable Rust versions
- MSRV is tested in CI

### Updating MSRV

When updating MSRV:
1. Document in CHANGELOG.md
2. Update `rust-version` in Cargo.toml
3. Update CI configuration
4. Announce in release notes

## Protocol Version Compatibility

### Supported Protocol Versions

| SDK Version | MCP Versions |
|-------------|--------------|
| 0.1.x | 2024-11-05, 2025-11-25 |

### Protocol Updates

When a new MCP protocol version is released:

1. Add support in the next minor release
2. Maintain backward compatibility with previous versions
3. Document any capability differences
4. Provide migration guides for breaking changes

See [protocol-versions.md](./protocol-versions.md) for protocol compatibility details.

## Feature Flags

### Stable Features (default)

These are enabled by default and covered by stability guarantees:

```toml
[features]
default = ["server", "client", "tokio-runtime"]
```

### Experimental Features

These may change without notice:

```toml
[features]
experimental = []  # Experimental features
```

Enable experimental features at your own risk:

```toml
mcpkit = { version = "0.1", features = ["experimental"] }
```

### Optional Transports

Feature-gated but stable:

```toml
[features]
websocket = ["tokio-tungstenite"]
http = ["reqwest", "axum"]
unix = []  # Unix-only
```

## Reporting Issues

If you encounter breaking changes or deprecation issues:

1. Check the [CHANGELOG.md](../CHANGELOG.md) for migration notes
2. Search existing [GitHub issues](https://github.com/anthropics/mcpkit/issues)
3. Open a new issue with reproduction steps

## Summary

| Aspect | Pre-1.0 | Post-1.0 |
|--------|---------|----------|
| Breaking changes | Minor versions | Major versions only |
| Deprecation warning | 1 minor release | 2 minor releases |
| MSRV increases | Minor version | Minor version |
| Protocol updates | Minor version | Minor version |
| Security fixes | Patch version | Patch version |
