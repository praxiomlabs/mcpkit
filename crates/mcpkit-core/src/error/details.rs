//! Boxed error detail types to reduce `McpError` enum size.
//!
//! These types contain detailed information about specific error categories
//! and are boxed within `McpError` variants to keep the enum size small.

use std::fmt;

use super::transport::{TransportContext, TransportErrorKind};

/// Type alias for boxed errors that are Send + Sync.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

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
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
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
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
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
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
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
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}
