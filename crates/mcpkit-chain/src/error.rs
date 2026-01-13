//! Error types for chain operations.

use thiserror::Error;

/// Errors that can occur during chain execution.
#[derive(Debug, Error)]
pub enum ChainError {
    /// Provider error during LLM operation.
    #[error("provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),

    /// Type conversion error.
    #[error("type error: expected {expected}, got {actual}")]
    TypeError {
        /// Expected type name.
        expected: String,
        /// Actual type name.
        actual: String,
    },

    /// No matching branch in a conditional.
    #[error("no matching branch for input")]
    NoBranchMatch,

    /// Chain execution was cancelled.
    #[error("chain execution cancelled")]
    Cancelled,

    /// Timeout exceeded.
    #[error("timeout exceeded after {elapsed_ms}ms")]
    Timeout {
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
    },

    /// Retry limit exceeded.
    #[error("retry limit exceeded after {attempts} attempts: {last_error}")]
    RetryExhausted {
        /// Number of attempts made.
        attempts: u32,
        /// The last error encountered.
        last_error: String,
    },

    /// Invalid chain configuration.
    #[error("invalid configuration: {message}")]
    InvalidConfig {
        /// Description of the configuration error.
        message: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Custom error from user-defined runnable.
    #[error("{0}")]
    Custom(String),
}

impl ChainError {
    /// Create a custom error.
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Custom(msg.into())
    }

    /// Create a type error.
    pub fn type_error(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::TypeError {
            expected: expected.into(),
            actual: actual.into(),
        }
    }
}

/// Result type for chain operations.
pub type ChainResult<T> = Result<T, ChainError>;
