//! Error types for vector storage operations.

use thiserror::Error;

/// Errors that can occur during vector storage operations.
#[derive(Debug, Error)]
pub enum EmbeddingError {
    /// The embedding dimensions don't match the store's configuration.
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected number of dimensions.
        expected: usize,
        /// Actual number of dimensions provided.
        actual: usize,
    },

    /// The requested item was not found.
    #[error("item not found: {id}")]
    NotFound {
        /// The ID that was not found.
        id: String,
    },

    /// Duplicate ID insertion attempted.
    #[error("duplicate id: {id}")]
    DuplicateId {
        /// The duplicate ID.
        id: String,
    },

    /// The store is empty and cannot perform the requested operation.
    #[error("store is empty")]
    EmptyStore,

    /// Invalid search parameters.
    #[error("invalid search parameter: {message}")]
    InvalidParameter {
        /// Description of the invalid parameter.
        message: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Provider error during embedding generation.
    #[error("provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),
}

/// Result type for embedding operations.
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;
