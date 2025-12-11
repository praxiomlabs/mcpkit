//! Error types for the MCP Actix extension.

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use std::fmt;

/// Errors that can occur in the MCP Actix extension.
#[derive(Debug)]
pub enum ExtensionError {
    /// Protocol version not supported.
    UnsupportedVersion(String),
    /// Invalid JSON-RPC message.
    InvalidMessage(String),
    /// Session not found.
    SessionNotFound(String),
    /// JSON serialization error.
    Serialization(serde_json::Error),
    /// Internal server error.
    Internal(String),
}

impl fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "Unsupported protocol version: {v}"),
            Self::InvalidMessage(msg) => write!(f, "Invalid message: {msg}"),
            Self::SessionNotFound(id) => write!(f, "Session not found: {id}"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for ExtensionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Serialization(e) => Some(e),
            _ => None,
        }
    }
}

impl ResponseError for ExtensionError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::UnsupportedVersion(_) => StatusCode::BAD_REQUEST,
            Self::InvalidMessage(_) => StatusCode::BAD_REQUEST,
            Self::SessionNotFound(_) => StatusCode::NOT_FOUND,
            Self::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let body = serde_json::json!({
            "error": {
                "code": status.as_u16(),
                "message": self.to_string(),
            }
        });
        HttpResponse::build(status)
            .content_type("application/json")
            .body(body.to_string())
    }
}
