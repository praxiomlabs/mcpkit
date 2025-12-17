//! The primary error type for the MCP SDK.
//!
//! This module contains the unified `McpError` enum that replaces
//! nested error hierarchies with a single, context-rich type.

use miette::Diagnostic;
use thiserror::Error;

use super::codes;
use super::details::{
    BoxError, HandshakeDetails, InvalidParamsDetails, ToolExecutionDetails, TransportDetails,
};
use super::transport::{TransportContext, TransportErrorKind};

/// The primary error type for the MCP SDK.
///
/// This unified error type replaces nested error hierarchies with a single,
/// context-rich type that preserves error chains and provides excellent
/// diagnostic output.
///
/// Large error variants are boxed to keep `Result<T, McpError>` small
/// (approximately 24 bytes on 64-bit systems).
#[derive(Error, Diagnostic, Debug)]
#[allow(clippy::large_enum_variant)] // We've intentionally boxed large variants
pub enum McpError {
    // ========================================================================
    // JSON-RPC Protocol Errors (-32700 to -32600)
    // ========================================================================
    /// Invalid JSON was received by the server.
    #[error("Parse error: {message}")]
    #[diagnostic(
        code(mcp::protocol::parse_error),
        help("Ensure the message is valid JSON-RPC 2.0 format")
    )]
    Parse {
        /// Human-readable error message.
        message: String,
        /// The underlying parse error, if available.
        #[source]
        source: Option<BoxError>,
    },

    /// The JSON sent is not a valid Request object.
    #[error("Invalid request: {message}")]
    #[diagnostic(code(mcp::protocol::invalid_request))]
    InvalidRequest {
        /// Human-readable error message.
        message: String,
        /// The underlying error, if available.
        #[source]
        source: Option<BoxError>,
    },

    /// The method does not exist or is not available.
    #[error("Method not found: {method}")]
    #[diagnostic(code(mcp::protocol::method_not_found))]
    MethodNotFound {
        /// The method that was requested.
        method: String,
        /// List of available methods for suggestions (boxed to reduce size).
        available: Box<[String]>,
    },

    /// Invalid method parameter(s) (details boxed to reduce enum size).
    #[error("Invalid params for '{}': {}", .0.method, .0.message)]
    #[diagnostic(code(mcp::protocol::invalid_params))]
    InvalidParams(#[source] Box<InvalidParamsDetails>),

    /// Internal JSON-RPC error.
    #[error("Internal error: {message}")]
    #[diagnostic(code(mcp::protocol::internal_error), severity(error))]
    Internal {
        /// Human-readable error message.
        message: String,
        /// The underlying error, if available.
        #[source]
        source: Option<BoxError>,
    },

    // ========================================================================
    // Transport Errors
    // ========================================================================
    /// Transport-level error (details boxed to reduce enum size).
    #[error("Transport error ({}): {}", .0.kind, .0.message)]
    #[diagnostic(code(mcp::transport::error))]
    Transport(#[source] Box<TransportDetails>),

    // ========================================================================
    // Tool Execution Errors
    // ========================================================================
    /// A tool execution failed (details boxed to reduce enum size).
    #[error("Tool '{}' failed: {}", .0.tool, .0.message)]
    #[diagnostic(code(mcp::tool::execution_error))]
    ToolExecution(#[source] Box<ToolExecutionDetails>),

    // ========================================================================
    // Resource Errors
    // ========================================================================
    /// A requested resource was not found.
    #[error("Resource not found: {uri}")]
    #[diagnostic(
        code(mcp::resource::not_found),
        help("Verify the URI is correct and the resource exists")
    )]
    ResourceNotFound {
        /// The URI of the resource that was not found.
        uri: String,
    },

    /// Access to a resource was denied.
    #[error("Resource access denied: {uri}")]
    #[diagnostic(code(mcp::resource::access_denied))]
    ResourceAccessDenied {
        /// The URI of the resource.
        uri: String,
        /// The reason for denial, if available.
        reason: Option<String>,
    },

    // ========================================================================
    // Connection/Session Errors
    // ========================================================================
    /// Connection establishment failed.
    #[error("Connection failed: {message}")]
    #[diagnostic(code(mcp::connection::failed))]
    ConnectionFailed {
        /// Human-readable error message.
        message: String,
        /// The underlying error, if available.
        #[source]
        source: Option<BoxError>,
    },

    /// Session has expired.
    #[error("Session expired: {session_id}")]
    #[diagnostic(
        code(mcp::session::expired),
        help("Re-initialize the connection to continue")
    )]
    SessionExpired {
        /// The expired session ID.
        session_id: String,
    },

    /// Protocol handshake failed (details boxed to reduce enum size).
    #[error("Handshake failed: {}", .0.message)]
    #[diagnostic(code(mcp::handshake::failed))]
    HandshakeFailed(#[source] Box<HandshakeDetails>),

    // ========================================================================
    // Capability Errors
    // ========================================================================
    /// A requested capability is not supported.
    #[error("Capability not supported: {capability}")]
    #[diagnostic(code(mcp::capability::not_supported))]
    CapabilityNotSupported {
        /// The capability that was requested.
        capability: String,
        /// List of available capabilities (boxed to reduce size).
        available: Box<[String]>,
    },

    // ========================================================================
    // User/Client Errors
    // ========================================================================
    /// User rejected an operation.
    #[error("User rejected: {message}")]
    #[diagnostic(code(mcp::user::rejected))]
    UserRejected {
        /// Human-readable message about what was rejected.
        message: String,
        /// The operation that was rejected.
        operation: String,
    },

    // ========================================================================
    // Timeout Errors
    // ========================================================================
    /// An operation timed out.
    #[error("Timeout after {duration:?}: {operation}")]
    #[diagnostic(
        code(mcp::timeout),
        help("Consider increasing the timeout or checking connectivity")
    )]
    Timeout {
        /// The operation that timed out.
        operation: String,
        /// How long we waited before timing out.
        duration: std::time::Duration,
    },

    // ========================================================================
    // Cancellation
    // ========================================================================
    /// An operation was cancelled.
    #[error("Operation cancelled: {operation}")]
    #[diagnostic(code(mcp::cancelled))]
    Cancelled {
        /// The operation that was cancelled.
        operation: String,
        /// Reason for cancellation, if provided.
        reason: Option<String>,
    },

    // ========================================================================
    // Context-Wrapped Errors
    // ========================================================================
    /// An error with additional context.
    #[error("{context}: {source}")]
    #[diagnostic(code(mcp::context))]
    WithContext {
        /// The context message.
        context: String,
        /// The underlying error.
        #[source]
        source: Box<McpError>,
    },

    // ========================================================================
    // Generic Internal Error (simple variant)
    // ========================================================================
    /// A simple internal error with just a message.
    #[error("Internal error: {message}")]
    #[diagnostic(code(mcp::internal))]
    InternalMessage {
        /// Human-readable error message.
        message: String,
    },
}

// ============================================================================
// Error Construction Helpers
// ============================================================================

impl McpError {
    /// Create a parse error.
    pub fn parse(message: impl Into<String>) -> Self {
        Self::Parse {
            message: message.into(),
            source: None,
        }
    }

    /// Create a parse error with a source.
    pub fn parse_with_source<E: std::error::Error + Send + Sync + 'static>(
        message: impl Into<String>,
        source: E,
    ) -> Self {
        Self::Parse {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
            source: None,
        }
    }

    /// Create a method not found error.
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Self::MethodNotFound {
            method: method.into(),
            available: Box::new([]),
        }
    }

    /// Create a method not found error with suggestions.
    pub fn method_not_found_with_suggestions(
        method: impl Into<String>,
        available: Vec<String>,
    ) -> Self {
        Self::MethodNotFound {
            method: method.into(),
            available: available.into_boxed_slice(),
        }
    }

    /// Create an invalid params error.
    pub fn invalid_params(method: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidParams(Box::new(InvalidParamsDetails {
            method: method.into(),
            message: message.into(),
            param_path: None,
            expected: None,
            actual: None,
            source: None,
        }))
    }

    /// Create an invalid params error with full details.
    pub fn invalid_params_detailed(
        method: impl Into<String>,
        message: impl Into<String>,
        param_path: Option<String>,
        expected: Option<String>,
        actual: Option<String>,
    ) -> Self {
        Self::InvalidParams(Box::new(InvalidParamsDetails {
            method: method.into(),
            message: message.into(),
            param_path,
            expected,
            actual,
            source: None,
        }))
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
            source: None,
        }
    }

    /// Create an internal error with a source.
    pub fn internal_with_source<E: std::error::Error + Send + Sync + 'static>(
        message: impl Into<String>,
        source: E,
    ) -> Self {
        Self::Internal {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a transport error.
    pub fn transport(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self::Transport(Box::new(TransportDetails {
            kind,
            message: message.into(),
            context: TransportContext::default(),
            source: None,
        }))
    }

    /// Create a transport error with context.
    pub fn transport_with_context(
        kind: TransportErrorKind,
        message: impl Into<String>,
        context: TransportContext,
    ) -> Self {
        Self::Transport(Box::new(TransportDetails {
            kind,
            message: message.into(),
            context,
            source: None,
        }))
    }

    /// Create a tool execution error.
    pub fn tool_error(tool: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolExecution(Box::new(ToolExecutionDetails {
            tool: tool.into(),
            message: message.into(),
            is_recoverable: true,
            data: None,
            source: None,
        }))
    }

    /// Create a tool execution error with full details.
    pub fn tool_error_detailed(
        tool: impl Into<String>,
        message: impl Into<String>,
        is_recoverable: bool,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self::ToolExecution(Box::new(ToolExecutionDetails {
            tool: tool.into(),
            message: message.into(),
            is_recoverable,
            data,
            source: None,
        }))
    }

    /// Create a resource not found error.
    pub fn resource_not_found(uri: impl Into<String>) -> Self {
        Self::ResourceNotFound { uri: uri.into() }
    }

    /// Create a handshake failed error.
    pub fn handshake_failed(message: impl Into<String>) -> Self {
        Self::HandshakeFailed(Box::new(HandshakeDetails {
            message: message.into(),
            client_version: None,
            server_version: None,
            source: None,
        }))
    }

    /// Create a handshake failed error with version info.
    pub fn handshake_failed_with_versions(
        message: impl Into<String>,
        client_version: Option<String>,
        server_version: Option<String>,
    ) -> Self {
        Self::HandshakeFailed(Box::new(HandshakeDetails {
            message: message.into(),
            client_version,
            server_version,
            source: None,
        }))
    }

    /// Create a capability not supported error.
    pub fn capability_not_supported(capability: impl Into<String>) -> Self {
        Self::CapabilityNotSupported {
            capability: capability.into(),
            available: Box::new([]),
        }
    }

    /// Create a capability not supported error with available list.
    pub fn capability_not_supported_with_available(
        capability: impl Into<String>,
        available: Vec<String>,
    ) -> Self {
        Self::CapabilityNotSupported {
            capability: capability.into(),
            available: available.into_boxed_slice(),
        }
    }

    /// Create a timeout error.
    pub fn timeout(operation: impl Into<String>, duration: std::time::Duration) -> Self {
        Self::Timeout {
            operation: operation.into(),
            duration,
        }
    }

    /// Create a cancelled error.
    pub fn cancelled(operation: impl Into<String>) -> Self {
        Self::Cancelled {
            operation: operation.into(),
            reason: None,
        }
    }

    /// Create a cancelled error with reason.
    pub fn cancelled_with_reason(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Cancelled {
            operation: operation.into(),
            reason: Some(reason.into()),
        }
    }

    /// Get the JSON-RPC error code for this error.
    #[must_use]
    pub fn code(&self) -> i32 {
        match self {
            Self::Parse { .. } => codes::PARSE_ERROR,
            Self::InvalidRequest { .. } => codes::INVALID_REQUEST,
            Self::MethodNotFound { .. } => codes::METHOD_NOT_FOUND,
            Self::InvalidParams(_) => codes::INVALID_PARAMS,
            Self::Internal { .. } => codes::INTERNAL_ERROR,
            Self::Transport(_) => codes::SERVER_ERROR_START,
            Self::ToolExecution(_) => codes::SERVER_ERROR_START - 1,
            Self::ResourceNotFound { .. } => codes::RESOURCE_NOT_FOUND,
            Self::ResourceAccessDenied { .. } => codes::SERVER_ERROR_START - 2,
            Self::ConnectionFailed { .. } => codes::SERVER_ERROR_START - 3,
            Self::SessionExpired { .. } => codes::SERVER_ERROR_START - 4,
            Self::HandshakeFailed(_) => codes::SERVER_ERROR_START - 5,
            Self::CapabilityNotSupported { .. } => codes::SERVER_ERROR_START - 6,
            Self::UserRejected { .. } => codes::USER_REJECTED,
            Self::Timeout { .. } => codes::SERVER_ERROR_START - 7,
            Self::Cancelled { .. } => codes::SERVER_ERROR_START - 8,
            Self::WithContext { source, .. } => source.code(),
            Self::InternalMessage { .. } => codes::INTERNAL_ERROR,
        }
    }

    /// Check if this is a recoverable error (LLM can retry).
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ToolExecution(details) => details.is_recoverable,
            Self::InvalidParams(_) => true,
            Self::ResourceNotFound { .. } => true,
            Self::Timeout { .. } => true,
            Self::WithContext { source, .. } => source.is_recoverable(),
            Self::InternalMessage { .. } => false,
            _ => false,
        }
    }
}

// ============================================================================
// Standard Error Conversions
// ============================================================================

impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        Self::parse_with_source("JSON serialization/deserialization error", err)
    }
}

impl From<std::io::Error> for McpError {
    fn from(err: std::io::Error) -> Self {
        let kind = match err.kind() {
            std::io::ErrorKind::NotFound => TransportErrorKind::ConnectionFailed,
            std::io::ErrorKind::ConnectionRefused => TransportErrorKind::ConnectionFailed,
            std::io::ErrorKind::ConnectionReset => TransportErrorKind::ConnectionClosed,
            std::io::ErrorKind::ConnectionAborted => TransportErrorKind::ConnectionClosed,
            std::io::ErrorKind::TimedOut => TransportErrorKind::Timeout,
            std::io::ErrorKind::WriteZero => TransportErrorKind::WriteFailed,
            std::io::ErrorKind::UnexpectedEof => TransportErrorKind::ReadFailed,
            _ => TransportErrorKind::ReadFailed,
        };
        let message = err.to_string();
        Self::Transport(Box::new(TransportDetails {
            kind,
            message,
            context: TransportContext::default(),
            source: Some(Box::new(err)),
        }))
    }
}
