# Releasing MCPkit

Comprehensive guide for releasing new versions of MCPkit to crates.io.

**Version:** 0.5.0 | **MSRV:** 1.85 | **Edition:** 2024

---

## Table of Contents

1. [Cardinal Rules](#cardinal-rules)
2. [Pre-flight Checks](#0-pre-flight-checks)
3. [Codebase Hygiene & Safety](#1-codebase-hygiene--safety)
4. [Version Consistency](#2-version-consistency-the-blast-radius)
5. [Environment & Infrastructure Alignment](#3-environment--infrastructure-alignment)
6. [Dependency & Security Compliance](#4-dependency--security-compliance)
7. [Documentation Integrity](#5-documentation-integrity)
8. [Final Build Verification](#6-final-build-verification)
9. [API Compatibility & Semver](#7-api-compatibility--semver)
10. [Publishing Preparation](#8-publishing-preparation-rustcratesio-specific)
11. [Git & Release Protocol](#9-git--release-protocol)
12. [Post-Release Verification](#10-post-release-verification)
13. [Manual Recovery Procedures](#manual-recovery-procedures)
14. [Crate Dependency Graph](#crate-dependency-graph)
15. [Feature-Specific Testing](#feature-specific-testing)
16. [Platform-Specific Notes](#platform-specific-notes)
17. [Security Incident Response](#security-incident-response)
18. [Lessons Learned](#lessons-learned)
19. [CI Automation Coverage](#ci-automation-coverage)
20. [Justfile Recipe Mapping](#justfile-recipe-mapping)

---

## Cardinal Rules

> **These rules are non-negotiable. Violating them will cause release failures.**

### 1. Never Tag Before CI Passes

```bash
# WRONG: Tag immediately after committing
git commit -m "chore: bump to 0.5.0" && git tag v0.5.0  # ❌

# RIGHT: Wait for CI, then tag
git push origin main
gh run watch  # Wait for green
just tag      # Verifies CI status automatically
```

### 2. Never Force Push Tags

Once a tag is pushed, it cannot be safely changed. If you need to fix something:
- Delete the broken tag remotely: `git push --delete origin v0.X.0`
- Delete locally: `git tag -d v0.X.0`
- Fix the issue, bump to the next patch version, and re-tag

### 3. Publishing is Irreversible

`cargo publish` cannot be undone. A yanked crate still counts as "published":
- The version number is permanently consumed
- You must bump to a new version to publish fixes
- Always run `just publish-dry` before the real publish

### 4. Local Must Match CI

If it passes locally but fails in CI, your environment is misconfigured:
- Run `just deny` locally before pushing (matches CI `cargo-deny-action@v2`)
- Use `just ci` to mirror the full CI pipeline
- Never assume "it works on my machine" is sufficient

---

## 0. Pre-flight Checks

Quick verification before detailed review:

```bash
# Option A: Use just recipe (recommended)
just ci

# Option B: Manual commands
git status  # Should show no uncommitted changes (or only expected ones)
gh run list --limit 5  # Or check GitHub Actions UI
cargo check --all-features --all-targets
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
```

- [ ] Git working directory is clean (or changes are intentional)
- [ ] CI is passing on the target branch
- [ ] Local build/test/lint all pass (`just ci`)

### ⚠️ Critical: CI Parity

**Local checks MUST match CI exactly.** If CI fails after a push, your local tooling is misconfigured.

The following commands are designed to mirror CI exactly:

| Local Command | CI Job | What It Checks |
|---------------|--------|----------------|
| `just deny` | `cargo-deny-action@v2 check all` | Advisories, licenses, bans |
| `just clippy` | `cargo clippy --all-features --all-targets -- -D warnings` | Linting |
| `just test-locked` | `cargo test --all-features --locked` | Tests with locked deps |

**Before pushing any changes, run:**
```bash
just deny        # MUST pass - matches CI cargo-deny exactly
just release-check   # Full validation
```

If `just deny` fails locally, it WILL fail in CI. Fix it before pushing.

---

## 1. Codebase Hygiene & Safety

### Work-in-Progress Markers

```bash
# Use just recipe (recommended)
just wip-check

# Manual commands
grep -rn "TODO\|FIXME\|XXX\|HACK" --include="*.rs" crates/
grep -rn "todo!\|unimplemented!" --include="*.rs" crates/
```
- [ ] Run `just wip-check` or grep for `TODO`, `FIXME`, `XXX`, `HACK` comments
- [ ] Verify no `todo!()`, `unimplemented!()` macros in production code
- [ ] Ensure no incomplete logic ships to production

### Panic Path Audit

```bash
# Use just recipe (recommended)
just panic-audit

# Manual commands
grep -rn "\.unwrap()" crates/*/src/ --include="*.rs"
grep -rn "\.expect(" crates/*/src/ --include="*.rs"
```

- [ ] Run `just panic-audit` to audit `.unwrap()` and `.expect()` calls
- [ ] **Note:** High line numbers often indicate test modules - verify context
- [ ] **Remediation examples discovered:**
  - `partial_cmp().unwrap()` on floats → use `total_cmp()` (NaN-safe)
  - `checked_sub().unwrap()` → use `is_none_or()` pattern for graceful handling
- [ ] Verify all production panic paths have documented justification

### Dead Code Analysis
- [ ] Review `#[allow(dead_code)]` suppressions
- [ ] Ensure suppressions are either documented (reserved for future use) or removed
- [ ] Check for unused imports and dependencies

### Strict Linting

```bash
just clippy          # Standard linting
just clippy-strict   # Pedantic linting
```

- [ ] Run `just clippy` (warnings-as-errors)
- [ ] Address non-idiomatic patterns (e.g., `map_or(true, ...)` → `is_none_or(...)`)
- [ ] Verify all feature flag combinations pass linting

---

## 2. Version Consistency (The "Blast Radius")

```bash
just version-sync    # Verify README + getting-started match Cargo.toml
```

### Core Manifests
- [ ] Bump version in `Cargo.toml` / `package.json` / equivalent
- [ ] Verify workspace members share consistent versioning (if applicable)

### Documentation Version References
Run `just version-sync` then grep for old version strings across `docs/` directory:
- [ ] Installation instructions (e.g., `dependency = "0.2"`)
- [ ] Migration guides
- [ ] ADRs mentioning specific versions
- [ ] Runtime/transport configuration examples
- [ ] Getting started guides

### Crate/Package Name Consistency
- [ ] Verify all documentation references correct package names
- [ ] Check ASCII diagrams and architecture docs for old names
- [ ] Update code examples in documentation

### Example Projects
- [ ] Ensure `examples/` directories reference current library version
- [ ] Verify example Cargo.toml dependencies are up to date
- [ ] Note: Internal example package versions (0.1.0) are acceptable if unpublished

---

## 3. Environment & Infrastructure Alignment

### Minimum Supported Version (MSRV) Sync

```bash
just msrv-check    # Verify code compiles with declared MSRV
```

Ensure MSRV is consistent across **all** locations:
- [ ] CI configuration (workflow files)
- [ ] `CONTRIBUTING.md` and prerequisites docs
- [ ] Dockerfiles (base image tags, e.g., `FROM rust:1.85`)
- [ ] `Cargo.toml` rust-version field
- [ ] Issue templates (example version placeholders)

### CI Configuration Validity
- [ ] Verify CI tool paths match current project structure
- [ ] Check `codecov.yml` / coverage config points to valid directories
- [ ] Ensure all crates/packages are included in coverage reporting
- [ ] Add missing CI jobs (e.g., code coverage upload)

### Container/Deployment Configs
- [ ] Update Dockerfile base images to match MSRV
- [ ] Verify docker-compose files reference correct versions
- [ ] Check Kubernetes manifests or deployment configs

---

## 4. Dependency & Security Compliance

### Vulnerability Scan

```bash
just deny     # Run cargo-deny (licenses, bans, advisories) - MUST MATCH CI
just audit    # Run cargo-audit (security vulnerabilities)
```

- [ ] Run `just deny` (comprehensive: licenses, bans, advisories)
- [ ] **CRITICAL:** `just deny` runs `cargo deny --all-features check all` to match CI exactly
- [ ] Review and address all advisories
- [ ] **Note:** `duplicate` warnings are informational (common in large dependency trees)
- [ ] **Note:** `license-not-encountered` warnings mean unused license allowances (safe to ignore)

> **Configuration:** Advisory ignores are configured in `deny.toml` with documented rationale.
> The `.cargo/audit.toml` file is for `cargo audit` only (separate tool).

### Advisory Documentation
- [ ] If ignoring an advisory, document rationale in config file
- [ ] Example: Discontinued but functional optional dependencies
- [ ] Include advisory ID, explanation, and user guidance

### License Compliance
- [ ] Verify no new dependencies violate licensing policy
- [ ] Check transitive dependencies

---

## 5. Documentation Integrity

### Link Validation

```bash
just link-check    # Uses lychee if installed, otherwise skips with warning
just doc-check     # Check documentation builds without warnings
```

CI runs `lychee` automatically. For local verification:
```bash
# Manual fallback: Check internal links
for link in $(grep -roh '\]\(\./[^)]*\.md\)' docs/*.md | sed 's/\](\.\///' | sed 's/)$//'); do
  [ ! -f "docs/$link" ] && echo "BROKEN: docs/$link"
done
```
- [ ] Run `just link-check` (or verify CI `link-check` job passed)
- [ ] Run `just doc-check` to verify docs build without warnings
- [ ] Verify internal relative links resolve: `[Guide](./guide.md)`
- [ ] Check external links haven't gone stale

### Structural Documentation
- [ ] Update ASCII art/diagrams when names change
- [ ] Verify architecture diagrams reflect current structure
- [ ] Check dependency graphs are accurate

### Changelog Maintenance
- [ ] Move "Unreleased" changes to versioned header
- [ ] Add release date
- [ ] Ensure semantic versioning adherence
- [ ] Include all breaking changes prominently

### Roadmap/Timeline Updates
- [ ] Update status tables (e.g., "Alpha Q4 2024" → "Beta - Released")
- [ ] Revise milestone dates if applicable
- [ ] Remove or update stale timeline references

### Documentation Gaps
- [ ] Identify missing documentation (e.g., troubleshooting guide)
- [ ] Ensure error messages have corresponding documentation
- [ ] Verify all public APIs are documented

---

## 6. Final Build Verification

```bash
just ci-release    # Full release CI: ci + coverage + audit + deny + semver + msrv + test-features
```

### Clean Build
```bash
just check    # Fast type check
just build    # Full debug build
```
- [ ] Verify clean compilation with all feature combinations

### Test Suite
```bash
just test         # Standard test run
just test-locked  # With locked dependencies (reproducible)
```
- [ ] All tests pass (`just test`)
- [ ] No flaky tests
- [ ] Integration tests complete successfully

### Linting (Final Pass)
```bash
just clippy    # Standard: warnings-as-errors
```
- [ ] Zero warnings
- [ ] All feature flag combinations pass

### Example Compilation
```bash
just examples    # Build all examples
```
- [ ] Run `just examples` to build all examples
- [ ] Run examples to verify they don't rely on broken APIs
- [ ] Check examples work with documented configuration

---

## 7. API Compatibility & Semver

### Breaking Change Detection

```bash
just semver    # Run cargo-semver-checks (if installed)
```

CI runs `cargo-semver-checks` on PRs. Local check (if installed):
```bash
cargo semver-checks check-release --package mcpkit
# Note: Requires cargo-semver-checks to be installed and baseline to exist
```
- [ ] Run `just semver` or verify CI `semver` job passed
- [ ] Review any flagged breaking changes
- [ ] Ensure breaking changes warrant major/minor version bump (pre-1.0: minor, post-1.0: major)

### Public API Surface
- [ ] Audit public exports for unintended exposure
- [ ] Verify `#[doc(hidden)]` items are intentional
- [ ] Check that internal modules aren't accidentally public

### Deprecations
- [ ] Add `#[deprecated]` attributes with migration guidance
- [ ] Document deprecations in CHANGELOG
- [ ] Provide minimum one release cycle warning before removal

---

## 8. Publishing Preparation (Rust/crates.io Specific)

### Pre-publish Verification

```bash
just publish-dry    # Dry-run publish all crates in dependency order
```

- [ ] Run `just publish-dry` - all crates succeed
- [ ] No unexpected files included in package
- [ ] Package size is reasonable

### Cargo.toml Metadata

```bash
just metadata-check    # Verify required metadata for crates.io
```

Check metadata for crates.io display:
```bash
cargo metadata --no-deps --format-version 1 | jq '.packages[] | select(.name == "mcpkit") | {description, repository, keywords, categories, license}'
```

**Required fields:**
- [ ] `description` - concise crate description
- [ ] `license` or `license-file` - SPDX identifier (e.g., "MIT OR Apache-2.0")
- [ ] `repository` - GitHub/source URL

**Recommended fields:**
- [ ] `keywords` - up to 5 searchable keywords
- [ ] `categories` - crates.io categories

**Optional fields** (auto-generated or not always needed):
- [ ] `documentation` - defaults to docs.rs (usually omit)
- [ ] `homepage` - only if different from repository

### Publishing Order
For workspaces with interdependencies:
- [ ] Identify dependency graph between crates
- [ ] Publish in topological order (dependencies first)
- [ ] Allow index propagation time between publishes (~30s)

### Required Secrets/Credentials
- [ ] `CARGO_REGISTRY_TOKEN` configured in CI
- [ ] `CODECOV_TOKEN` configured (if using Codecov)
- [ ] `GITHUB_TOKEN` permissions for releases

---

## 9. Git & Release Protocol

### ⚠️ Release Workflow (Follow This Order)

**Publishing to crates.io is IRREVERSIBLE.** Follow this exact sequence:

```
┌─────────────────────────────────────────────────────────────┐
│  1. PREPARE: Version bump + CHANGELOG + commit              │
│                         ↓                                   │
│  2. PUSH: git push origin main                              │
│                         ↓                                   │
│  3. WAIT: CI must pass on main (watch with `gh run watch`)  │
│                         ↓                                   │
│  4. TAG: just tag (creates v<version>)                      │
│                         ↓                                   │
│  5. RELEASE: git push origin v<version>                     │
│                         ↓                                   │
│  6. AUTOMATED: CI publishes to crates.io + GitHub Release   │
└─────────────────────────────────────────────────────────────┘
```

**Step-by-step commands:**

```bash
# Step 1: Prepare (already done if following this checklist)
# - Bump version in Cargo.toml (workspace + all crates)
# - Update CHANGELOG.md with new version section
# - Commit: git commit -m "chore: bump version to X.Y.Z"

# Step 2: Push to main
git push origin main

# Step 3: Wait for CI to pass
gh run watch                    # Interactive watch
# OR
gh run list --limit 1           # Check status

# Step 4: Create tag (ONLY after CI passes!)
just tag                        # Creates annotated tag v<version>

# Step 5: Push tag to trigger release
git push origin v<version>      # Triggers release.yml workflow

# Step 6: Monitor release
gh run watch                    # Watch publish workflow
```

### Pre-Tag Checklist

- [ ] Version bumped in all Cargo.toml files
- [ ] CHANGELOG.md updated with version and date
- [ ] Version bump committed and pushed to main
- [ ] **CI passing on main** (critical - verify before tagging!)

### Tagging

```bash
just tag    # Create annotated tag from Cargo.toml version
```

- [ ] Run `just tag` to create `v<version>` tag
- [ ] Tag matches version in Cargo.toml exactly
- [ ] Tags are annotated (not lightweight)
- [ ] Tag pushed: `git push origin v<version>`

### Release Artifacts (Automated)
- [ ] GitHub Release created (automated by release.yml)
- [ ] Release notes extracted from CHANGELOG
- [ ] Prerelease flag set appropriately for alpha/beta/rc

---

## 10. Post-Release Verification

### Publication Verification
- [ ] Crates appear on crates.io
- [ ] Documentation builds on docs.rs
- [ ] Version numbers correct on registry

### Installation Test
```bash
cargo new test-install && cd test-install
cargo add mcpkit@<new-version>
cargo build
```
- [ ] Fresh installation from registry succeeds
- [ ] Basic functionality works

### Repository Cleanup
- [ ] Update `[Unreleased]` section in CHANGELOG for next cycle
- [ ] Optionally bump version to next dev version (e.g., 0.3.0-dev)
- [ ] Close related milestones/issues

### Announcement
- [ ] Post to relevant channels (Discord, Twitter, Reddit, etc.)
- [ ] Update project website/blog if applicable

---

## Manual Recovery Procedures

### Failed CI After Tagging

If CI fails on the tag push (before publish):

```bash
# 1. Delete the remote tag
git push --delete origin v0.X.0

# 2. Delete the local tag
git tag -d v0.X.0

# 3. Fix the issue
# ... make fixes ...
git commit -m "fix: resolve CI failure"
git push origin main

# 4. Wait for CI to pass on main
gh run watch

# 5. Re-tag with same version (if no code changes) or bump patch
just tag
git push origin v0.X.0
```

### Partial Publish (Some Crates Failed)

If `just publish` fails partway through:

```bash
# 1. Check what was published
cargo search mcpkit-core mcpkit-macros mcpkit-transport # etc.

# 2. Wait for crates.io index to update (~2-5 minutes)

# 3. Resume publishing from the failed crate
cargo publish -p mcpkit-<failed-crate>
# Continue with remaining crates in dependency order
```

### Published with Bug (Critical Fix Needed)

If a critical bug is discovered after publishing:

```bash
# 1. Yank the broken version (makes it invisible but preserves existing locks)
cargo yank --version 0.X.0 mcpkit

# 2. Bump patch version in Cargo.toml
# 0.X.0 -> 0.X.1

# 3. Fix the bug
# ... make fixes ...

# 4. Full release cycle with new version
just release-check
git commit -m "fix: critical bug in v0.X.0"
git push origin main
gh run watch
just tag  # Creates v0.X.1
git push origin v0.X.1
```

### Tag Exists But Release Workflow Failed

If the tag was pushed but the release workflow didn't trigger or failed:

```bash
# 1. Check workflow status
gh run list --workflow=release.yml --limit 5

# 2. If workflow didn't trigger, re-run manually
gh workflow run release.yml --ref v0.X.0

# 3. If workflow failed, check logs and fix
gh run view <run-id> --log-failed
```

### Rollback a Release

You cannot truly "undo" a crates.io publish, but you can mitigate:

1. **Yank the version:** `cargo yank --version 0.X.0 mcpkit`
   - Prevents new projects from depending on it
   - Existing Cargo.lock files still work

2. **Publish a patch:** Release 0.X.1 with the fix or revert

3. **Update documentation:** Note the broken version in CHANGELOG.md

---

## Crate Dependency Graph

Understanding the dependency structure is critical for correct publish order.

```
Tier 0 (No internal deps):
└── mcpkit-core

Tier 1 (Depends on Tier 0):
├── mcpkit-macros → mcpkit-core
└── mcpkit-transport → mcpkit-core

Tier 2 (Depends on Tier 1):
├── mcpkit-server → mcpkit-core, mcpkit-transport
├── mcpkit-client → mcpkit-core, mcpkit-transport
└── mcpkit-testing → mcpkit-core, mcpkit-transport

Tier 3 (Integration crates):
├── mcpkit-axum → mcpkit-core, mcpkit-server
├── mcpkit-actix → mcpkit-core, mcpkit-server
├── mcpkit-rocket → mcpkit-core, mcpkit-server
└── mcpkit-warp → mcpkit-core, mcpkit-server

Tier 4 (Umbrella crate):
└── mcpkit → mcpkit-core, mcpkit-transport, mcpkit-server, mcpkit-client

Not Published:
├── mcpkit-macros-tests (test-only)
├── benches (benchmarks)
└── examples/* (example applications)
```

### Publish Order

**Always publish in this order, waiting ~30 seconds between tiers:**

1. `mcpkit-core`
2. `mcpkit-macros`, `mcpkit-transport` (can publish in parallel)
3. `mcpkit-server`, `mcpkit-client`, `mcpkit-testing` (can publish in parallel)
4. `mcpkit-axum`, `mcpkit-actix`, `mcpkit-rocket`, `mcpkit-warp` (can publish in parallel)
5. `mcpkit` (umbrella crate, last)

This is handled automatically by `just publish` which uses the correct order.

---

## Feature-Specific Testing

Before release, verify that all feature combinations work correctly.

### mcpkit (Umbrella Crate)

| Feature | Description | Test Command | Notes |
|---------|-------------|--------------|-------|
| `default` | server + client + tokio-runtime | `cargo test -p mcpkit` | Standard functionality |
| `server` | Server-side functionality | `cargo test -p mcpkit --no-default-features --features server,tokio-runtime` | Server only |
| `client` | Client-side functionality | `cargo test -p mcpkit --no-default-features --features client,tokio-runtime` | Client only |
| `tokio-runtime` | Tokio async runtime | `cargo test -p mcpkit --features tokio-runtime` | Default runtime |
| `websocket` | WebSocket transport | `cargo test -p mcpkit --features websocket` | Requires tokio |
| `http` | HTTP transport | `cargo test -p mcpkit --features http` | Requires tokio |
| `full` | All transports | `cargo test -p mcpkit --features full` | websocket + http |

### mcpkit-transport

| Feature | Description | Test Command | Notes |
|---------|-------------|--------------|-------|
| `tokio-runtime` | Tokio runtime (default) | `cargo test -p mcpkit-transport` | Standard |
| `smol-runtime` | smol/async-io runtime | `cargo test -p mcpkit-transport --no-default-features --features smol-runtime` | Lighter weight |
| `http` | HTTP/SSE transport | `cargo test -p mcpkit-transport --features http` | axum/reqwest |
| `websocket` | WebSocket transport | `cargo test -p mcpkit-transport --features websocket` | tokio-tungstenite |
| `grpc` | gRPC transport | `cargo test -p mcpkit-transport --features grpc` | tonic/prost |
| `opentelemetry` | OpenTelemetry tracing | `cargo test -p mcpkit-transport --features opentelemetry` | Observability |
| `prometheus` | Prometheus metrics | `cargo test -p mcpkit-transport --features prometheus` | Metrics export |
| `full` | All transport features | `cargo test -p mcpkit-transport --features full` | Everything |

### mcpkit-core

| Feature | Description | Test Command | Notes |
|---------|-------------|--------------|-------|
| `default` | No features | `cargo test -p mcpkit-core` | Core only |
| `fancy-errors` | Colorful error output | `cargo test -p mcpkit-core --features fancy-errors` | miette/fancy |

### Critical Feature Combinations

```bash
# Test no-default-features compiles for each crate
for crate in mcpkit-core mcpkit-transport mcpkit-server mcpkit-client; do
    cargo check -p "$crate" --no-default-features
done

# Test runtime exclusivity (should not compile with both)
cargo check -p mcpkit-transport --no-default-features --features tokio-runtime
cargo check -p mcpkit-transport --no-default-features --features smol-runtime

# Test full workspace with all features
cargo test --workspace --all-features

# Test examples compile
cargo build --package minimal-server
cargo build --package smol-server --no-default-features --features smol-runtime
```

---

## Platform-Specific Notes

### Linux

- **glibc compatibility**: Builds require glibc 2.17+ (CentOS 7+, Ubuntu 14.04+, Debian 8+)
- **musl builds**: For static binaries, use `x86_64-unknown-linux-musl` target
- **OpenSSL**: HTTP/WebSocket features may require OpenSSL dev libraries

```bash
# Install OpenSSL dev on Debian/Ubuntu
sudo apt install libssl-dev pkg-config

# Install OpenSSL dev on Fedora/RHEL
sudo dnf install openssl-devel
```

### macOS

- **Universal binaries**: Consider building for both `x86_64-apple-darwin` and `aarch64-apple-darwin`
- **Security framework**: Some crypto operations use macOS Security.framework
- **Homebrew**: If dependencies are installed via Homebrew, ensure paths are configured

### Windows

- **MSVC vs GNU**: Prefer `x86_64-pc-windows-msvc` for better compatibility
- **OpenSSL alternatives**: Consider using `rustls` instead of native-tls where available
- **Long paths**: Enable long path support if needed: `git config --system core.longpaths true`

### Cross-Platform Testing

```bash
# Verify platform-specific code compiles
cargo check --target x86_64-unknown-linux-gnu
cargo check --target x86_64-apple-darwin  # Requires macOS or cross
cargo check --target x86_64-pc-windows-msvc  # Requires Windows or cross

# CI handles cross-platform testing via matrix builds
```

---

## Security Incident Response

This section documents procedures for handling security vulnerabilities in released versions.

### Severity Assessment

| Severity | CVSS Score | Response Time | Examples |
|----------|------------|---------------|----------|
| **Critical** | 9.0-10.0 | Immediate (same day) | RCE in transport layer, auth bypass |
| **High** | 7.0-8.9 | 24-48 hours | Message injection, significant data leak |
| **Medium** | 4.0-6.9 | 1 week | Limited information disclosure, DoS |
| **Low** | 0.1-3.9 | Next release | Minor information disclosure |

### Security Release Process

1. **Assess and Confirm**
   - Verify the vulnerability is real and reproducible
   - Determine affected versions and severity
   - Check if actively exploited

2. **Develop Fix**
   - Create fix on private branch
   - Ensure fix doesn't introduce new issues
   - Prepare minimal, targeted patch

3. **Coordinate Disclosure** (for Critical/High)
   - Notify affected downstream users privately if known
   - Coordinate with security researchers if externally reported
   - Prepare security advisory

4. **Release Security Patch**
   - Follow standard release process with expedited timeline
   - Use PATCH version bump (e.g., 0.5.0 → 0.5.1)
   - Document as security fix in CHANGELOG

5. **Post-Release**
   - Publish GitHub Security Advisory
   - Request CVE if applicable
   - Update RustSec advisory database

### Security Advisory Template

```markdown
## Security Advisory: [Brief Description]

**Severity**: [Critical/High/Medium/Low]
**CVE**: [CVE-YYYY-NNNNN or "Pending"]
**Affected Versions**: [e.g., < 0.5.1]
**Fixed Versions**: [e.g., >= 0.5.1]

### Description

[Detailed description of the vulnerability]

### Impact

[What can an attacker do with this vulnerability]

### Mitigation

[Immediate steps users can take before updating]

### Resolution

Update to version X.Y.Z or later:
\`\`\`bash
cargo update -p mcpkit
\`\`\`

### Credits

[Acknowledge reporters if they consent]
```

### Yanking Considerations

For severe security issues, yank affected versions:

```bash
# Yank vulnerable versions (all affected crates)
for crate in mcpkit mcpkit-core mcpkit-transport mcpkit-server mcpkit-client; do
    cargo yank --version 0.X.Y "$crate"
done
```

---

## Lessons Learned

This section documents issues encountered in past releases and patterns to avoid.

### 1. CI Parity is Non-Negotiable

**Issue**: Local tests pass but CI fails due to environment differences.

**Solution**: Always run `just deny` and `just ci` before pushing. These commands are designed to exactly mirror CI configuration.

### 2. Version Grep Across All Docs

**Issue**: Version references in documentation become stale after bumps.

**Solution**: Run `just version-sync` and manually grep for old version strings in all markdown files, not just README.

### 3. crates.io Index Propagation

**Issue**: Publishing dependent crates immediately after dependencies causes "package not found" errors.

**Solution**: Wait 30 seconds between tiers. The `just publish` recipe handles this automatically.

### 4. Feature Combination Testing

**Issue**: Code compiles with `--all-features` but fails with specific combinations.

**Solution**: Test critical feature combinations explicitly, especially runtime features which are mutually exclusive.

### 5. Advisory Ignore Documentation

**Issue**: Ignored advisories in `deny.toml` without rationale cause confusion.

**Solution**: Always document why an advisory is ignored, including the advisory ID and user impact.

### 6. MSRV Drift

**Issue**: Using newer Rust features without updating declared MSRV.

**Solution**: Run `just msrv-check` regularly and before any release.

### 7. Panic Path Audit

**Issue**: `.unwrap()` calls in production code cause unexpected panics.

**Solution**: Run `just panic-audit` before release. Consider using `is_none_or()` pattern for graceful handling.

### 8. Float Comparison Safety

**Issue**: `partial_cmp().unwrap()` on floats panics with NaN values.

**Solution**: Use `total_cmp()` for NaN-safe float comparisons.

### 9. Tag Format Consistency

**Issue**: Tags without `v` prefix (e.g., `0.5.0` instead of `v0.5.0`) don't trigger release workflow.

**Solution**: Always use `just tag` which enforces the `vX.Y.Z` format.

### 10. Partial Publish Recovery

**Issue**: Publishing fails partway through workspace, leaving inconsistent state.

**Solution**: Check what was published with `cargo search`, wait for index propagation, then continue from the failed crate manually.

---

## Summary of Issues Addressed (Release 0.5.0)

### Code Fixes
| File | Issue | Resolution |
|------|-------|------------|
| `metrics.rs` | NaN-unsafe float comparison | `partial_cmp().unwrap()` → `total_cmp()` |
| `rate_limit.rs` | Panic when window > uptime | `checked_sub().unwrap()` → `is_none_or()` |

### Version Consistency
| Location | Issue | Resolution |
|----------|-------|------------|
| `docs/*.md` (6 files) | Old version "0.1" | Updated to "0.2" |
| `docs/architecture.md` | Old crate names in diagram | `mcp-*` → `mcpkit-*` |
| `CONTRIBUTING.md` | Wrong MSRV | 1.75 → 1.85 |
| `Dockerfile` | Wrong Rust version | 1.75 → 1.85 |

### Infrastructure
| File | Issue | Resolution |
|------|-------|------------|
| `codecov.yml` | Stale paths | Updated to current crate structure |
| `ci.yml` | Missing coverage job | Added cargo-llvm-cov + Codecov |
| `deny.toml` | Undocumented advisory ignore | Added RUSTSEC-2025-0052 rationale |

### Documentation
| File | Issue | Resolution |
|------|-------|------------|
| `troubleshooting.md` | Missing | Created comprehensive guide |
| `versioning.md` | Stale timeline | Updated milestones and status |

---

## Checklist Usage

1. **Before Release:** Work through each section systematically
2. **Blast Radius:** Version changes require grep across entire codebase
3. **Automation:** Many checks can be automated in CI
4. **Documentation:** Update this checklist as new patterns emerge

---

## CI Automation Coverage

The following checks are **automated in CI** (see `.github/workflows/`):

| Check | CI Job | Manual Needed |
|-------|--------|---------------|
| Format | `fmt` | No |
| Linting | `clippy` | No |
| Tests | `test` | No |
| MSRV | `msrv` | No |
| Cross-platform | `test-matrix` | No |
| Link validation | `link-check` | No |
| Version sync (README, getting-started) | `version-sync` | No |
| Security/license | `deny` | No |
| Semver compliance | `semver` | No |
| Doc build | `docs` | No |
| Code coverage | `coverage` | No |
| Publish to crates.io | `release.yml` | Tag triggers |
| GitHub Release | `release.yml` | Tag triggers |

**Still requires manual verification:**
- Grep for old versions in all docs (CI only checks README + getting-started)
- ASCII diagrams and architecture docs
- Post-release installation test
- Announcement/communication

---

## Justfile Recipe Mapping

Quick reference: which `just` recipes cover which checklist sections.

| Checklist Section | Just Recipe(s) | What It Covers |
|-------------------|----------------|----------------|
| **Setup** | `just setup` | Full project setup: tools, hooks, initial build |
| **Setup** | `just setup-quick` | Minimal tools and git hooks |
| **Setup** | `just setup-hooks` | Install pre-commit and pre-push hooks |
| **0. Pre-flight** | `just ci` | fmt, clippy, test, doc-check, link-check, version-sync |
| **0. Pre-flight** | `just ci-status` | Check CI status via GitHub CLI |
| **0. Pre-flight** | `just ci-watch` | Watch CI run in real-time |
| **1. Code Hygiene** | `just wip-check` | TODO/FIXME/XXX/HACK, todo!/unimplemented! |
| **1. Code Hygiene** | `just panic-audit` | .unwrap()/.expect() in production code |
| **1. Code Hygiene** | `just typos` | Spell checking |
| **1. Code Hygiene** | `just clippy` | Linting with warnings-as-errors |
| **1. Code Hygiene** | `just machete` | Unused dependencies |
| **2. Version Consistency** | `just version-sync` | README + getting-started version check |
| **3. Environment** | `just msrv-check` | MSRV compilation verification |
| **4. Security** | `just deny` | Licenses, bans, advisories |
| **4. Security** | `just audit` | Security vulnerabilities |
| **4. Security** | `just vet` | Supply chain audit |
| **4. Security** | `just check-deps` | Git/path dependencies (crates.io compliance) |
| **5. Documentation** | `just link-check` | Markdown link validation |
| **5. Documentation** | `just doc-check` | Documentation builds without warnings |
| **6. Build Verification** | `just ci-release` | Full CI + coverage + security + semver + msrv |
| **6. Build Verification** | `just test` | Test suite |
| **6. Build Verification** | `just examples` | Example compilation |
| **7. Semver** | `just semver` | Breaking change detection |
| **8. Publishing** | `just publish-dry` | Dry-run publish all crates |
| **8. Publishing** | `just publish` | Publish all crates to crates.io |
| **8. Publishing** | `just metadata-check` | Cargo.toml metadata verification |
| **9. Git Protocol** | `just tag` | Create annotated version tag (verifies CI) |
| **9. Git Protocol** | `just release-check` | Full release validation + git state |
| **Feature Testing** | `just test-features` | Basic feature matrix (no/default/all) |
| **Feature Testing** | `just test-feature-matrix` | Comprehensive per-crate feature testing |
| **Feature Testing** | `just test-runtime` | Verify runtime exclusivity (tokio/smol) |
| **Platform** | `just cross-check` | Cross-platform compilation verification |
| **Security** | `just yank <version>` | Yank version from crates.io (incidents) |
| **Security** | `just unyank <version>` | Restore yanked version |
| **Utility** | `just dep-graph` | Generate dependency graph visualization |

**Comprehensive Release Command:**
```bash
just release-check    # Runs: ci-release + wip-check + panic-audit + version-sync + typos + machete + metadata-check + git checks
```
