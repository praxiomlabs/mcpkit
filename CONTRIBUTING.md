# Contributing to mcpkit

Thank you for your interest in contributing to the Rust MCP SDK! This document provides guidelines and instructions for contributing.

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- Rust 1.85 or later (see [MSRV policy](docs/versioning.md#minimum-supported-rust-version))
- Git
- [Just](https://github.com/casey/just) command runner (recommended)

### Setup

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/mcpkit.git
   cd mcpkit
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/praxiomlabs/mcpkit.git
   ```
4. Install Just (if not already installed):
   ```bash
   cargo install just
   # or: brew install just / apt install just
   ```
5. Install development tools:
   ```bash
   just install-tools
   # or for minimal setup:
   just install-tools-minimal
   ```
6. Build the project:
   ```bash
   just build
   # or: cargo build
   ```
7. Run tests:
   ```bash
   just test
   # or: cargo test
   ```

### Development Tools

The following tools are used for development and CI:

| Tool | Purpose | Install |
|------|---------|---------|
| `cargo-audit` | Security vulnerability scanning | Required |
| `cargo-deny` | License and advisory checks | Required |
| `cargo-outdated` | Dependency freshness | Recommended |
| `cargo-nextest` | Faster test runner | Recommended |
| `cargo-llvm-cov` | Code coverage | Recommended |
| `cargo-semver-checks` | Semver compliance | Required for releases |
| `cargo-watch` | File watching | Optional |

Run `just check-tools` to see which tools are installed.

## Development Workflow

### Creating a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### Making Changes

1. Write your code
2. Add tests for new functionality
3. Ensure all tests pass: `cargo test`
4. Check formatting: `cargo fmt --check`
5. Run clippy: `cargo clippy --all-features`
6. Update documentation if needed

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Formatting, no code change
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding tests
- `chore`: Maintenance tasks

Examples:
```
feat(macros): add support for optional parameters in #[tool]
fix(transport): handle WebSocket reconnection timeout
docs(readme): update quick start example
```

### Pull Requests

1. Push your branch to your fork
2. Create a Pull Request against `main`
3. Fill in the PR template
4. Wait for CI to pass
5. Address review feedback

## Project Structure

```
mcpkit/
├── crates/
│   ├── mcpkit-core/        # Core types (runtime-agnostic)
│   ├── mcpkit-transport/   # Transport implementations
│   ├── mcpkit-server/      # Server implementation
│   ├── mcpkit-client/      # Client implementation
│   ├── mcpkit-macros/      # Procedural macros
│   ├── mcpkit-testing/     # Test utilities
│   ├── mcpkit-axum/        # Axum web framework integration
│   └── mcpkit-actix/       # Actix-web framework integration
├── mcpkit/                 # Facade crate
├── examples/            # Example servers
├── tests/               # Integration tests
├── benches/             # Benchmarks
└── docs/                # Documentation
```

## Contributing Extensions

We welcome contributions for framework integrations and extensions! See [`docs/extensions.md`](docs/extensions.md) for patterns and guidelines.

When contributing a new extension:

1. Follow the patterns established in `mcpkit-axum` and `mcpkit-actix`
2. Include session management with automatic cleanup
3. Support protocol version validation
4. Add comprehensive tests
5. Document all public APIs

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p mcpkit-core

# With output
cargo test -- --nocapture

# Integration tests only
cargo test --test '*'

# Run benchmarks
cargo bench --package mcpkit-benches
```

### Test Organization Conventions

We use a consistent test organization pattern across all crates:

**Inline Unit Tests** (`#[cfg(test)] mod tests`):
- Test individual functions and types in isolation
- Located in the same file as the code being tested
- Use for fast, focused tests of internal logic
- No external dependencies (mock everything)

**Integration Tests** (`mcpkit/tests/`):
- Test public API behavior across crate boundaries
- Located in the workspace `mcpkit/tests/` directory
- Use for end-to-end workflow testing
- May use real transports, file I/O, etc.

**Crate-Specific Integration Tests** (`crates/*/tests/`):
- Some crates have their own `tests/` directory for crate-specific integration tests
- Use when tests need access to crate internals not exposed publicly

**Benchmarks** (`benches/`):
- Performance benchmarks using Criterion
- Located in the dedicated `benches/` workspace member
- Run with `cargo bench`

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Unit test for internal function
    }

    #[tokio::test]
    async fn test_async_something() {
        // Async unit test
    }
}
```

### Test Utilities

Use `mcpkit-testing` utilities for complex tests:

```rust
use mcpkit_testing::{assert_tool_result, TestServer};

#[tokio::test]
async fn test_with_utilities() {
    let server = TestServer::new();
    // ...
}
```

## Documentation

### Code Documentation

- All public items must have doc comments
- Include examples in doc comments
- Use `#![warn(missing_docs)]`

```rust
/// Creates a new tool with the given name.
///
/// # Arguments
///
/// * `name` - The tool name (must be unique)
///
/// # Example
///
/// ```
/// let tool = Tool::new("my-tool");
/// ```
pub fn new(name: impl Into<String>) -> Self {
    // ...
}
```

### Updating Documentation

- Update `README.md` for user-facing changes
- Update `docs/` for detailed guides
- Update `CHANGELOG.md` for all notable changes

## Code Style

### Formatting

We use `rustfmt` with default settings:

```bash
cargo fmt
```

### Linting

We use strict clippy settings:

```bash
cargo clippy --all-features -- -D warnings
```

### Guidelines

- No `unsafe` code without justification
- Prefer `impl Trait` over `Box<dyn Trait>` where possible
- Use descriptive variable names
- Keep functions focused and small
- Add comments for complex logic

## Adding Dependencies

Before adding a new dependency:

1. Check if existing dependencies can solve the problem
2. Evaluate the dependency's maintenance status
3. Consider binary size impact
4. Ensure license compatibility (MIT/Apache-2.0)
5. Prefer feature-gated optional dependencies

## Versioning Policy

mcpkit follows [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** (x.0.0): Breaking changes to public API
- **MINOR** (0.x.0): New features, backward-compatible additions
- **PATCH** (0.0.x): Bug fixes, security patches, documentation

### Pre-1.0 Stability

During the 0.x.y phase:
- Minor version bumps (0.x.0) may include breaking changes
- Patch versions (0.0.x) remain backward compatible
- We aim to minimize churn, but cannot guarantee full API stability

### What Constitutes a Breaking Change

Breaking changes include:
- Removing public types, functions, methods, or fields
- Changing function signatures (parameters, return types)
- Changing trait definitions
- Changing macro syntax or behavior
- Removing or renaming feature flags
- Increasing MSRV (Minimum Supported Rust Version)

Non-breaking changes include:
- Adding new public items (types, functions, methods)
- Adding new optional parameters with defaults
- Adding new feature flags
- Deprecating items (but not removing them)
- Bug fixes that change incorrect behavior
- Performance improvements
- Documentation updates

### CI Enforcement

We use `cargo-semver-checks` in CI to automatically detect unintentional breaking changes. All PRs must pass semver checks before merging.

## Release Process

Releases are handled by maintainers:

1. Update `CHANGELOG.md`
2. Bump versions in `Cargo.toml`
3. Create a git tag
4. CI publishes to crates.io

## Getting Help

- **Questions & Bugs**: Open an [Issue](https://github.com/praxiomlabs/mcpkit/issues)
- **Security**: See [SECURITY.md](SECURITY.md)

## Recognition

Contributors are recognized in:
- Git commit history
- Release notes
- AUTHORS file (for significant contributions)

Thank you for contributing!
