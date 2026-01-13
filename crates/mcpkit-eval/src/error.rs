//! Error types for evaluation operations.

use thiserror::Error;

/// Result type for evaluation operations.
pub type EvalResult<T> = Result<T, EvalError>;

/// Errors that can occur during evaluation.
#[derive(Debug, Error)]
pub enum EvalError {
    /// Error from the LLM provider.
    #[error("Provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),

    /// Error parsing metric output.
    #[error("Failed to parse metric output: {message}")]
    ParseError {
        /// Error message.
        message: String,
    },

    /// Invalid test case configuration.
    #[error("Invalid test case: {message}")]
    InvalidTestCase {
        /// Error message.
        message: String,
    },

    /// Metric evaluation failed.
    #[error("Metric evaluation failed: {message}")]
    MetricFailed {
        /// The metric that failed.
        metric: String,
        /// Error message.
        message: String,
    },

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Custom error.
    #[error("{0}")]
    Custom(String),
}

impl EvalError {
    /// Create a parse error.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
        }
    }

    /// Create an invalid test case error.
    pub fn invalid_test_case(message: impl Into<String>) -> Self {
        Self::InvalidTestCase {
            message: message.into(),
        }
    }

    /// Create a metric failed error.
    pub fn metric_failed(metric: impl Into<String>, message: impl Into<String>) -> Self {
        Self::MetricFailed {
            metric: metric.into(),
            message: message.into(),
        }
    }

    /// Create a custom error.
    pub fn custom(message: impl Into<String>) -> Self {
        Self::Custom(message.into())
    }
}
