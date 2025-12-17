//! Context extension trait for error handling.
//!
//! This module provides `anyhow`-style context methods while
//! preserving the typed error system.

use super::types::McpError;

/// Extension trait for adding context to `Result` types.
///
/// This provides `anyhow`-style context methods while preserving the
/// typed error system.
///
/// # Example
///
/// ```rust
/// use mcpkit_core::error::{McpError, McpResultExt};
///
/// fn process() -> Result<(), McpError> {
///     let result: Result<(), McpError> = Err(McpError::internal("oops"));
///     result.context("Failed to process data")?;
///     Ok(())
/// }
/// ```
pub trait McpResultExt<T> {
    /// Add context to an error.
    fn context<C: Into<String>>(self, context: C) -> Result<T, McpError>;

    /// Add context lazily (only evaluated on error).
    fn with_context<C, F>(self, f: F) -> Result<T, McpError>
    where
        C: Into<String>,
        F: FnOnce() -> C;
}

impl<T> McpResultExt<T> for Result<T, McpError> {
    fn context<C: Into<String>>(self, context: C) -> Self {
        self.map_err(|e| McpError::WithContext {
            context: context.into(),
            source: Box::new(e),
        })
    }

    fn with_context<C, F>(self, f: F) -> Self
    where
        C: Into<String>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| McpError::WithContext {
            context: f().into(),
            source: Box::new(e),
        })
    }
}
