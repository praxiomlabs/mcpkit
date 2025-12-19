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
# Version is read dynamically from Cargo.toml to avoid drift
version := `cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "mcpkit") | .version'`
msrv := "1.85"
edition := "2024"
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
[doc("Run clippy lints (matches CI configuration)")]
clippy:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy...\n'
    {{cargo}} clippy --workspace --all-features --all-targets -- -D warnings
    printf '{{green}}[OK]{{reset}}   Clippy passed\n'

[group('lint')]
[doc("Run clippy with strict deny on warnings")]
clippy-strict:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running clippy (strict)...\n'
    {{cargo}} clippy --workspace --all-targets --all-features -- \
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
    {{cargo}} clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged
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

[group('docs')]
[doc("Check markdown links (requires lychee)")]
link-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking markdown links...\n'
    if ! command -v lychee &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} lychee not installed (cargo install lychee)\n'
        printf '{{yellow}}[WARN]{{reset}} Skipping link check\n'
        exit 0
    fi
    lychee --verbose --no-progress --accept 200,204,206 \
        --exclude '^https://crates.io' \
        --exclude '^https://docs.rs' \
        --exclude '^https://www.reddit.com' \
        './docs/**/*.md' './README.md' './CONTRIBUTING.md'
    printf '{{green}}[OK]{{reset}}   Link check passed\n'

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
    # Examples are workspace packages, not --examples targets
    {{cargo}} build -p minimal-server -p full-server -p http-server-example \
        -p client-example -p database-server-example -p websocket-server-example \
        -p with-middleware-example -p filesystem-server
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

[group('examples')]
[doc("Run filesystem server example")]
example-filesystem sandbox="/tmp/mcpkit-sandbox":
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running filesystem-server example...\n'
    mkdir -p {{sandbox}}
    RUST_LOG={{rust_log}} {{cargo}} run -p filesystem-server -- {{sandbox}}

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
        bash -c "cargo fmt --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace --all-features --locked"
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
    {{cargo}} watch -x "clippy --workspace --all-targets --all-features"

# ============================================================================
# CI/CD RECIPES
# ============================================================================

[group('ci')]
[doc("Check documentation versions match Cargo.toml")]
version-sync:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking version sync...\n'
    VERSION=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "mcpkit") | .version')
    MAJOR_MINOR=$(echo "$VERSION" | cut -d. -f1,2)

    # Check README.md
    if ! grep -q "mcpkit = \"$MAJOR_MINOR\"" README.md; then
        printf '{{red}}[ERR]{{reset}}  README.md version mismatch (expected %s)\n' "$MAJOR_MINOR"
        exit 1
    fi

    # Check docs/getting-started.md
    if ! grep -q "mcpkit = \"$MAJOR_MINOR\"" docs/getting-started.md; then
        printf '{{red}}[ERR]{{reset}}  docs/getting-started.md version mismatch (expected %s)\n' "$MAJOR_MINOR"
        exit 1
    fi
    printf '{{green}}[OK]{{reset}}   Version sync passed (v%s)\n' "$MAJOR_MINOR"

[group('ci')]
[doc("Standard CI pipeline (matches GitHub Actions)")]
ci: fmt-check clippy test-locked doc-check link-check version-sync
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

# ============================================================================
# RELEASE CHECKLIST RECIPES
# ============================================================================

[group('release')]
[doc("Check for WIP markers (TODO, FIXME, XXX, HACK, todo!, unimplemented!)")]
wip-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking for WIP markers...\n'

    # Search for comment markers
    COMMENTS=$(grep -rn "TODO\|FIXME\|XXX\|HACK" --include="*.rs" crates/ 2>/dev/null || true)
    if [ -n "$COMMENTS" ]; then
        printf '{{yellow}}[WARN]{{reset}} Found WIP comments:\n'
        echo "$COMMENTS" | head -20
        COMMENT_COUNT=$(echo "$COMMENTS" | wc -l)
        if [ "$COMMENT_COUNT" -gt 20 ]; then
            printf '{{yellow}}[WARN]{{reset}} ... and %d more\n' "$((COMMENT_COUNT - 20))"
        fi
    fi

    # Search for incomplete macros (excluding tests)
    MACROS=$(grep -rn "todo!\|unimplemented!" --include="*.rs" crates/*/src/ 2>/dev/null || true)
    if [ -n "$MACROS" ]; then
        printf '{{red}}[ERR]{{reset}}  Found incomplete macros in production code:\n'
        echo "$MACROS"
        exit 1
    fi

    printf '{{green}}[OK]{{reset}}   WIP check passed (no blocking issues)\n'

[group('release')]
[doc("Audit panic paths (.unwrap(), .expect()) in production code")]
panic-audit:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Auditing panic paths in production code...\n'

    # Find .unwrap() in src/ directories (production code)
    UNWRAPS=$(grep -rn "\.unwrap()" crates/*/src/ --include="*.rs" 2>/dev/null || true)
    EXPECTS=$(grep -rn "\.expect(" crates/*/src/ --include="*.rs" 2>/dev/null || true)

    if [ -n "$UNWRAPS" ] || [ -n "$EXPECTS" ]; then
        printf '{{yellow}}[WARN]{{reset}} Found potential panic paths:\n'
        if [ -n "$UNWRAPS" ]; then
            echo "$UNWRAPS" | head -15
            UNWRAP_COUNT=$(echo "$UNWRAPS" | wc -l)
            printf '{{cyan}}[INFO]{{reset}} Total .unwrap() calls: %d\n' "$UNWRAP_COUNT"
        fi
        if [ -n "$EXPECTS" ]; then
            echo "$EXPECTS" | head -10
            EXPECT_COUNT=$(echo "$EXPECTS" | wc -l)
            printf '{{cyan}}[INFO]{{reset}} Total .expect() calls: %d\n' "$EXPECT_COUNT"
        fi
        printf '{{yellow}}[NOTE]{{reset}} Review each for production safety. High line numbers may be test modules.\n'
    else
        printf '{{green}}[OK]{{reset}}   No panic paths found in production code\n'
    fi

[group('release')]
[doc("Verify Cargo.toml metadata for crates.io publishing")]
metadata-check:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Checking Cargo.toml metadata...\n'

    METADATA=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "mcpkit")')

    # Required fields
    DESC=$(echo "$METADATA" | jq -r '.description // empty')
    LICENSE=$(echo "$METADATA" | jq -r '.license // empty')
    REPO=$(echo "$METADATA" | jq -r '.repository // empty')

    MISSING=""
    [ -z "$DESC" ] && MISSING="$MISSING description"
    [ -z "$LICENSE" ] && MISSING="$MISSING license"
    [ -z "$REPO" ] && MISSING="$MISSING repository"

    if [ -n "$MISSING" ]; then
        printf '{{red}}[ERR]{{reset}}  Missing required fields:%s\n' "$MISSING"
        exit 1
    fi

    # Recommended fields
    KEYWORDS=$(echo "$METADATA" | jq -r '.keywords // [] | length')
    CATEGORIES=$(echo "$METADATA" | jq -r '.categories // [] | length')

    [ "$KEYWORDS" -eq 0 ] && printf '{{yellow}}[WARN]{{reset}} No keywords defined (recommended for discoverability)\n'
    [ "$CATEGORIES" -eq 0 ] && printf '{{yellow}}[WARN]{{reset}} No categories defined (recommended for discoverability)\n'

    printf '{{cyan}}[INFO]{{reset}} Package metadata:\n'
    printf '  description: %s\n' "$DESC"
    printf '  license:     %s\n' "$LICENSE"
    printf '  repository:  %s\n' "$REPO"
    printf '  keywords:    %d defined\n' "$KEYWORDS"
    printf '  categories:  %d defined\n' "$CATEGORIES"

    printf '{{green}}[OK]{{reset}}   Metadata check passed\n'

[group('release')]
[doc("Run typos spell checker")]
typos:
    #!/usr/bin/env bash
    printf '{{cyan}}[INFO]{{reset}} Running typos spell checker...\n'
    if ! command -v typos &> /dev/null; then
        printf '{{yellow}}[WARN]{{reset}} typos not installed (cargo install typos-cli)\n'
        exit 0
    fi
    typos crates/ docs/ README.md CHANGELOG.md RELEASING.md
    printf '{{green}}[OK]{{reset}}   Typos check passed\n'

[group('release')]
[doc("Prepare for release (full validation)")]
release-check: ci-release wip-check panic-audit version-sync typos machete metadata-check
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
[confirm("This will publish to crates.io. This action is IRREVERSIBLE. Continue?")]
[doc("Publish all crates to crates.io in dependency order")]
publish:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{blue}}══════ Publishing to crates.io ══════{{reset}}\n\n'
    printf '{{yellow}}[WARN]{{reset}} This action is IRREVERSIBLE!\n'

    # Tier 0: Core crates
    printf '{{cyan}}[INFO]{{reset}} Publishing core crates...\n'
    {{cargo}} publish -p mcpkit-core
    {{cargo}} publish -p mcpkit-macros
    printf '{{cyan}}[INFO]{{reset}} Waiting for crates.io index propagation...\n'
    sleep 30

    # Tier 1: Transport and framework crates
    printf '{{cyan}}[INFO]{{reset}} Publishing transport and framework crates...\n'
    {{cargo}} publish -p mcpkit-transport
    {{cargo}} publish -p mcpkit-server
    {{cargo}} publish -p mcpkit-client
    {{cargo}} publish -p mcpkit-testing
    sleep 30

    # Tier 2: Integration crates
    printf '{{cyan}}[INFO]{{reset}} Publishing integration crates...\n'
    {{cargo}} publish -p mcpkit-axum
    {{cargo}} publish -p mcpkit-actix
    sleep 30

    # Tier 3: Umbrella crate
    printf '{{cyan}}[INFO]{{reset}} Publishing umbrella crate...\n'
    {{cargo}} publish -p mcpkit

    printf '\n{{green}}[OK]{{reset}}   All crates published successfully!\n'
    printf '{{cyan}}[INFO]{{reset}} Next steps:\n'
    printf '  1. Verify: cargo search mcpkit\n'
    printf '  2. Check docs.rs in ~15 minutes\n'
    printf '  3. Update CHANGELOG.md [Unreleased] section\n'

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
[doc("Install all recommended development tools")]
install-tools:
    #!/usr/bin/env bash
    printf '\n{{bold}}Installing Development Tools{{reset}}\n'
    printf '═══════════════════════════════════════\n'

    # Core cargo extensions (required for CI)
    printf '\n{{cyan}}[INFO]{{reset}} Installing required tools...\n'
    {{cargo}} install cargo-audit cargo-deny cargo-outdated cargo-nextest cargo-llvm-cov

    # Recommended tools
    printf '\n{{cyan}}[INFO]{{reset}} Installing recommended tools...\n'
    {{cargo}} install cargo-watch cargo-semver-checks cargo-machete

    # Optional but useful tools
    printf '\n{{cyan}}[INFO]{{reset}} Installing optional tools...\n'
    {{cargo}} install cargo-expand cargo-bloat || true

    printf '\n{{green}}[OK]{{reset}}   Development tools installed\n'
    printf '{{cyan}}[INFO]{{reset}} Run "just check-tools" to verify installation\n'

[group('help')]
[doc("Install minimal tools for CI/release checks")]
install-tools-minimal:
    #!/usr/bin/env bash
    printf '\n{{bold}}Installing Minimal Development Tools{{reset}}\n'
    printf '═══════════════════════════════════════\n'
    {{cargo}} install cargo-audit cargo-deny cargo-semver-checks
    printf '\n{{green}}[OK]{{reset}}   Minimal tools installed\n'

[group('help')]
[doc("Show all available recipes grouped by category")]
help:
    #!/usr/bin/env bash
    printf '\n{{bold}}{{project_name}} v{{version}}{{reset}} — MCP SDK Development Command Runner\n'
    printf 'MSRV: {{msrv}} | Edition: {{edition}} | Platform: {{platform}}\n\n'
    printf '{{bold}}Usage:{{reset}} just [recipe] [arguments...]\n\n'
    just --list --unsorted
