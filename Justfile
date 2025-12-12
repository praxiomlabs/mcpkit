# ============================================================================
# MCPkit Development Justfile
# ============================================================================
#
# Modern command runner for the MCPkit MCP SDK.
# Replaces traditional Makefile with improved UX, safety, and features.
#
# Usage:
#   just              - Show all available commands
#   just build        - Build debug
#   just ci           - Run full CI pipeline
#   just <recipe>     - Run any recipe
#
# Requirements:
#   - Just >= 1.23.0 (for [group], [confirm], [doc] attributes)
#   - Rust toolchain (rustup recommended)
#
# Install Just:
#   cargo install just
#   # or: brew install just / apt install just / pacman -S just
#
# ============================================================================

# ----------------------------------------------------------------------------
# Project Configuration
# ----------------------------------------------------------------------------

project_name := "mcpkit"
version := "0.1.0"
msrv := "1.75"
edition := "2021"
docker_image := project_name
docker_tag := version

# ----------------------------------------------------------------------------
# Tool Configuration (can be overridden via environment)
# ----------------------------------------------------------------------------

cargo := env_var_or_default("CARGO", "cargo")
docker := env_var_or_default("DOCKER", "docker")
cross := env_var_or_default("CROSS", "cross")

# Parallel jobs: auto-detect CPU count
jobs := env_var_or_default("JOBS", num_cpus())

# Runtime configuration
rust_log := env_var_or_default("RUST_LOG", "info")
rust_backtrace := env_var_or_default("RUST_BACKTRACE", "1")

# Fuzz configuration
fuzz_time := env_var_or_default("FUZZ_TIME", "60")
fuzz_target := env_var_or_default("FUZZ_TARGET", "fuzz_jsonrpc_message")

# Paths
fuzz_dir := "fuzz"
target_dir := "target"

# ----------------------------------------------------------------------------
# Platform Detection
# ----------------------------------------------------------------------------

platform := if os() == "linux" { "linux" } else if os() == "macos" { "macos" } else { "windows" }
open_cmd := if os() == "linux" { "xdg-open" } else if os() == "macos" { "open" } else { "start" }

# ----------------------------------------------------------------------------
# ANSI Color Codes
# ----------------------------------------------------------------------------

reset := '\033[0m'
bold := '\033[1m'
green := '\033[0;32m'
yellow := '\033[0;33m'
red := '\033[0;31m'
cyan := '\033[0;36m'
blue := '\033[0;34m'
magenta := '\033[0;35m'

# ----------------------------------------------------------------------------
# Default Recipe & Settings
# ----------------------------------------------------------------------------

# Show help by default
default:
    @just --list --unsorted

# Load .env file if present
set dotenv-load

# Use bash for shell commands
set shell := ["bash", "-cu"]

# Export all variables to child processes
set export

# ============================================================================
# CORE BUILD RECIPES
# ============================================================================

[group('build')]
[doc("Build workspace in debug mode")]
build:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Building (debug) ══════{{reset}}\n\n'
    {{cargo}} build --workspace --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Build complete\n'

[group('build')]
[doc("Build workspace in release mode with optimizations")]
release:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Building (release) ══════{{reset}}\n\n'
    {{cargo}} build --workspace --all-features --release -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Release build complete\n'

[group('build')]
[doc("Fast type check without code generation")]
check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Type checking...\n'
    {{cargo}} check --workspace --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Type check passed\n'

[group('build')]
[doc("Analyze build times")]
build-timing:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Building with timing analysis...\n'
    {{cargo}} build --workspace --all-features --timings
    printf '{{green}}[OK]{{reset}}   Build timing report generated (see target/cargo-timings/)\n'

[group('build')]
[confirm("This will delete all build artifacts. Continue?")]
[doc("Clean all build artifacts")]
clean:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Cleaning build artifacts...\n'
    {{cargo}} clean
    rm -rf coverage/ lcov.info *.profraw *.profdata
    printf '{{green}}[OK]{{reset}}   Clean complete\n'

[group('build')]
[doc("Clean and rebuild from scratch")]
rebuild: clean build

# ============================================================================
# TESTING RECIPES
# ============================================================================

[group('test')]
[doc("Run all tests")]
test:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests ══════{{reset}}\n\n'
    {{cargo}} test --workspace --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Run tests with locked dependencies (reproducible)")]
test-locked:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (locked) ══════{{reset}}\n\n'
    {{cargo}} test --workspace --all-features --locked -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed (locked)\n'

[group('test')]
[doc("Run tests with output visible")]
test-verbose:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (verbose) ══════{{reset}}\n\n'
    {{cargo}} test --workspace --all-features -j {{jobs}} -- --nocapture
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Test specific crate")]
test-crate crate:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Testing crate: {{crate}}\n'
    {{cargo}} test -p {{crate}} --all-features -- --nocapture
    printf '{{green}}[OK]{{reset}}   Crate tests passed\n'

[group('test')]
[doc("Run documentation tests only")]
test-doc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running doc tests...\n'
    {{cargo}} test --workspace --all-features --doc
    printf '{{green}}[OK]{{reset}}   Doc tests passed\n'

[group('test')]
[doc("Run ignored/slow tests")]
test-ignored:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running ignored tests...\n'
    {{cargo}} test --workspace --all-features -- --ignored
    printf '{{green}}[OK]{{reset}}   Ignored tests complete\n'

[group('test')]
[doc("Run tests with cargo-nextest (faster, parallel)")]
nextest:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (nextest) ══════{{reset}}\n\n'
    {{cargo}} nextest run --workspace --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed\n'

[group('test')]
[doc("Run tests with nextest and locked dependencies")]
nextest-locked:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (nextest, locked) ══════{{reset}}\n\n'
    {{cargo}} nextest run --workspace --all-features --locked -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   All tests passed (locked)\n'

[group('test')]
[doc("Run tests under Miri for undefined behavior detection")]
miri:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Miri ══════{{reset}}\n\n'
    {{cargo}} +nightly miri test --workspace
    printf '{{green}}[OK]{{reset}}   Miri passed (no UB detected)\n'

[group('test')]
[doc("Run tests with extra UB detection via cargo-careful")]
test-careful:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Tests (careful) ══════{{reset}}\n\n'
    {{cargo}} +nightly careful test --workspace --all-features
    printf '{{green}}[OK]{{reset}}   Careful tests passed (no UB detected)\n'

[group('test')]
[doc("Run tests with various feature combinations")]
test-features:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Testing Feature Matrix ══════{{reset}}\n\n'
    printf '{{cyan}}[INFO]{{reset}} Testing with no features...\n'
    {{cargo}} test --workspace --no-default-features -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with default features...\n'
    {{cargo}} test --workspace -j {{jobs}}
    printf '{{cyan}}[INFO]{{reset}} Testing with all features...\n'
    {{cargo}} test --workspace --all-features -j {{jobs}}
    printf '{{green}}[OK]{{reset}}   Feature matrix tests passed\n'

# ============================================================================
# CODE QUALITY RECIPES
# ============================================================================

[group('lint')]
[doc("Format all code")]
fmt:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Formatting code...\n'
    {{cargo}} fmt --all
    printf '{{green}}[OK]{{reset}}   Formatting complete\n'

[group('lint')]
[doc("Check code formatting")]
fmt-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking format...\n'
    {{cargo}} fmt --all -- --check
    printf '{{green}}[OK]{{reset}}   Format check passed\n'

[group('lint')]
[doc("Run clippy lints (workspace-configured)")]
clippy:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy...\n'
    {{cargo}} clippy --all-targets --all-features
    printf '{{green}}[OK]{{reset}}   Clippy passed\n'

[group('lint')]
[doc("Run clippy with strict deny on warnings")]
clippy-strict:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy (strict)...\n'
    {{cargo}} clippy --all-targets --all-features -- \
        -D warnings \
        -D clippy::all \
        -D clippy::pedantic \
        -D clippy::nursery \
        -A clippy::module_name_repetitions \
        -A clippy::too_many_lines
    printf '{{green}}[OK]{{reset}}   Clippy (strict) passed\n'

[group('lint')]
[doc("Auto-fix clippy warnings")]
clippy-fix:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Auto-fixing clippy warnings...\n'
    {{cargo}} clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    printf '{{green}}[OK]{{reset}}   Clippy fixes applied\n'

[group('security')]
[doc("Security vulnerability audit via cargo-audit")]
audit:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running security audit...\n'
    {{cargo}} audit
    printf '{{green}}[OK]{{reset}}   Security audit passed\n'

[group('security')]
[doc("Run cargo-deny checks (licenses, bans, advisories)")]
deny:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running cargo-deny...\n'
    {{cargo}} deny check
    printf '{{green}}[OK]{{reset}}   Deny checks passed\n'

[group('lint')]
[doc("Find unused dependencies via cargo-udeps (requires nightly)")]
udeps:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Finding unused dependencies...\n'
    {{cargo}} +nightly udeps --workspace --all-features
    printf '{{green}}[OK]{{reset}}   Unused deps check complete\n'

[group('lint')]
[doc("Find unused dependencies via cargo-machete (fast, heuristic)")]
machete:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Finding unused dependencies (fast)...\n'
    {{cargo}} machete
    printf '{{green}}[OK]{{reset}}   Machete check complete\n'

[group('lint')]
[doc("Verify MSRV compliance")]
msrv-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking MSRV {{msrv}}...\n'
    {{cargo}} +{{msrv}} check --workspace --all-features
    printf '{{green}}[OK]{{reset}}   MSRV {{msrv}} check passed\n'

[group('lint')]
[doc("Test with minimal dependency versions")]
minimal-versions:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Testing minimal versions...\n'
    {{cargo}} +nightly -Z minimal-versions check --workspace --all-features
    printf '{{green}}[OK]{{reset}}   Minimal versions check passed\n'

[group('lint')]
[doc("Check for semver violations (for library crates)")]
semver:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking semver compliance...\n'
    {{cargo}} semver-checks check-release
    printf '{{green}}[OK]{{reset}}   Semver check passed\n'

[group('security')]
[doc("Supply chain security audit via cargo-vet")]
vet:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running supply chain audit...\n'
    {{cargo}} vet
    printf '{{green}}[OK]{{reset}}   Supply chain audit passed\n'

[group('lint')]
[doc("Run all lints (fmt + clippy)")]
lint: fmt-check clippy
    @printf '{{green}}[OK]{{reset}}   All lints passed\n'

[group('lint')]
[doc("Run comprehensive lint suite")]
lint-full: fmt-check clippy-strict audit deny machete
    @printf '{{green}}[OK]{{reset}}   Full lint suite passed\n'

# ============================================================================
# DOCUMENTATION RECIPES
# ============================================================================

[group('docs')]
[doc("Generate documentation")]
doc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation...\n'
    {{cargo}} doc --workspace --all-features --no-deps
    printf '{{green}}[OK]{{reset}}   Documentation generated\n'

[group('docs')]
[doc("Generate and open documentation")]
doc-open:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation...\n'
    {{cargo}} doc --workspace --all-features --no-deps --open
    printf '{{green}}[OK]{{reset}}   Documentation opened\n'

[group('docs')]
[doc("Generate docs including private items")]
doc-private:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating documentation (with private items)...\n'
    {{cargo}} doc --workspace --all-features --no-deps --document-private-items --open
    printf '{{green}}[OK]{{reset}}   Documentation opened\n'

[group('docs')]
[doc("Check documentation for warnings")]
doc-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking documentation...\n'
    RUSTDOCFLAGS="-D warnings" {{cargo}} doc --workspace --all-features --no-deps
    printf '{{green}}[OK]{{reset}}   Documentation check passed\n'

# ============================================================================
# COVERAGE RECIPES
# ============================================================================

[group('coverage')]
[doc("Generate HTML coverage report and open in browser")]
coverage:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Generating Coverage Report ══════{{reset}}\n\n'
    {{cargo}} llvm-cov --workspace --all-features --html --open
    printf '{{green}}[OK]{{reset}}   Coverage report opened\n'

[group('coverage')]
[doc("Generate LCOV coverage for CI integration")]
coverage-lcov output="lcov.info":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating LCOV coverage...\n'
    {{cargo}} llvm-cov --workspace --all-features --lcov --output-path {{output}}
    printf '{{green}}[OK]{{reset}}   Coverage saved to {{output}}\n'

[group('coverage')]
[doc("Generate coverage with nextest (faster)")]
coverage-nextest:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Generating Coverage (nextest) ══════{{reset}}\n\n'
    {{cargo}} llvm-cov nextest --workspace --all-features --html --open
    printf '{{green}}[OK]{{reset}}   Coverage report opened\n'

[group('coverage')]
[doc("Show coverage summary in terminal")]
coverage-summary:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Coverage summary:\n'
    {{cargo}} llvm-cov --workspace --all-features --text

[group('coverage')]
[doc("Generate Codecov-compatible coverage")]
coverage-codecov output="codecov.json":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Generating Codecov coverage...\n'
    {{cargo}} llvm-cov --workspace --all-features --codecov --output-path {{output}}
    printf '{{green}}[OK]{{reset}}   Coverage saved to {{output}}\n'

# ============================================================================
# FUZZING RECIPES
# ============================================================================

[group('fuzz')]
[doc("Run default fuzz target")]
fuzz target=fuzz_target time=fuzz_time:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Fuzzing: {{target}} ══════{{reset}}\n\n'
    cd {{fuzz_dir}} && {{cargo}} +nightly fuzz run {{target}} -- -max_total_time={{time}}
    printf '{{green}}[OK]{{reset}}   Fuzzing complete\n'

[group('fuzz')]
[doc("List available fuzz targets")]
fuzz-list:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Available fuzz targets:\n'
    cd {{fuzz_dir}} && {{cargo}} +nightly fuzz list

[group('fuzz')]
[doc("Fuzz JSON-RPC message parsing")]
fuzz-jsonrpc-message time=fuzz_time:
    @just fuzz fuzz_jsonrpc_message {{time}}

[group('fuzz')]
[doc("Fuzz JSON-RPC request parsing")]
fuzz-jsonrpc-request time=fuzz_time:
    @just fuzz fuzz_jsonrpc_request {{time}}

[group('fuzz')]
[doc("Fuzz JSON-RPC response parsing")]
fuzz-jsonrpc-response time=fuzz_time:
    @just fuzz fuzz_jsonrpc_response {{time}}

[group('fuzz')]
[doc("Fuzz JSON-RPC structured messages")]
fuzz-jsonrpc-structured time=fuzz_time:
    @just fuzz fuzz_jsonrpc_structured {{time}}

[group('fuzz')]
[doc("Fuzz progress token handling")]
fuzz-progress-token time=fuzz_time:
    @just fuzz fuzz_progress_token {{time}}

[group('fuzz')]
[doc("Run all fuzz targets briefly (smoke test)")]
fuzz-all time="30":
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Fuzzing All Targets ══════{{reset}}\n\n'
    for target in fuzz_jsonrpc_message fuzz_jsonrpc_request fuzz_jsonrpc_response \
                  fuzz_jsonrpc_structured fuzz_progress_token; do
        printf '{{cyan}}[INFO]{{reset}} Fuzzing %s...\n' "$target"
        cd {{fuzz_dir}} && {{cargo}} +nightly fuzz run "$target" -- -max_total_time={{time}}
    done
    printf '{{green}}[OK]{{reset}}   All fuzz targets complete\n'

[group('fuzz')]
[doc("Run mutation testing via cargo-mutants")]
mutants package="mcpkit-core":
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Mutation Tests ══════{{reset}}\n\n'
    {{cargo}} mutants --package {{package}} --jobs {{jobs}} --timeout 300
    printf '{{green}}[OK]{{reset}}   Mutation testing complete\n'

# ============================================================================
# EXAMPLE RECIPES
# ============================================================================

[group('examples')]
[doc("Build all examples")]
examples:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Building all examples...\n'
    {{cargo}} build --examples --all-features
    printf '{{green}}[OK]{{reset}}   Examples built\n'

[group('examples')]
[doc("Run minimal server example")]
example-minimal:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running minimal-server example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p minimal-server

[group('examples')]
[doc("Run full server example")]
example-full:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running full-server example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p full-server

[group('examples')]
[doc("Run HTTP server example")]
example-http:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running http-server example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p http-server

[group('examples')]
[doc("Run WebSocket server example")]
example-websocket:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running websocket-server example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p websocket-server

[group('examples')]
[doc("Run client example")]
example-client:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running client-example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p client-example

[group('examples')]
[doc("Run middleware example")]
example-middleware:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running with-middleware example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p with-middleware

[group('examples')]
[doc("Run database server example")]
example-database:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running database-server example...\n'
    RUST_LOG={{rust_log}} {{cargo}} run -p database-server

# ============================================================================
# BENCHMARK RECIPES
# ============================================================================

[group('bench')]
[doc("Run benchmarks")]
bench:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Running Benchmarks ══════{{reset}}\n\n'
    {{cargo}} bench --workspace
    printf '{{green}}[OK]{{reset}}   Benchmarks complete\n'

[group('bench')]
[doc("Run benchmarks and save baseline")]
bench-save name="baseline":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running benchmarks (saving baseline: {{name}})...\n'
    {{cargo}} bench --workspace -- --save-baseline {{name}}
    printf '{{green}}[OK]{{reset}}   Baseline saved: {{name}}\n'

[group('bench')]
[doc("Run benchmarks and compare to baseline")]
bench-compare name="baseline":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Comparing to baseline: {{name}}...\n'
    {{cargo}} bench --workspace -- --baseline {{name}}
    printf '{{green}}[OK]{{reset}}   Comparison complete\n'

# ============================================================================
# DOCKER RECIPES
# ============================================================================

[group('docker')]
[doc("Build Docker image")]
docker-build:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Building Docker image {{docker_image}}:{{docker_tag}}...\n'
    {{docker}} build -t {{docker_image}}:{{docker_tag}} -t {{docker_image}}:latest .
    printf '{{green}}[OK]{{reset}}   Docker image built\n'

[group('docker')]
[doc("Run tests in Docker container")]
docker-test:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running tests in Docker...\n'
    {{docker}} run --rm -v "$(pwd):/workspace" -w /workspace {{docker_image}}:{{docker_tag}} \
        cargo test --workspace --all-features --locked
    printf '{{green}}[OK]{{reset}}   Docker tests passed\n'

[group('docker')]
[doc("Run CI pipeline in Docker")]
docker-ci:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running CI in Docker...\n'
    {{docker}} run --rm -v "$(pwd):/workspace" -w /workspace {{docker_image}}:{{docker_tag}} \
        bash -c "cargo fmt --check && cargo clippy --all-targets --all-features && cargo test --workspace --all-features --locked"
    printf '{{green}}[OK]{{reset}}   Docker CI passed\n'

[group('docker')]
[doc("Interactive shell in Docker container")]
docker-shell:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Opening Docker shell...\n'
    {{docker}} run --rm -it -v "$(pwd):/workspace" -w /workspace {{docker_image}}:{{docker_tag}} /bin/bash

# ============================================================================
# DEVELOPMENT WORKFLOW RECIPES
# ============================================================================

[group('dev')]
[doc("Full development setup")]
dev: build test lint
    @printf '{{green}}[OK]{{reset}}   Development environment ready\n'

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run tests on file changes")]
watch:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (tests)...\n'
    {{cargo}} watch -x "test --workspace --all-features"

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run check on file changes")]
watch-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (check)...\n'
    {{cargo}} watch -x "check --workspace --all-features"

[group('dev')]
[no-exit-message]
[doc("Watch mode: re-run clippy on file changes")]
watch-clippy:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Watching for changes (clippy)...\n'
    {{cargo}} watch -x "clippy --all-targets --all-features"

# ============================================================================
# CI/CD RECIPES
# ============================================================================

[group('ci')]
[doc("Standard CI pipeline (fmt, clippy, test, doc)")]
ci: fmt-check clippy test-locked doc-check
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ CI Pipeline Complete ══════{{reset}}\n\n'
    printf '{{green}}[OK]{{reset}}   All CI checks passed\n'

[group('ci')]
[doc("Fast CI checks (no tests)")]
ci-fast: fmt-check clippy check
    @printf '{{green}}[OK]{{reset}}   Fast CI checks passed\n'

[group('ci')]
[doc("Full CI with coverage and security audit")]
ci-full: ci coverage-lcov audit deny
    @printf '{{green}}[OK]{{reset}}   Full CI pipeline passed\n'

[group('ci')]
[doc("Complete CI with all checks (for releases)")]
ci-release: ci-full semver msrv-check test-features
    @printf '{{green}}[OK]{{reset}}   Release CI pipeline passed\n'

[group('ci')]
[doc("Pre-commit hook checks")]
pre-commit: fmt-check clippy check
    @printf '{{green}}[OK]{{reset}}   Pre-commit checks passed\n'

[group('ci')]
[doc("Pre-push hook checks")]
pre-push: ci
    @printf '{{green}}[OK]{{reset}}   Pre-push checks passed\n'

# ============================================================================
# DEPENDENCY MANAGEMENT
# ============================================================================

[group('deps')]
[doc("Check for outdated dependencies")]
outdated:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking for outdated dependencies...\n'
    {{cargo}} outdated -R

[group('deps')]
[doc("Update Cargo.lock to latest compatible versions")]
update:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Updating dependencies...\n'
    {{cargo}} update
    printf '{{green}}[OK]{{reset}}   Dependencies updated\n'

[group('deps')]
[doc("Update specific dependency")]
update-dep package:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Updating {{package}}...\n'
    {{cargo}} update -p {{package}}
    printf '{{green}}[OK]{{reset}}   {{package}} updated\n'

[group('deps')]
[doc("Show dependency tree")]
tree:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Dependency tree:\n'
    {{cargo}} tree --workspace

[group('deps')]
[doc("Show duplicate dependencies")]
tree-duplicates:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Duplicate dependencies:\n'
    {{cargo}} tree --workspace --duplicates

[group('deps')]
[doc("Show dependencies with specific features")]
tree-features package:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Features for {{package}}:\n'
    {{cargo}} tree -p {{package}} -f "{p} {f}"

# ============================================================================
# RELEASE RECIPES
# ============================================================================

[group('release')]
[doc("Prepare for release (full validation)")]
release-check: ci-release
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Release Validation ══════{{reset}}\n\n'
    printf '{{cyan}}[INFO]{{reset}} Checking for uncommitted changes...\n'
    if ! git diff-index --quiet HEAD --; then
        printf '{{red}}[ERR]{{reset}}  Uncommitted changes detected\n'
        exit 1
    fi
    printf '{{cyan}}[INFO]{{reset}} Checking for unpushed commits...\n'
    if [ -n "$(git log @{u}.. 2>/dev/null)" ]; then
        printf '{{yellow}}[WARN]{{reset}} Unpushed commits detected\n'
    fi
    printf '{{green}}[OK]{{reset}}   Ready for release\n'

[group('release')]
[doc("Publish all crates to crates.io (dry run)")]
publish-dry:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Publishing (dry run)...\n'
    # Publish in dependency order
    {{cargo}} publish --dry-run -p mcpkit-core
    {{cargo}} publish --dry-run -p mcpkit-macros
    {{cargo}} publish --dry-run -p mcpkit-transport
    {{cargo}} publish --dry-run -p mcpkit-server
    {{cargo}} publish --dry-run -p mcpkit-client
    {{cargo}} publish --dry-run -p mcpkit-testing
    {{cargo}} publish --dry-run -p mcpkit-axum
    {{cargo}} publish --dry-run -p mcpkit-actix
    {{cargo}} publish --dry-run -p mcpkit
    printf '{{green}}[OK]{{reset}}   Dry run complete\n'

[group('release')]
[doc("Create git tag for release")]
tag:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Creating tag v{{version}}...\n'
    git tag -a "v{{version}}" -m "Release v{{version}}"
    printf '{{green}}[OK]{{reset}}   Tag created: v{{version}}\n'

# ============================================================================
# UTILITIES
# ============================================================================

[group('util')]
[doc("Count lines of code")]
loc:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Lines of code:\n'
    tokei . --exclude target --exclude node_modules 2>/dev/null || \
        find crates -name '*.rs' | xargs wc -l | tail -1

[group('util')]
[doc("Analyze binary size bloat")]
bloat crate="mcpkit":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Binary size analysis for {{crate}}...\n'
    {{cargo}} bloat --release -p {{crate}} --crates

[group('security')]
[doc("Check for unsafe code usage")]
geiger:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Scanning for unsafe code...\n'
    for crate in crates/*/; do
        name=$(basename "$crate")
        printf '{{cyan}}[INFO]{{reset}} Scanning %s...\n' "$name"
        {{cargo}} geiger -p "$name" --all-features --all-targets 2>/dev/null || true
    done
    printf '{{green}}[OK]{{reset}}   Unsafe code scan complete\n'

[group('util')]
[doc("Show expanded macros")]
expand crate:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Expanding macros in {{crate}}...\n'
    {{cargo}} expand -p {{crate}}

[group('util')]
[doc("Generate and display project statistics")]
stats: loc
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Project Statistics ══════{{reset}}\n\n'
    printf '{{cyan}}Crates:{{reset}}\n'
    find crates -maxdepth 1 -type d | tail -n +2 | while read dir; do
        name=$(basename "$dir")
        printf '  - %s\n' "$name"
    done
    printf '\n{{cyan}}Examples:{{reset}}\n'
    find examples -maxdepth 1 -type d | tail -n +2 | while read dir; do
        name=$(basename "$dir")
        printf '  - %s\n' "$name"
    done
    printf '\n{{cyan}}Dependencies:{{reset}}\n'
    printf '  Direct: %s\n' "$({{cargo}} tree --workspace --depth 1 | grep -c '├\|└')"
    printf '  Total:  %s\n' "$({{cargo}} tree --workspace | wc -l)"
    printf '\n'

# ============================================================================
# HELP & DOCUMENTATION
# ============================================================================

[group('help')]
[doc("Show version and environment info")]
info:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{project_name}} v{{version}}{{reset}}\n'
    printf '═══════════════════════════════════════\n'
    printf '{{cyan}}MSRV:{{reset}}      {{msrv}}\n'
    printf '{{cyan}}Edition:{{reset}}   {{edition}}\n'
    printf '{{cyan}}Platform:{{reset}}  {{platform}}\n'
    printf '{{cyan}}Jobs:{{reset}}      {{jobs}}\n'
    printf '\n{{cyan}}Rust:{{reset}}      %s\n' "$(rustc --version)"
    printf '{{cyan}}Cargo:{{reset}}     %s\n' "$(cargo --version)"
    printf '{{cyan}}Just:{{reset}}      %s\n' "$(just --version)"
    printf '\n'

[group('help')]
[doc("Check which development tools are installed")]
check-tools:
    #!/usr/bin/env bash
    printf '\n{{bold}}Development Tool Status{{reset}}\n'
    printf '═══════════════════════════════════════\n'

    check_tool() {
        if command -v "$1" &> /dev/null || {{cargo}} "$1" --version &> /dev/null 2>&1; then
            printf '{{green}}✓{{reset}} %s\n' "$1"
        else
            printf '{{red}}✗{{reset}} %s (not installed)\n' "$1"
        fi
    }

    # Core tools
    printf '\n{{cyan}}Core:{{reset}}\n'
    check_tool "rustfmt"
    check_tool "clippy"

    # Cargo extensions
    printf '\n{{cyan}}Cargo Extensions:{{reset}}\n'
    for tool in nextest llvm-cov audit deny outdated watch mutants \
                semver-checks machete vet bloat geiger expand careful; do
        if {{cargo}} $tool --version &> /dev/null 2>&1; then
            printf '{{green}}✓{{reset}} cargo-%s\n' "$tool"
        else
            printf '{{red}}✗{{reset}} cargo-%s\n' "$tool"
        fi
    done

    # External tools
    printf '\n{{cyan}}External:{{reset}}\n'
    check_tool "tokei"
    check_tool "cross"
    check_tool "docker"

    printf '\n'

[group('help')]
[doc("Show all available recipes grouped by category")]
help:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{project_name}} v{{version}}{{reset}} — MCP SDK Development Command Runner\n'
    printf 'MSRV: {{msrv}} | Edition: {{edition}} | Platform: {{platform}}\n\n'
    printf '{{bold}}Usage:{{reset}} just [recipe] [arguments...]\n\n'
    just --list --unsorted
