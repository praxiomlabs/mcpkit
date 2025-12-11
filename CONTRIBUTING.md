# Contributing to mcpkit

Thank you for your interest in contributing to the Rust MCP SDK! This document provides guidelines and instructions for contributing.

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Git

### Setup

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/mcpkit.git
   cd mcpkit
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/praxislabs/mcpkit.git
   ```
4. Build the project:
   ```bash
   cargo build
   ```
5. Run tests:
   ```bash
   cargo test
   ```

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
```

### Writing Tests

- Place unit tests in the same file as the code
- Place integration tests in `tests/`
- Use `mcpkit-testing` utilities for complex tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // ...
    }

    #[tokio::test]
    async fn test_async_something() {
        // ...
    }
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

- **Questions**: Open a [Discussion](https://github.com/praxislabs/mcpkit/discussions)
- **Bugs**: Open an [Issue](https://github.com/praxislabs/mcpkit/issues)
- **Security**: See [SECURITY.md](SECURITY.md)

## Recognition

Contributors are recognized in:
- Git commit history
- Release notes
- AUTHORS file (for significant contributions)

Thank you for contributing!
