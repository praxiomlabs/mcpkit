//! JSON-RPC error response type and conversions.
//!
//! This module provides the `JsonRpcError` type for wire format
//! and conversions from `McpError`.

use serde::{Deserialize, Serialize};

use super::types::McpError;

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
            McpError::MethodNotFound {
                method, available, ..
            } => Some(serde_json::json!({
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
            McpError::ToolExecution(details) => details
                .data
                .clone()
                .or_else(|| Some(serde_json::json!({ "tool": details.tool }))),
            McpError::HandshakeFailed(details) => Some(serde_json::json!({
                "client_version": details.client_version,
                "server_version": details.server_version,
            })),
            McpError::WithContext { source, .. } => {
                let inner: Self = source.as_ref().into();
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
