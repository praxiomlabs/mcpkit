//! Error types for LLM providers.
//!
//! This module defines the error types used across all provider implementations,
//! enabling consistent error handling regardless of which provider is being used.

use miette::Diagnostic;
use std::time::Duration;
use thiserror::Error;

/// Result type alias for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Errors that can occur when interacting with LLM providers.
#[derive(Debug, Error, Diagnostic)]
pub enum ProviderError {
    /// Authentication failed (invalid API key, expired token, etc.)
    #[error("Authentication failed: {message}")]
    #[diagnostic(code(provider::auth_failed))]
    AuthenticationFailed {
        /// The error message from the provider.
        message: String,
        /// The provider that reported the error.
        provider: String,
    },

    /// Rate limit exceeded.
    #[error("Rate limit exceeded for {provider}, retry after {retry_after:?}")]
    #[diagnostic(
        code(provider::rate_limited),
        help("Consider implementing exponential backoff or reducing request frequency")
    )]
    RateLimited {
        /// The provider that rate limited the request.
        provider: String,
        /// When the rate limit resets.
        retry_after: Option<Duration>,
    },

    /// The requested model is not available.
    #[error("Model '{model}' is not available from provider '{provider}'")]
    #[diagnostic(code(provider::model_not_found))]
    ModelNotFound {
        /// The requested model.
        model: String,
        /// The provider that was queried.
        provider: String,
    },

    /// The request was invalid.
    #[error("Invalid request: {message}")]
    #[diagnostic(code(provider::invalid_request))]
    InvalidRequest {
        /// The error message.
        message: String,
        /// Optional field that caused the error.
        field: Option<String>,
    },

    /// Content was filtered by the provider's safety systems.
    #[error("Content filtered: {reason}")]
    #[diagnostic(
        code(provider::content_filtered),
        help("Modify your prompt to avoid triggering content filters")
    )]
    ContentFiltered {
        /// The reason the content was filtered.
        reason: String,
    },

    /// The context length was exceeded.
    #[error("Context length exceeded: {message}")]
    #[diagnostic(
        code(provider::context_length_exceeded),
        help("Reduce the input length or use a model with larger context window")
    )]
    ContextLengthExceeded {
        /// The error message.
        message: String,
        /// The maximum allowed tokens.
        max_tokens: Option<u32>,
        /// The actual token count.
        actual_tokens: Option<u32>,
    },

    /// The provider returned an unexpected response.
    #[error("Unexpected response from {provider}: {message}")]
    #[diagnostic(code(provider::unexpected_response))]
    UnexpectedResponse {
        /// The provider that returned the unexpected response.
        provider: String,
        /// The error message.
        message: String,
    },

    /// Network error during API communication.
    #[error("Network error: {message}")]
    #[diagnostic(code(provider::network_error))]
    NetworkError {
        /// The error message.
        message: String,
        /// Whether the error is retryable.
        retryable: bool,
        /// The underlying error source.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Timeout waiting for response.
    #[error("Request timed out after {duration:?}")]
    #[diagnostic(
        code(provider::timeout),
        help("Consider increasing the timeout or reducing the request complexity")
    )]
    Timeout {
        /// The duration before timeout.
        duration: Duration,
    },

    /// The stream was interrupted.
    #[error("Stream interrupted: {message}")]
    #[diagnostic(code(provider::stream_interrupted))]
    StreamInterrupted {
        /// The error message.
        message: String,
    },

    /// Provider-specific error that doesn't fit other categories.
    #[error("Provider error ({provider}): {message}")]
    #[diagnostic(code(provider::other))]
    Other {
        /// The provider that reported the error.
        provider: String,
        /// The error message.
        message: String,
        /// Error code from the provider, if available.
        code: Option<String>,
    },

    /// Configuration error.
    #[error("Configuration error: {message}")]
    #[diagnostic(code(provider::configuration))]
    Configuration {
        /// The error message.
        message: String,
    },

    /// Serialization/deserialization error.
    #[error("Serialization error: {message}")]
    #[diagnostic(code(provider::serialization))]
    Serialization {
        /// The error message.
        message: String,
        /// The underlying error source.
        #[source]
        source: Option<serde_json::Error>,
    },

    /// The requested feature is not supported by this provider.
    #[error("Feature '{feature}' is not supported by provider '{provider}'")]
    #[diagnostic(
        code(provider::unsupported),
        help("Check the provider's capabilities or use a different provider")
    )]
    Unsupported {
        /// The provider that doesn't support the feature.
        provider: String,
        /// The feature that is not supported.
        feature: String,
    },
}

impl ProviderError {
    /// Create an authentication failed error.
    pub fn auth_failed(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::AuthenticationFailed {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Create a rate limited error.
    pub fn rate_limited(provider: impl Into<String>, retry_after: Option<Duration>) -> Self {
        Self::RateLimited {
            provider: provider.into(),
            retry_after,
        }
    }

    /// Create a model not found error.
    pub fn model_not_found(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self::ModelNotFound {
            provider: provider.into(),
            model: model.into(),
        }
    }

    /// Create an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
            field: None,
        }
    }

    /// Create an invalid request error with a field.
    pub fn invalid_request_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
            field: Some(field.into()),
        }
    }

    /// Create a network error.
    pub fn network(message: impl Into<String>, retryable: bool) -> Self {
        Self::NetworkError {
            message: message.into(),
            retryable,
            source: None,
        }
    }

    /// Create a timeout error.
    pub fn timeout(duration: Duration) -> Self {
        Self::Timeout { duration }
    }

    /// Check if this error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::RateLimited { .. } => true,
            Self::NetworkError { retryable, .. } => *retryable,
            Self::Timeout { .. } => true,
            Self::StreamInterrupted { .. } => true,
            _ => false,
        }
    }

    /// Get the retry delay if this is a rate limit error.
    #[must_use]
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

#[cfg(feature = "openai")]
impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        let retryable = err.is_timeout() || err.is_connect();
        Self::NetworkError {
            message: err.to_string(),
            retryable,
            source: Some(Box::new(err)),
        }
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization {
            message: err.to_string(),
            source: Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(ProviderError::rate_limited("test", None).is_retryable());
        assert!(ProviderError::timeout(Duration::from_secs(30)).is_retryable());
        assert!(ProviderError::network("connection failed", true).is_retryable());
        assert!(!ProviderError::network("connection failed", false).is_retryable());
        assert!(!ProviderError::auth_failed("test", "invalid key").is_retryable());
    }

    #[test]
    fn test_retry_after() {
        let err = ProviderError::rate_limited("test", Some(Duration::from_secs(60)));
        assert_eq!(err.retry_after(), Some(Duration::from_secs(60)));

        let err = ProviderError::auth_failed("test", "invalid");
        assert_eq!(err.retry_after(), None);
    }
}
