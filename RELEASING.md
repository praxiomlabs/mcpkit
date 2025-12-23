# Releasing MCPkit

Comprehensive guide for releasing new versions of MCPkit to crates.io.

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

## Summary of Issues Addressed (This Release)

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
| **0. Pre-flight** | `just ci` | fmt, clippy, test, doc-check, link-check, version-sync |
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
| **5. Documentation** | `just link-check` | Markdown link validation |
| **5. Documentation** | `just doc-check` | Documentation builds without warnings |
| **6. Build Verification** | `just ci-release` | Full CI + coverage + security + semver + msrv |
| **6. Build Verification** | `just test` | Test suite |
| **6. Build Verification** | `just examples` | Example compilation |
| **7. Semver** | `just semver` | Breaking change detection |
| **8. Publishing** | `just publish-dry` | Dry-run publish all crates |
| **8. Publishing** | `just publish` | Publish all crates to crates.io |
| **8. Publishing** | `just metadata-check` | Cargo.toml metadata verification |
| **9. Git Protocol** | `just tag` | Create annotated version tag |
| **9. Git Protocol** | `just release-check` | Full release validation + git state |

**Comprehensive Release Command:**
```bash
just release-check    # Runs: ci-release + wip-check + panic-audit + version-sync + typos + machete + metadata-check + git checks
```
