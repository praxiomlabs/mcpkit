//! Extension-specific error types.

use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError};
use mcpkit_core::error::McpError;
use thiserror::Error;

/// Errors that can occur in the Actix MCP extension.
#[derive(Error, Debug)]
pub enum ExtensionError {
    /// Protocol version is not supported.
    #[error("Protocol version '{0}' is not supported")]
    UnsupportedVersion(String),

    /// Session not found.
    #[error("Session '{0}' not found")]
    SessionNotFound(String),

    /// Session has expired.
    #[error("Session '{0}' has expired")]
    SessionExpired(String),

    /// Invalid JSON-RPC message.
    #[error("Invalid JSON-RPC message: {0}")]
    InvalidMessage(String),

    /// Handler error from the MCP server.
    #[error("Handler error: {0}")]
    Handler(#[from] McpError),

    /// JSON serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ExtensionError {
    /// Get the HTTP status code for this error.
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::UnsupportedVersion(_) => StatusCode::BAD_REQUEST,
            Self::SessionNotFound(_) => StatusCode::NOT_FOUND,
            Self::SessionExpired(_) => StatusCode::GONE,
            Self::InvalidMessage(_) => StatusCode::BAD_REQUEST,
            Self::Handler(e) => match e {
                McpError::InvalidParams { .. } => StatusCode::BAD_REQUEST,
                McpError::MethodNotFound { .. } => StatusCode::NOT_FOUND,
                McpError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Serialization(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Create an error response body.
    #[must_use]
    pub fn error_body(&self) -> String {
        serde_json::json!({
            "error": {
                "code": self.status_code().as_u16(),
                "message": self.to_string()
            }
        })
        .to_string()
    }
}

impl ResponseError for ExtensionError {
    fn status_code(&self) -> StatusCode {
        Self::status_code(self)
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let body = self.error_body();
        HttpResponse::build(status)
            .content_type("application/json")
            .body(body)
    }
}
