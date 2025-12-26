# Migration Guide: 0.x to 1.0

This guide helps you migrate from mcpkit 0.x to the stable 1.0 release.

## TL;DR

**If you're on 0.5.x**: No migration required. The 1.0 API is identical to 0.5.x.

**If you're on 0.4.x or earlier**: Follow the version-specific guides below.

## What Changes in 1.0

### Stability Guarantees

After 1.0, mcpkit follows strict semantic versioning. See [API Stability](api-stability.md) for the complete policy.

| Change Type | Version Bump |
|-------------|--------------|
| Breaking API changes | Major (2.0, 3.0, etc.) |
| New features (backward compatible) | Minor (1.1, 1.2, etc.) |
| Bug fixes | Patch (1.0.1, 1.0.2, etc.) |

### API Stability Tiers

**Tier 1 - Stable** (covered by semver):
- `#[mcp_server]`, `#[tool]`, `#[resource]`, `#[prompt]` macro syntax
- `McpError` error type and variants
- `ToolOutput`, `ResourceContents`, `PromptMessage` types
- `ServerBuilder` and `ClientBuilder` APIs
- Transport trait implementations
- JSON-RPC wire format

**Tier 2 - Unstable** (may change in minor versions):
- Items marked `#[doc(hidden)]`
- Feature-gated experimental features
- Internal implementation details

See [API Stability](api-stability.md) for the complete tier definitions.

## Migration by Version

### From 0.5.x to 1.0

**No changes required.** The 0.5.x API is the 1.0 API.

Simply update your dependency:

```toml
# Before
mcpkit = "0.5"

# After
mcpkit = "1"
```

### From 0.2.x to 1.0

#### Step 1: Update Dependency

```toml
mcpkit = "1"
```

#### Step 2: Update MCP Protocol Handling (if applicable)

The default protocol version changed from `2025-06-18` to `2025-11-25`:

```rust
// If you need the old default, explicitly specify it:
use mcpkit::capability::PROTOCOL_VERSION;

// But generally, no changes needed - the SDK auto-negotiates
```

#### Step 3: Update Runtime Feature (if using async-std)

async-std has been replaced with smol:

```toml
# Before
mcpkit = { version = "0.2", features = ["async-std-runtime"] }

# After (the alias still works, but prefer explicit smol)
mcpkit = { version = "1", features = ["smol-runtime"] }
```

#### Step 4: Optional - Enable New Features

New capabilities available in 1.0:

```rust
use mcpkit::prelude::*;

// Tasks for long-running operations
let caps = ServerCapabilities::new()
    .with_tools()
    .with_tasks();  // New in 0.3/1.0

// Elicitation for structured user input
let caps = ServerCapabilities::new()
    .with_elicitation();  // New in 0.3/1.0

// Extensions for custom protocols
use mcpkit::extension::{Extension, ExtensionRegistry};
let registry = ExtensionRegistry::new()
    .register(Extension::new("com.example.myext").with_version("1.0.0"));
let caps = ServerCapabilities::new()
    .with_extensions(registry);  // New in 0.3/1.0
```

### From 0.1.x to 1.0

#### Step 1: Update Dependency

```toml
mcpkit = "1"
```

#### Step 2: Update MSRV

mcpkit 1.0 requires Rust 1.85+. Update your `rust-version` in `Cargo.toml`:

```toml
rust-version = "1.85"
```

#### Step 3: Update ServerCapabilities Pattern

The capability builder pattern changed:

```rust
// Before (0.1.x)
let caps = ServerCapabilities {
    tools: Some(ToolsCapability::default()),
    resources: Some(ResourcesCapability::default()),
    ..Default::default()
};

// After (1.0)
let caps = ServerCapabilities::new()
    .with_tools()
    .with_resources();
```

#### Step 4: Update Custom Transports (if applicable)

If you implemented a custom transport, the trait was simplified:

```rust
// Before (0.1.x)
impl Transport for MyTransport {
    type Error = MyError;
    async fn send(&mut self, msg: Message) -> Result<(), Self::Error>;
    async fn recv(&mut self) -> Result<Option<Message>, Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
}

// After (1.0)
impl Transport for MyTransport {
    async fn send(&self, msg: Message) -> Result<(), TransportError>;
    async fn recv(&self) -> Result<Option<Message>, TransportError>;
    async fn close(&self) -> Result<(), TransportError>;
}
```

Key changes:
- Associated `Error` type removed; use `TransportError`
- Methods take `&self` instead of `&mut self`

## Pre-1.0 Breaking Changes Summary

| Version Transition | Change | Migration |
|--------------------|--------|-----------|
| 0.2.x → 0.3.x | MCP protocol default: 2025-06-18 → 2025-11-25 | No action needed |
| 0.2.x → 0.3.x | Tasks API added | Optional: use `with_tasks()` |
| 0.2.x → 0.3.x | Elicitation API added | Optional: use `with_elicitation()` |
| 0.2.x → 0.3.x | OAuth 2.1 types added | Optional: use `with_oauth()` in HTTP routers |
| 0.2.x → 0.3.x | Extensions infrastructure | Optional: use `with_extensions()` |
| 0.2.x → 0.3.x | async-std replaced with smol | Feature alias preserved |
| 0.1.x → 0.2.x | Protocol version negotiation added | No action needed |
| 0.1.x → 0.2.x | Capability builder pattern changed | Update `ServerCapabilities` calls |
| 0.1.x → 0.2.x | Transport trait simplified | Update custom transports |

## Migration Checklist

### Before Upgrading

- [ ] Read this migration guide completely
- [ ] Check current version: `cargo tree -p mcpkit`
- [ ] Review [CHANGELOG](../CHANGELOG.md) for your version range
- [ ] Backup your project or ensure git is clean

### Upgrade Process

- [ ] Update `Cargo.toml`: `mcpkit = "1"`
- [ ] Run `cargo build` and fix any compile errors
- [ ] Address deprecation warnings: `cargo build 2>&1 | grep -i deprecated`
- [ ] Run test suite: `cargo test`
- [ ] Test manually against MCP clients

### After Upgrading

- [ ] Verify protocol negotiation works with clients
- [ ] Check server logs for warnings
- [ ] Consider enabling new features (tasks, elicitation, extensions)
- [ ] Update your documentation if needed

## Verifying Protocol Compatibility

After migration, verify your server negotiates protocols correctly:

```rust
use mcpkit::capability::SUPPORTED_PROTOCOL_VERSIONS;

// mcpkit 1.0 supports:
for version in SUPPORTED_PROTOCOL_VERSIONS {
    println!("Supported: {}", version);
}
// Output:
// Supported: 2024-11-05
// Supported: 2025-03-26
// Supported: 2025-06-18
// Supported: 2025-11-25
```

Test with clients that use different protocol versions to ensure negotiation works.

## Rollback Plan

If migration causes critical issues, you can roll back to your previous version.

### Rolling Back to 0.5.x

```toml
# Revert Cargo.toml
mcpkit = "0.5"
```

```bash
# Clean build artifacts and rebuild
cargo clean
cargo build
```

### Rolling Back to 0.2.x or Earlier

```toml
# Revert Cargo.toml
mcpkit = "0.2"

# If you modified code for 1.0, you may need to revert those changes
# Check git diff for what changed
```

### Rollback Checklist

1. [ ] Revert `Cargo.toml` dependency version
2. [ ] Revert any code changes made for migration
3. [ ] Run `cargo clean` to remove cached artifacts
4. [ ] Run `cargo build` and verify compilation
5. [ ] Run test suite: `cargo test`
6. [ ] Verify against MCP clients

### When to Rollback

Consider rolling back if:
- Critical functionality is broken
- Performance degradation is unacceptable
- Incompatibility with required MCP clients
- Cannot resolve migration issues within acceptable timeframe

**Note:** Report rollback-triggering issues to [GitHub Issues](https://github.com/praxiomlabs/mcpkit/issues) so we can improve the migration experience.

## Common Migration Issues

### Issue: `async-std` feature not found

**Symptom:**
```
error: feature `async-std-runtime` not found
```

**Solution:**
The feature alias is preserved, so this shouldn't happen. If it does, switch to `smol-runtime`:

```toml
mcpkit = { version = "1", features = ["smol-runtime"] }
```

### Issue: Transport trait changes

**Symptom:**
```
error[E0046]: not all trait items implemented, missing: `send`, `recv`
```

**Solution:**
Update your custom transport to use `&self` and `TransportError`. See the "Update Custom Transports" section above.

### Issue: Capability struct fields

**Symptom:**
```
error: no field `tools` on type `ServerCapabilities`
```

**Solution:**
Use the builder pattern instead of struct literals:

```rust
// Use this:
ServerCapabilities::new().with_tools()

// Not this:
ServerCapabilities { tools: Some(...), .. }
```

## Getting Help

If you encounter issues during migration:

1. Check the [Troubleshooting Guide](troubleshooting.md)
2. Search [GitHub Issues](https://github.com/praxiomlabs/mcpkit/issues)
3. Open a new issue with:
   - Your previous mcpkit version
   - Your target mcpkit version
   - Complete error messages
   - Minimal reproduction code

## Version Compatibility Matrix

| mcpkit | MCP Protocol | Rust | Status |
|--------|--------------|------|--------|
| 1.0.x  | 2024-11-05 → 2025-11-25 | 1.85+ | Stable |
| 0.5.x  | 2024-11-05 → 2025-11-25 | 1.85+ | Current RC |
| 0.4.x  | 2024-11-05 → 2025-11-25 | 1.85+ | Maintenance |
| 0.3.x  | 2024-11-05 → 2025-11-25 | 1.85+ | Security Only |
| 0.2.x  | 2024-11-05 → 2025-06-18 | 1.82+ | End of Life |
| 0.1.x  | 2024-11-05 | 1.80+ | End of Life |

---

*Last updated: December 2025*
