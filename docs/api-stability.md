# API Stability Commitment

This document defines the API stability commitment for mcpkit. After 1.0, these guarantees are binding and enforced through semantic versioning.

## Stability Tiers

All public APIs fall into one of three stability tiers:

### Tier 1: Stable

These APIs are covered by strict semver guarantees. Breaking changes require a major version bump (1.0 → 2.0).

| Category | Covered Items |
|----------|---------------|
| **Macros** | `#[mcp_server]`, `#[tool]`, `#[resource]`, `#[prompt]`, `#[derive(ToolInput)]` |
| **Core Types** | `McpError`, `ToolOutput`, `ResourceContents`, `PromptMessage`, `Content` |
| **Builder APIs** | `ServerBuilder`, `ClientBuilder`, `ToolBuilder`, `ResourceBuilder` |
| **Handler Traits** | `ServerHandler`, `ToolHandler`, `ResourceHandler`, `PromptHandler`, `TaskHandler`, `SamplingHandler`, `ElicitationHandler` |
| **Transport Trait** | `Transport`, `TransportListener`, `TransportMetadata` |
| **Capability Types** | `ServerCapabilities`, `ClientCapabilities`, `ServerInfo`, `ClientInfo` |
| **Protocol Types** | `Request`, `Response`, `Notification`, `Message`, `RequestId` |
| **State Types** | `Connection<S>`, `Disconnected`, `Initializing`, `Ready`, `Closing`, `Connected` |
| **Context Types** | `Context`, `ContextData`, `Peer`, `CancellationToken` |
| **Extension Types** | `Extension`, `ExtensionRegistry` |
| **Wire Format** | JSON-RPC 2.0 request/response structure |

### Tier 2: Unstable

These APIs may change in minor versions (1.1, 1.2, etc.):

| Category | Items |
|----------|-------|
| **Hidden Items** | Anything marked `#[doc(hidden)]` |
| **Feature-Gated** | APIs behind `experimental` feature flag |
| **Internal Modules** | Private module internals |
| **Metrics Types** | `ServerMetrics`, `MetricsSnapshot`, `MethodStats` |

### Tier 3: Internal

No stability guarantees (may change in any release):

| Category | Items |
|----------|-------|
| **Private Items** | Non-`pub` functions, types, and fields |
| **Test Utilities** | `mcpkit-testing` crate contents |
| **Build Internals** | Build scripts, CI configuration |
| **Benchmarks** | Benchmark implementation details |

## What Constitutes a Breaking Change

Based on [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) and [Cargo SemVer](https://doc.rust-lang.org/cargo/reference/semver.html):

### Major Changes (require major version bump)

**Type Changes:**
- Removing or renaming public types, functions, methods, or fields
- Changing function signatures (parameters, return types)
- Changing trait definitions (adding non-defaulted methods)
- Removing trait implementations
- Changing type layout for `#[repr(C)]` types

**Behavior Changes:**
- Changing semantics of existing APIs in incompatible ways
- Removing feature flags
- Making previously-infallible operations fallible

**Macro Changes:**
- Changing required macro attribute syntax
- Removing supported attribute options
- Changing generated code in ways that break existing callers

### Minor Changes (require minor version bump)

**Additions:**
- Adding new public types, functions, methods, or modules
- Adding new feature flags
- Adding new defaulted trait methods
- Adding new optional parameters with defaults
- Adding new enum variants to `#[non_exhaustive]` enums

**Improvements:**
- Performance optimizations
- Relaxing type bounds
- Adding trait implementations (with rare exceptions)

**Policy Changes:**
- Increasing MSRV (Minimum Supported Rust Version)
- Adding new protocol version support

### Patch Changes (require patch version bump)

- Bug fixes that don't change API semantics
- Security patches
- Documentation updates
- Internal refactoring (no public API changes)

## MSRV Policy

Minimum Supported Rust Version policy:

- **Current MSRV**: Rust 1.85 (Edition 2024)
- **MSRV increases**: Treated as minor version changes
- **Support window**: 4 most recent stable Rust versions
- **CI verification**: MSRV is tested on every commit

## Maintenance Commitment

Following the [Tokio model](https://tokio.rs/blog/2020-10-tokio-0-3), we commit to:

- **Minimum 3 years of maintenance** for each major version after 1.0
- **Minimum 2 years before a hypothetical 2.0** release
- **Security patches** for the previous major version for 1 year after a new major release

This provides ecosystem stability and allows users to plan upgrades.

## Platform Support

### Tier 1 Platforms (Guaranteed Support)

These platforms are tested in CI and fully supported:

| Platform | Architecture | Notes |
|----------|--------------|-------|
| Linux | x86_64, aarch64 | glibc 2.17+ |
| macOS | x86_64, aarch64 | macOS 11+ |
| Windows | x86_64 | Windows 10+ |

### Tier 2 Platforms (Best Effort)

These platforms should work but are not tested in CI:

| Platform | Architecture | Notes |
|----------|--------------|-------|
| Linux | armv7, i686 | Community tested |
| FreeBSD | x86_64 | Community tested |
| Android | aarch64 | API level 28+ |
| iOS | aarch64 | Requires custom transport |

### Tier 3 Platforms (Community)

- WebAssembly (WASI): Experimental, stdio transport only
- Other Unix-likes: May work, untested

Platform requirements may change in minor versions (e.g., minimum glibc version).

## Protocol Version Compatibility

MCP protocol version handling:

| Scenario | Version Impact |
|----------|----------------|
| Adding support for new protocol version | Minor |
| Changing default protocol version | Minor |
| Removing support for old protocol version | Major |
| Changing wire format | Major |

Currently supported protocol versions:
- `2024-11-05` (original)
- `2025-03-26` (OAuth, annotations)
- `2025-06-18` (elicitation, structured output)
- `2025-11-25` (tasks, parallel tools) - **default**

## Deprecation Policy

### Standard Deprecation

1. **Announce**: Document in release notes with migration path
2. **Mark**: Add `#[deprecated(since = "x.y.z", note = "...")]`
3. **Warning Period**: 2 minor releases post-1.0 (1 minor release pre-1.0)
4. **Remove**: Next major version

### Example

```rust
#[deprecated(
    since = "1.2.0",
    note = "Use `ServerBuilder::new()` instead. Will be removed in 2.0."
)]
pub fn create_server() -> Server { ... }
```

### Security Deprecation

For security issues, the deprecation timeline is accelerated:

1. **Immediate patch**: Release secure alternative
2. **Deprecate**: Mark old API deprecated immediately
3. **Security advisory**: Document CVE and migration
4. **Remove**: May remove in next minor with sufficient notice

## Enforcement

API stability is enforced through:

### Automated Tooling

```bash
# Run before every release
cargo semver-checks

# Included in CI pipeline
cargo deny check
cargo audit
```

### CI Checks

- `cargo-semver-checks` runs on every PR to detect breaking changes
- Breaking changes on non-major bumps will fail CI
- Documentation coverage is enforced

### Release Checklist

Before any release:

1. [ ] `cargo semver-checks` passes
2. [ ] CHANGELOG documents all changes
3. [ ] Version bump matches change type
4. [ ] Deprecation warnings added for removed APIs
5. [ ] Migration guide updated if needed

## Exceptions

Certain categories are explicitly excluded from stability guarantees:

### Soundness Fixes

If an API allows undefined behavior or memory unsafety, we may fix it in a patch release even if it technically breaks code depending on the unsound behavior.

### Specification Compliance

If an API violates the MCP specification, we may fix it to match the spec in a minor release with documentation.

### Security Vulnerabilities

Security fixes take precedence over API stability. We follow responsible disclosure practices.

## Comparison with Other Crates

| Crate | Stability Approach | Notes |
|-------|-------------------|-------|
| **mcpkit** | Tiered (Stable/Unstable/Internal) | Full semver post-1.0 |
| **rmcp** | Pre-1.0, limited guarantees | Official but unstable |
| **tokio** | Tiered with LTS releases | Battle-tested model |
| **serde** | Very stable, rare breaking changes | Gold standard |

## Practical Examples

### Safe Updates

These updates are safe and won't break your code:

```toml
# Any 1.x.y version is compatible with 1.0.0
mcpkit = "1"

# Patch updates are always safe
mcpkit = "1.0"
```

### Breaking Update Example

If we release 2.0.0 with breaking changes:

```toml
# This pins to 1.x and won't get 2.0 changes
mcpkit = "1"

# To upgrade, explicitly opt in
mcpkit = "2"
```

### Using Unstable Features

```toml
# Opt in to unstable features (may break in minor versions)
mcpkit = { version = "1", features = ["experimental"] }
```

## Commitment

This stability commitment is effective starting with version 1.0.0. Pre-1.0 releases (0.x.y) follow relaxed semver where minor versions may include breaking changes.

**We take API stability seriously.** Breaking changes are expensive for users and we will avoid them whenever possible. When breaking changes are necessary, we will:

1. Provide clear migration documentation
2. Offer deprecation periods
3. Minimize the scope of changes
4. Consider compatibility shims where practical

---

*Last updated: December 2024*

## References

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Cargo SemVer Compatibility](https://doc.rust-lang.org/cargo/reference/semver.html)
- [RFC 1105: API Evolution](https://rust-lang.github.io/rfcs/1105-api-evolution.html)
- [Effective Rust: SemVer](https://effective-rust.com/semver.html)
