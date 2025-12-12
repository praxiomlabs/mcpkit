//! JSON-RPC 2.0 protocol types for the Model Context Protocol.
//!
//! This module provides the foundational JSON-RPC 2.0 types used for all
//! MCP communication. These types handle message framing, request/response
//! correlation, and notification delivery.
//!
//! # Protocol Overview
//!
//! MCP uses JSON-RPC 2.0 as its transport protocol. All messages are one of:
//!
//! - **Request**: A method call expecting a response
//! - **Response**: A reply to a request (success or error)
//! - **Notification**: A one-way message with no response
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::protocol::{Request, Response, RequestId};
//!
//! // Create a request
//! let request = Request::new("tools/list", RequestId::Number(1));
//!
//! // Parse a response
//! let json = r#"{"jsonrpc": "2.0", "id": 1, "result": {}}"#;
//! let response: Response = serde_json::from_str(json).unwrap();
//! ```

use crate::error::JsonRpcError;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The JSON-RPC version string. Always "2.0".
pub const JSONRPC_VERSION: &str = "2.0";

/// A JSON-RPC request ID.
///
/// Request IDs are used to correlate requests with their responses.
/// They can be either numbers or strings per the JSON-RPC 2.0 specification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// Numeric request ID (most common).
    Number(u64),
    /// String request ID.
    String(String),
}

impl RequestId {
    /// Create a new numeric request ID.
    #[must_use]
    pub const fn number(id: u64) -> Self {
        Self::Number(id)
    }

    /// Create a new string request ID.
    #[must_use]
    pub fn string(id: impl Into<String>) -> Self {
        Self::String(id.into())
    }
}

impl From<u64> for RequestId {
    fn from(id: u64) -> Self {
        Self::Number(id)
    }
}

impl From<String> for RequestId {
    fn from(id: String) -> Self {
        Self::String(id)
    }
}

impl From<&str> for RequestId {
    fn from(id: &str) -> Self {
        Self::String(id.to_string())
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "{s}"),
        }
    }
}

/// A JSON-RPC 2.0 request message.
///
/// Requests are method calls that expect a response. Each request has a unique
/// ID that is echoed in the corresponding response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// The JSON-RPC version. Always "2.0".
    pub jsonrpc: Cow<'static, str>,
    /// The request ID for correlation.
    pub id: RequestId,
    /// The method to invoke.
    pub method: Cow<'static, str>,
    /// The method parameters, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Request {
    /// Create a new request with no parameters.
    #[must_use]
    pub fn new(method: impl Into<Cow<'static, str>>, id: impl Into<RequestId>) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    /// Create a new request with parameters.
    #[must_use]
    pub fn with_params(
        method: impl Into<Cow<'static, str>>,
        id: impl Into<RequestId>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            id: id.into(),
            method: method.into(),
            params: Some(params),
        }
    }

    /// Set the parameters for this request.
    #[must_use]
    pub fn params(mut self, params: serde_json::Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Get the method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }
}

/// A JSON-RPC 2.0 response message.
///
/// Responses are sent in reply to requests. They contain either a result
/// (on success) or an error (on failure), never both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// The JSON-RPC version. Always "2.0".
    pub jsonrpc: Cow<'static, str>,
    /// The request ID this response corresponds to.
    pub id: RequestId,
    /// The result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// The error on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl Response {
    /// Create a successful response.
    #[must_use]
    pub fn success(id: impl Into<RequestId>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(id: impl Into<RequestId>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            id: id.into(),
            result: None,
            error: Some(error),
        }
    }

    /// Check if this response indicates success.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.result.is_some() && self.error.is_none()
    }

    /// Check if this response indicates an error.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Get the result, consuming self.
    ///
    /// Returns `Err` if this was an error response.
    pub fn into_result(self) -> Result<serde_json::Value, JsonRpcError> {
        if let Some(error) = self.error {
            Err(error)
        } else {
            self.result.ok_or_else(|| JsonRpcError {
                code: -32603,
                message: "Response contained neither result nor error".to_string(),
                data: None,
            })
        }
    }
}

/// A JSON-RPC 2.0 notification message.
///
/// Notifications are one-way messages that do not expect a response.
/// They have no ID field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// The JSON-RPC version. Always "2.0".
    pub jsonrpc: Cow<'static, str>,
    /// The notification method.
    pub method: Cow<'static, str>,
    /// The notification parameters, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Notification {
    /// Create a new notification with no parameters.
    #[must_use]
    pub fn new(method: impl Into<Cow<'static, str>>) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            method: method.into(),
            params: None,
        }
    }

    /// Create a new notification with parameters.
    #[must_use]
    pub fn with_params(method: impl Into<Cow<'static, str>>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: Cow::Borrowed(JSONRPC_VERSION),
            method: method.into(),
            params: Some(params),
        }
    }

    /// Set the parameters for this notification.
    #[must_use]
    pub fn params(mut self, params: serde_json::Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Get the method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }
}

/// A JSON-RPC 2.0 message (request, response, or notification).
///
/// This enum allows handling all message types uniformly during
/// parsing and routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    /// A request message.
    Request(Request),
    /// A response message.
    Response(Response),
    /// A notification message.
    Notification(Notification),
}

impl Message {
    /// Get the method name if this is a request or notification.
    #[must_use]
    pub fn method(&self) -> Option<&str> {
        match self {
            Self::Request(r) => Some(&r.method),
            Self::Notification(n) => Some(&n.method),
            Self::Response(_) => None,
        }
    }

    /// Get the request ID if this is a request or response.
    #[must_use]
    pub const fn id(&self) -> Option<&RequestId> {
        match self {
            Self::Request(r) => Some(&r.id),
            Self::Response(r) => Some(&r.id),
            Self::Notification(_) => None,
        }
    }

    /// Check if this is a request.
    #[must_use]
    pub const fn is_request(&self) -> bool {
        matches!(self, Self::Request(_))
    }

    /// Check if this is a response.
    #[must_use]
    pub const fn is_response(&self) -> bool {
        matches!(self, Self::Response(_))
    }

    /// Check if this is a notification.
    #[must_use]
    pub const fn is_notification(&self) -> bool {
        matches!(self, Self::Notification(_))
    }

    /// Try to get this as a request.
    #[must_use]
    pub const fn as_request(&self) -> Option<&Request> {
        match self {
            Self::Request(r) => Some(r),
            _ => None,
        }
    }

    /// Try to get this as a response.
    #[must_use]
    pub const fn as_response(&self) -> Option<&Response> {
        match self {
            Self::Response(r) => Some(r),
            _ => None,
        }
    }

    /// Try to get this as a notification.
    #[must_use]
    pub const fn as_notification(&self) -> Option<&Notification> {
        match self {
            Self::Notification(n) => Some(n),
            _ => None,
        }
    }
}

impl From<Request> for Message {
    fn from(r: Request) -> Self {
        Self::Request(r)
    }
}

impl From<Response> for Message {
    fn from(r: Response) -> Self {
        Self::Response(r)
    }
}

impl From<Notification> for Message {
    fn from(n: Notification) -> Self {
        Self::Notification(n)
    }
}

/// A progress token for tracking long-running operations.
///
/// Progress tokens are included in requests that may take a long time,
/// allowing the server to send progress updates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    /// Numeric progress token.
    Number(u64),
    /// String progress token.
    String(String),
}

impl std::fmt::Display for ProgressToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::String(s) => write!(f, "{s}"),
        }
    }
}

/// A cursor for paginated results.
///
/// Cursors are opaque strings that represent a position in a paginated
/// result set. Pass the cursor from a previous response to get the next page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cursor(pub String);

impl Cursor {
    /// Create a new cursor.
    #[must_use]
    pub fn new(cursor: impl Into<String>) -> Self {
        Self(cursor.into())
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Cursor {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Cursor {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = Request::new("tools/list", 1u64);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_request_with_params() {
        let request = Request::with_params(
            "tools/call",
            1u64,
            serde_json::json!({"name": "search", "arguments": {"query": "test"}}),
        );
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"params\""));
        assert!(json.contains("\"name\":\"search\""));
    }

    #[test]
    fn test_response_success() {
        let response = Response::success(1u64, serde_json::json!({"tools": []}));
        assert!(response.is_success());
        assert!(!response.is_error());

        let result = response.into_result().unwrap();
        assert!(result.get("tools").is_some());
    }

    #[test]
    fn test_response_error() {
        let error = JsonRpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        };
        let response = Response::error(1u64, error);
        assert!(!response.is_success());
        assert!(response.is_error());

        let err = response.into_result().unwrap_err();
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn test_notification() {
        let notification = Notification::with_params(
            "notifications/progress",
            serde_json::json!({"progress": 50, "total": 100}),
        );
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("\"method\":\"notifications/progress\""));
        assert!(!json.contains("\"id\"")); // Notifications have no ID
    }

    #[test]
    fn test_message_parsing() {
        // Request
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"test"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_request());
        assert_eq!(msg.method(), Some("test"));

        // Response
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_response());

        // Notification
        let json = r#"{"jsonrpc":"2.0","method":"notify"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_notification());
    }

    #[test]
    fn test_request_id_types() {
        // Number ID
        let request = Request::new("test", 42u64);
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":42"));

        // String ID
        let request = Request::new("test", "req-001");
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":\"req-001\""));
    }
}
