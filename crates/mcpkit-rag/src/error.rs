//! Error types for RAG operations.

use thiserror::Error;

/// Result type for RAG operations.
pub type RagResult<T> = Result<T, RagError>;

/// Errors that can occur during RAG operations.
#[derive(Debug, Error)]
pub enum RagError {
    /// Error loading a document.
    #[error("Failed to load document: {message}")]
    LoadError {
        /// Error message.
        message: String,
    },

    /// Error reading a file.
    #[error("Failed to read file '{path}': {source}")]
    FileError {
        /// The file path.
        path: String,
        /// The underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Error splitting text.
    #[error("Failed to split text: {message}")]
    SplitError {
        /// Error message.
        message: String,
    },

    /// Error during retrieval.
    #[error("Retrieval failed: {message}")]
    RetrievalError {
        /// Error message.
        message: String,
    },

    /// Error from the provider.
    #[error("Provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),

    /// Error from the embedding store.
    #[error("Embedding error: {0}")]
    Embedding(#[from] mcpkit_embedding::EmbeddingError),

    /// Invalid configuration.
    #[error("Invalid configuration: {message}")]
    InvalidConfig {
        /// Error message.
        message: String,
    },

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// No documents found.
    #[error("No documents found")]
    NoDocuments,

    /// Custom error.
    #[error("{0}")]
    Custom(String),
}

impl RagError {
    /// Create a load error.
    pub fn load(message: impl Into<String>) -> Self {
        Self::LoadError {
            message: message.into(),
        }
    }

    /// Create a file error.
    pub fn file(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::FileError {
            path: path.into(),
            source,
        }
    }

    /// Create a split error.
    pub fn split(message: impl Into<String>) -> Self {
        Self::SplitError {
            message: message.into(),
        }
    }

    /// Create a retrieval error.
    pub fn retrieval(message: impl Into<String>) -> Self {
        Self::RetrievalError {
            message: message.into(),
        }
    }

    /// Create an invalid config error.
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::InvalidConfig {
            message: message.into(),
        }
    }

    /// Create a custom error.
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom(message.into())
    }
}
