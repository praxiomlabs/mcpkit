//! Error types for memory operations.

use thiserror::Error;

/// Errors that can occur during memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// Failed to serialize or deserialize a message.
    #[error("serialization error: {message}")]
    Serialization {
        /// The error message.
        message: String,
    },

    /// Provider error during summarization.
    #[error("provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),

    /// Token estimation error.
    #[error("token estimation error: {message}")]
    TokenEstimation {
        /// The error message.
        message: String,
    },

    /// Memory capacity exceeded.
    #[error("memory capacity exceeded: {message}")]
    CapacityExceeded {
        /// The error message.
        message: String,
    },

    /// Invalid configuration.
    #[error("invalid configuration: {message}")]
    InvalidConfiguration {
        /// The error message.
        message: String,
    },
}

/// Result type alias for memory operations.
pub type MemoryResult<T> = Result<T, MemoryError>;
