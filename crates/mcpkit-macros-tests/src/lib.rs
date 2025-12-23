//! Integration tests for mcpkit-macros.
//!
//! This crate is not published to crates.io. It exists to break a circular
//! dev-dependency during publishing:
//!
//! - `mcpkit-macros` needs `mcpkit` for integration tests
//! - `mcpkit` depends on `mcpkit-macros`
//!
//! By having tests in an unpublished crate, we break this cycle during publishing
//! while still maintaining full test coverage.
