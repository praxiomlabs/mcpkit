//! Error types for Warp MCP integration.

use thiserror::Error;
use warp::http::StatusCode;
use warp::reject::Reject;

/// Errors that can occur during MCP request handling.
#[derive(Debug, Error)]
pub enum WarpError {
    /// Invalid JSON-RPC message format.
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// Unsupported protocol version.
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(String),

    /// Session not found.
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    /// JSON serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Reject for WarpError {}

/// Convert a `WarpError` to an HTTP status code.
impl WarpError {
    /// Get the appropriate HTTP status code for this error.
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            WarpError::InvalidMessage(_) | WarpError::UnsupportedVersion(_) => {
                StatusCode::BAD_REQUEST
            }
            WarpError::SessionNotFound(_) => StatusCode::NOT_FOUND,
            WarpError::Serialization(_) | WarpError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_message_error() {
        let error = WarpError::InvalidMessage("bad json".to_string());
        assert_eq!(error.to_string(), "Invalid message: bad json");
        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_unsupported_version_error() {
        let error = WarpError::UnsupportedVersion("1.0.0".to_string());
        assert_eq!(error.to_string(), "Unsupported protocol version: 1.0.0");
        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_session_not_found_error() {
        let error = WarpError::SessionNotFound("abc-123".to_string());
        assert_eq!(error.to_string(), "Session not found: abc-123");
        assert_eq!(error.status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_serialization_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let error = WarpError::Serialization(json_err);
        assert!(error.to_string().starts_with("Serialization error:"));
        assert_eq!(error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_internal_error() {
        let error = WarpError::Internal("something went wrong".to_string());
        assert_eq!(error.to_string(), "Internal error: something went wrong");
        assert_eq!(error.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let error: WarpError = json_err.into();
        assert!(matches!(error, WarpError::Serialization(_)));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WarpError>();
    }

    #[test]
    fn test_error_debug_format() {
        let error = WarpError::InvalidMessage("test".to_string());
        let debug = format!("{:?}", error);
        assert!(debug.contains("InvalidMessage"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_error_implements_reject() {
        fn assert_reject<T: Reject>() {}
        assert_reject::<WarpError>();
    }
}
