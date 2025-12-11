//! Unified error handling for the MCP SDK.
//!
//! This module provides a single, context-rich error type that replaces
//! the nested error hierarchies found in other implementations.
//!
//! # Design Philosophy
//!
//! - **Single error type**: All errors flow through [`McpError`]
//! - **Rich context**: Errors preserve context through the entire call stack
//! - **JSON-RPC compatible**: Easy conversion to JSON-RPC error responses
//! - **Diagnostic-friendly**: Integrates with [`miette`] for beautiful error reports
//! - **Size-optimized**: Large error variants are boxed to keep `Result<T, McpError>` small
//!
//! # Error Handling Patterns
//!
//! The SDK has two distinct error handling patterns for different scenarios:
//!
//! ## Pattern 1: `Result<T, McpError>` - For SDK/Framework Errors
//!
//! Use `Result<T, McpError>` for errors that indicate something went wrong
//! with the MCP protocol, transport, or SDK internals:
//!
//! - **Transport failures**: Connection lost, timeout, I/O errors
//! - **Protocol errors**: Invalid JSON-RPC, version mismatch, missing fields
//! - **Resource not found**: Requested resource/tool/prompt doesn't exist
//! - **Capability errors**: Feature not supported by client/server
//! - **Internal errors**: Unexpected SDK state, serialization failures
//!
//! These errors typically indicate the request cannot be completed and
//! require intervention (reconnection, configuration change, bug fix).
//!
//! ```rust,no_run
//! # use mcpkit_core::error::McpError;
//! # struct Tool;
//! # struct ListResult { tools: Vec<Tool> }
//! # struct Client;
//! # impl Client {
//! #     fn has_tools(&self) -> bool { true }
//! #     fn ensure_capability(&self, _: &str, _: bool) -> Result<(), McpError> { Ok(()) }
//! #     async fn request(&self, _: &str, _: Option<()>) -> Result<ListResult, McpError> { Ok(ListResult { tools: vec![] }) }
//! async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
//!     self.ensure_capability("tools", self.has_tools())?;
//!     // Transport errors propagate as McpError
//!     let result = self.request("tools/list", None).await?;
//!     Ok(result.tools)
//! }
//! # }
//! ```
//!
//! ## Pattern 2: `ToolOutput::RecoverableError` - For User/LLM-Correctable Errors
//!
//! Use [`ToolOutput::error()`](crate::types::ToolOutput::error) for errors that the LLM
//! can potentially self-correct by adjusting its input:
//!
//! - **Validation failures**: Invalid argument format, out-of-range values
//! - **Business logic errors**: Division by zero, empty query, invalid date
//! - **Missing optional data**: Lookup returned no results
//! - **Rate limiting**: Too many requests (suggest retry)
//!
//! These errors are returned to the LLM with `is_error: true` in the response,
//! allowing the model to understand what went wrong and try again.
//!
//! ```rust,no_run
//! # use mcpkit_core::types::ToolOutput;
//! # struct Calculator;
//! # impl Calculator {
//! // #[tool(description = "Divide two numbers")]
//! async fn divide(&self, a: f64, b: f64) -> ToolOutput {
//!     if b == 0.0 {
//!         return ToolOutput::error_with_suggestion(
//!             "Cannot divide by zero",
//!             "Use a non-zero divisor",
//!         );
//!     }
//!     ToolOutput::text((a / b).to_string())
//! }
//! # }
//! ```
//!
//! ## Decision Guide
//!
//! | Scenario | Use | Reason |
//! |----------|-----|--------|
//! | Database connection failed | `McpError` | Infrastructure issue |
//! | User provided invalid email format | `ToolOutput::error` | LLM can fix input |
//! | Tool doesn't exist | `McpError` | Protocol/discovery issue |
//! | Search returned no results | `ToolOutput::text("No results")` | Expected outcome |
//! | API rate limit exceeded | `ToolOutput::error_with_suggestion` | Temporary, can retry |
//! | Authentication required | `McpError` | Configuration issue |
//! | Invalid number format in input | `ToolOutput::error` | LLM can fix input |
//!
//! ## Context Chaining
//!
//! For `McpError`, use context chaining to provide detailed diagnostics:
//!
//! ```rust
//! use mcpkit_core::error::{McpError, McpResultExt};
//!
//! fn fetch_data() -> Result<String, McpError> {
//!     let user_id = 42;
//!     // Errors automatically get context
//!     let result: Result<(), McpError> = Err(McpError::resource_not_found("user://42"));
//!     result
//!         .context("Failed to fetch user data")
//!         .with_context(|| format!("User ID: {}", user_id))?;
//!     Ok("data".to_string())
//! }
//! ```

use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Type alias for boxed errors that are Send + Sync.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

// ============================================================================
// Boxed Error Detail Types (to reduce McpError enum size)
// ============================================================================

/// Details for invalid params errors (boxed to reduce enum size).
#[derive(Debug)]
pub struct InvalidParamsDetails {
    /// The method that received invalid parameters.
    pub method: String,
    /// Human-readable error message.
    pub message: String,
    /// The parameter path that failed (e.g., "arguments.query").
    pub param_path: Option<String>,
    /// The expected type or format.
    pub expected: Option<String>,
    /// The actual value received (truncated/redacted if needed).
    pub actual: Option<String>,
    /// The underlying error, if available.
    pub source: Option<BoxError>,
}

impl fmt::Display for InvalidParamsDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid params for '{}': {}", self.method, self.message)
    }
}

impl std::error::Error for InvalidParamsDetails {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Details for transport errors (boxed to reduce enum size).
#[derive(Debug)]
pub struct TransportDetails {
    /// Classification of the transport error.
    pub kind: TransportErrorKind,
    /// Human-readable error message.
    pub message: String,
    /// Transport-specific context for debugging.
    pub context: TransportContext,
    /// The underlying error, if available.
    pub source: Option<BoxError>,
}

impl fmt::Display for TransportDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transport error ({}): {}", self.kind, self.message)
    }
}

impl std::error::Error for TransportDetails {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Details for tool execution errors (boxed to reduce enum size).
#[derive(Debug)]
pub struct ToolExecutionDetails {
    /// The name of the tool that failed.
    pub tool: String,
    /// Human-readable error message.
    pub message: String,
    /// Whether the LLM should see this error for self-correction.
    pub is_recoverable: bool,
    /// Additional structured error data.
    pub data: Option<serde_json::Value>,
    /// The underlying error, if available.
    pub source: Option<BoxError>,
}

impl fmt::Display for ToolExecutionDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tool '{}' failed: {}", self.tool, self.message)
    }
}

impl std::error::Error for ToolExecutionDetails {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Details for handshake errors (boxed to reduce enum size).
#[derive(Debug)]
pub struct HandshakeDetails {
    /// Human-readable error message.
    pub message: String,
    /// Client protocol version, if available.
    pub client_version: Option<String>,
    /// Server protocol version, if available.
    pub server_version: Option<String>,
    /// The underlying error, if available.
    pub source: Option<BoxError>,
}

impl fmt::Display for HandshakeDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handshake failed: {}", self.message)
    }
}

impl std::error::Error for HandshakeDetails {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

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

/// Classification of transport errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportErrorKind {
    /// Connection could not be established.
    ConnectionFailed,
    /// Connection was closed unexpectedly.
    ConnectionClosed,
    /// Read operation failed.
    ReadFailed,
    /// Write operation failed.
    WriteFailed,
    /// TLS/SSL error occurred.
    TlsError,
    /// DNS resolution failed.
    DnsResolutionFailed,
    /// Operation timed out.
    Timeout,
    /// Message format was invalid.
    InvalidMessage,
    /// Protocol violation detected.
    ProtocolViolation,
    /// Resources exhausted (e.g., too many connections).
    ResourceExhausted,
    /// Rate limit exceeded.
    RateLimited,
}

impl fmt::Display for TransportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectionFailed => write!(f, "connection failed"),
            Self::ConnectionClosed => write!(f, "connection closed"),
            Self::ReadFailed => write!(f, "read failed"),
            Self::WriteFailed => write!(f, "write failed"),
            Self::TlsError => write!(f, "TLS error"),
            Self::DnsResolutionFailed => write!(f, "DNS resolution failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::InvalidMessage => write!(f, "invalid message"),
            Self::ProtocolViolation => write!(f, "protocol violation"),
            Self::ResourceExhausted => write!(f, "resource exhausted"),
            Self::RateLimited => write!(f, "rate limited"),
        }
    }
}

/// Additional context for transport errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransportContext {
    /// Transport type (stdio, http, websocket, unix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    /// Remote endpoint address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    /// Local endpoint address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_addr: Option<String>,
    /// Bytes sent before error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    /// Bytes received before error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_received: Option<u64>,
    /// Connection duration before error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_duration_ms: Option<u64>,
}

impl TransportContext {
    /// Create a new transport context for a specific transport type.
    #[must_use]
    pub fn new(transport_type: impl Into<String>) -> Self {
        Self {
            transport_type: Some(transport_type.into()),
            ..Default::default()
        }
    }

    /// Set the remote address.
    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    /// Set the local address.
    #[must_use]
    pub fn with_local_addr(mut self, addr: impl Into<String>) -> Self {
        self.local_addr = Some(addr.into());
        self
    }
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
}

// ============================================================================
// JSON-RPC Error Codes
// ============================================================================

/// Standard JSON-RPC error codes.
pub mod codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameters.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// Server error range start.
    pub const SERVER_ERROR_START: i32 = -32000;
    /// Server error range end.
    pub const SERVER_ERROR_END: i32 = -32099;

    // MCP-specific codes
    /// User rejected the operation.
    pub const USER_REJECTED: i32 = -1;
    /// Resource was not found.
    pub const RESOURCE_NOT_FOUND: i32 = -32002;
}

impl McpError {
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
// JSON-RPC Error Response Type
// ============================================================================

/// A JSON-RPC error response object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Create an "invalid params" error (-32602).
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    /// Create an "internal error" (-32603).
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }

    /// Create a "method not found" error (-32601).
    pub fn method_not_found(message: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: message.into(),
            data: None,
        }
    }

    /// Create a "parse error" (-32700).
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: message.into(),
            data: None,
        }
    }

    /// Create an "invalid request" error (-32600).
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
            data: None,
        }
    }
}

impl From<&McpError> for JsonRpcError {
    fn from(err: &McpError) -> Self {
        let code = err.code();
        let message = err.to_string();
        let data = match err {
            McpError::MethodNotFound { method, available, .. } => Some(serde_json::json!({
                "method": method,
                "available": available,
            })),
            McpError::InvalidParams(details) => Some(serde_json::json!({
                "method": details.method,
                "param_path": details.param_path,
                "expected": details.expected,
                "actual": details.actual,
            })),
            McpError::Transport(details) => Some(serde_json::json!({
                "kind": format!("{:?}", details.kind),
                "context": details.context,
            })),
            McpError::ToolExecution(details) => {
                details.data.clone().or_else(|| Some(serde_json::json!({ "tool": details.tool })))
            }
            McpError::HandshakeFailed(details) => Some(serde_json::json!({
                "client_version": details.client_version,
                "server_version": details.server_version,
            })),
            McpError::WithContext { source, .. } => {
                let inner: JsonRpcError = source.as_ref().into();
                inner.data
            }
            _ => None,
        };

        Self {
            code,
            message,
            data,
        }
    }
}

impl From<McpError> for JsonRpcError {
    fn from(err: McpError) -> Self {
        Self::from(&err)
    }
}

// ============================================================================
// Context Extension Trait
// ============================================================================

/// Extension trait for adding context to `Result` types.
///
/// This provides `anyhow`-style context methods while preserving the
/// typed error system.
///
/// # Example
///
/// ```rust
/// use mcpkit_core::error::{McpError, McpResultExt};
///
/// fn process() -> Result<(), McpError> {
///     let result: Result<(), McpError> = Err(McpError::internal("oops"));
///     result.context("Failed to process data")?;
///     Ok(())
/// }
/// ```
pub trait McpResultExt<T> {
    /// Add context to an error.
    fn context<C: Into<String>>(self, context: C) -> Result<T, McpError>;

    /// Add context lazily (only evaluated on error).
    fn with_context<C, F>(self, f: F) -> Result<T, McpError>
    where
        C: Into<String>,
        F: FnOnce() -> C;
}

impl<T> McpResultExt<T> for Result<T, McpError> {
    fn context<C: Into<String>>(self, context: C) -> Result<T, McpError> {
        self.map_err(|e| McpError::WithContext {
            context: context.into(),
            source: Box::new(e),
        })
    }

    fn with_context<C, F>(self, f: F) -> Result<T, McpError>
    where
        C: Into<String>,
        F: FnOnce() -> C,
    {
        self.map_err(|e| McpError::WithContext {
            context: f().into(),
            source: Box::new(e),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_size_is_small() {
        // Verify that McpError is reasonably small (64 bytes or less on 64-bit).
        // This ensures Result<T, McpError> doesn't bloat return values.
        let size = std::mem::size_of::<McpError>();
        assert!(
            size <= 64,
            "McpError is {} bytes, should be <= 64 bytes. Consider boxing more variants.",
            size
        );

        // Also verify Result<(), McpError> is small
        let result_size = std::mem::size_of::<Result<(), McpError>>();
        assert!(
            result_size <= 72,
            "Result<(), McpError> is {} bytes, should be <= 72 bytes.",
            result_size
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(McpError::parse("test").code(), codes::PARSE_ERROR);
        assert_eq!(
            McpError::invalid_request("test").code(),
            codes::INVALID_REQUEST
        );
        assert_eq!(
            McpError::method_not_found("test").code(),
            codes::METHOD_NOT_FOUND
        );
        assert_eq!(
            McpError::invalid_params("m", "test").code(),
            codes::INVALID_PARAMS
        );
        assert_eq!(McpError::internal("test").code(), codes::INTERNAL_ERROR);
        assert_eq!(
            McpError::transport(TransportErrorKind::ConnectionFailed, "test").code(),
            codes::SERVER_ERROR_START
        );
        assert_eq!(
            McpError::tool_error("tool", "test").code(),
            codes::SERVER_ERROR_START - 1
        );
        assert_eq!(
            McpError::handshake_failed("test").code(),
            codes::SERVER_ERROR_START - 5
        );
    }

    #[test]
    fn test_context_chaining() {
        fn inner() -> Result<(), McpError> {
            Err(McpError::resource_not_found("test://resource"))
        }

        fn outer() -> Result<(), McpError> {
            inner().context("Failed in outer")?;
            Ok(())
        }

        let err = outer().unwrap_err();
        assert!(err.to_string().contains("Failed in outer"));

        // Verify code propagates through context
        assert_eq!(err.code(), codes::RESOURCE_NOT_FOUND);
    }

    #[test]
    fn test_json_rpc_error_conversion() {
        let err = McpError::method_not_found_with_suggestions(
            "unknown_method",
            vec!["tools/list".to_string(), "resources/list".to_string()],
        );

        let json_err: JsonRpcError = (&err).into();
        assert_eq!(json_err.code, codes::METHOD_NOT_FOUND);
        assert!(json_err.message.contains("unknown_method"));
        assert!(json_err.data.is_some());
    }

    #[test]
    fn test_json_rpc_error_conversion_boxed_variants() {
        // Test InvalidParams (boxed)
        let err = McpError::invalid_params_detailed(
            "test_method",
            "invalid value",
            Some("args.count".to_string()),
            Some("number".to_string()),
            Some("string".to_string()),
        );
        let json_err: JsonRpcError = (&err).into();
        assert_eq!(json_err.code, codes::INVALID_PARAMS);
        let data = json_err.data.unwrap();
        assert_eq!(data["method"], "test_method");
        assert_eq!(data["param_path"], "args.count");

        // Test Transport (boxed)
        let err = McpError::transport_with_context(
            TransportErrorKind::ConnectionFailed,
            "connection refused",
            TransportContext::new("websocket").with_remote_addr("ws://localhost:8080"),
        );
        let json_err: JsonRpcError = (&err).into();
        assert_eq!(json_err.code, codes::SERVER_ERROR_START);
        assert!(json_err.data.is_some());

        // Test ToolExecution (boxed)
        let err = McpError::tool_error_detailed(
            "calculator",
            "division by zero",
            true,
            Some(serde_json::json!({"operation": "divide"})),
        );
        let json_err: JsonRpcError = (&err).into();
        assert!(json_err.data.is_some());
        let data = json_err.data.unwrap();
        assert_eq!(data["operation"], "divide");

        // Test HandshakeFailed (boxed)
        let err = McpError::handshake_failed_with_versions(
            "version mismatch",
            Some("2024-11-05".to_string()),
            Some("2025-11-25".to_string()),
        );
        let json_err: JsonRpcError = (&err).into();
        assert!(json_err.data.is_some());
        let data = json_err.data.unwrap();
        assert_eq!(data["client_version"], "2024-11-05");
        assert_eq!(data["server_version"], "2025-11-25");
    }

    #[test]
    fn test_recoverable_errors() {
        assert!(McpError::invalid_params("m", "test").is_recoverable());
        assert!(McpError::resource_not_found("uri").is_recoverable());
        assert!(!McpError::internal("test").is_recoverable());

        // Test boxed tool execution with recoverable flag
        let recoverable_tool = McpError::tool_error_detailed("tool", "error", true, None);
        assert!(recoverable_tool.is_recoverable());

        let non_recoverable_tool = McpError::tool_error_detailed("tool", "error", false, None);
        assert!(!non_recoverable_tool.is_recoverable());
    }

    #[test]
    fn test_boxed_error_display() {
        // Ensure Display works correctly for boxed variants
        let err = McpError::invalid_params("method", "bad params");
        assert!(err.to_string().contains("method"));
        assert!(err.to_string().contains("bad params"));

        let err = McpError::transport(TransportErrorKind::Timeout, "connection timed out");
        assert!(err.to_string().contains("timeout"));
        assert!(err.to_string().contains("connection timed out"));

        let err = McpError::tool_error("my_tool", "tool failed");
        assert!(err.to_string().contains("my_tool"));
        assert!(err.to_string().contains("tool failed"));

        let err = McpError::handshake_failed("protocol mismatch");
        assert!(err.to_string().contains("protocol mismatch"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let mcp_err: McpError = io_err.into();

        // Should be converted to Transport error with appropriate kind
        if let McpError::Transport(details) = mcp_err {
            assert_eq!(details.kind, TransportErrorKind::ConnectionFailed);
            assert!(details.message.contains("refused"));
        } else {
            panic!("Expected Transport error variant");
        }
    }
}
