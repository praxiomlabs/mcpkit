//! Transport error types.

use mcpkit_core::error::{McpError, TransportContext, TransportDetails, TransportErrorKind};
use thiserror::Error;

/// Errors that can occur during transport operations.
#[derive(Error, Debug)]
pub enum TransportError {
    /// I/O error occurred.
    #[error("I/O error: {message}")]
    Io {
        /// Error message.
        message: String,
    },

    /// I/O error from `std::io::Error`.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Serialization error.
    #[error("Serialization error: {message}")]
    Serialization {
        /// Error message.
        message: String,
    },

    /// Deserialization error.
    #[error("Deserialization error: {message}")]
    Deserialization {
        /// Error message.
        message: String,
    },

    /// Connection error.
    #[error("Connection error: {message}")]
    Connection {
        /// Error message.
        message: String,
    },

    /// Connection was closed.
    #[error("Connection closed")]
    ConnectionClosed,

    /// Transport is not connected.
    #[error("Not connected")]
    NotConnected,

    /// Message was too large.
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge {
        /// Actual message size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Invalid message format.
    #[error("Invalid message: {message}")]
    InvalidMessage {
        /// Description of the problem.
        message: String,
    },

    /// Protocol error.
    #[error("Protocol error: {message}")]
    Protocol {
        /// Description of the protocol violation.
        message: String,
    },

    /// Timeout occurred.
    #[error("{operation} timed out after {duration:?}")]
    Timeout {
        /// The operation that timed out.
        operation: String,
        /// How long the operation waited.
        duration: std::time::Duration,
    },

    /// Transport was already closed.
    #[error("Transport already closed")]
    AlreadyClosed,

    /// Rate limit exceeded.
    #[error("Rate limit exceeded{}", retry_after.map(|d| format!(", retry after {d:?}")).unwrap_or_default())]
    RateLimited {
        /// Suggested retry delay.
        retry_after: Option<std::time::Duration>,
    },
}

impl TransportError {
    /// Create an invalid message error.
    pub fn invalid_message(message: impl Into<String>) -> Self {
        Self::InvalidMessage {
            message: message.into(),
        }
    }

    /// Create a protocol error.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    /// Get the transport error kind.
    #[must_use]
    pub fn kind(&self) -> TransportErrorKind {
        match self {
            Self::Io { .. } => TransportErrorKind::ReadFailed,
            Self::IoError(e) => match e.kind() {
                std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected => TransportErrorKind::ConnectionFailed,
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe => {
                    TransportErrorKind::ConnectionClosed
                }
                std::io::ErrorKind::TimedOut => TransportErrorKind::Timeout,
                std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::Interrupted
                | std::io::ErrorKind::UnexpectedEof => TransportErrorKind::ReadFailed,
                std::io::ErrorKind::WriteZero => TransportErrorKind::WriteFailed,
                _ => TransportErrorKind::ReadFailed,
            },
            Self::Json(_) => TransportErrorKind::InvalidMessage,
            Self::Serialization { .. } => TransportErrorKind::WriteFailed,
            Self::Deserialization { .. } => TransportErrorKind::InvalidMessage,
            Self::Connection { .. } => TransportErrorKind::ConnectionFailed,
            Self::ConnectionClosed | Self::AlreadyClosed => TransportErrorKind::ConnectionClosed,
            Self::NotConnected => TransportErrorKind::ConnectionFailed,
            Self::MessageTooLarge { .. } => TransportErrorKind::InvalidMessage,
            Self::InvalidMessage { .. } => TransportErrorKind::InvalidMessage,
            Self::Protocol { .. } => TransportErrorKind::ProtocolViolation,
            Self::Timeout { .. } => TransportErrorKind::Timeout,
            Self::RateLimited { .. } => TransportErrorKind::RateLimited,
        }
    }
}

impl From<TransportError> for McpError {
    fn from(err: TransportError) -> Self {
        Self::Transport(Box::new(TransportDetails {
            kind: err.kind(),
            message: err.to_string(),
            context: TransportContext::default(),
            source: Some(Box::new(err)),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_kinds() {
        assert_eq!(
            TransportError::ConnectionClosed.kind(),
            TransportErrorKind::ConnectionClosed
        );
        assert_eq!(
            TransportError::Timeout {
                operation: "test".to_string(),
                duration: std::time::Duration::from_secs(1),
            }
            .kind(),
            TransportErrorKind::Timeout
        );
        assert_eq!(
            TransportError::invalid_message("bad").kind(),
            TransportErrorKind::InvalidMessage
        );
    }

    #[test]
    fn test_mcp_error_conversion() {
        let err = TransportError::ConnectionClosed;
        let mcp_err: McpError = err.into();

        match mcp_err {
            McpError::Transport(details) => {
                assert_eq!(details.kind, TransportErrorKind::ConnectionClosed);
            }
            _ => panic!("Expected Transport error"),
        }
    }
}
