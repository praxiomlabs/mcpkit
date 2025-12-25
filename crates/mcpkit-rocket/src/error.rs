//! Error types for Rocket MCP integration.

use rocket::http::Status;
use rocket::response::{self, Responder};
use rocket::Request;
use thiserror::Error;

/// Errors that can occur during MCP request handling.
#[derive(Debug, Error)]
pub enum RocketError {
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

impl<'r> Responder<'r, 'static> for RocketError {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let status = match &self {
            RocketError::InvalidMessage(_) => Status::BadRequest,
            RocketError::UnsupportedVersion(_) => Status::BadRequest,
            RocketError::SessionNotFound(_) => Status::NotFound,
            RocketError::Serialization(_) => Status::InternalServerError,
            RocketError::Internal(_) => Status::InternalServerError,
        };

        Err(status)
    }
}

impl RocketError {
    /// Get the HTTP status code for this error.
    #[must_use]
    pub fn status(&self) -> Status {
        match self {
            RocketError::InvalidMessage(_) => Status::BadRequest,
            RocketError::UnsupportedVersion(_) => Status::BadRequest,
            RocketError::SessionNotFound(_) => Status::NotFound,
            RocketError::Serialization(_) => Status::InternalServerError,
            RocketError::Internal(_) => Status::InternalServerError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_message_error() {
        let error = RocketError::InvalidMessage("bad json".to_string());
        assert_eq!(error.to_string(), "Invalid message: bad json");
        assert_eq!(error.status(), Status::BadRequest);
    }

    #[test]
    fn test_unsupported_version_error() {
        let error = RocketError::UnsupportedVersion("1.0.0".to_string());
        assert_eq!(error.to_string(), "Unsupported protocol version: 1.0.0");
        assert_eq!(error.status(), Status::BadRequest);
    }

    #[test]
    fn test_session_not_found_error() {
        let error = RocketError::SessionNotFound("abc-123".to_string());
        assert_eq!(error.to_string(), "Session not found: abc-123");
        assert_eq!(error.status(), Status::NotFound);
    }

    #[test]
    fn test_serialization_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let error = RocketError::Serialization(json_err);
        assert!(error.to_string().starts_with("Serialization error:"));
        assert_eq!(error.status(), Status::InternalServerError);
    }

    #[test]
    fn test_internal_error() {
        let error = RocketError::Internal("something went wrong".to_string());
        assert_eq!(error.to_string(), "Internal error: something went wrong");
        assert_eq!(error.status(), Status::InternalServerError);
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let error: RocketError = json_err.into();
        assert!(matches!(error, RocketError::Serialization(_)));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RocketError>();
    }

    #[test]
    fn test_error_debug_format() {
        let error = RocketError::InvalidMessage("test".to_string());
        let debug = format!("{:?}", error);
        assert!(debug.contains("InvalidMessage"));
        assert!(debug.contains("test"));
    }
}
