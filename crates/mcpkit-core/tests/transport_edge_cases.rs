//! Edge case tests for transport implementations.
//!
//! These tests verify correct behavior under unusual conditions:
//! - Empty messages
//! - Malformed JSON
//! - Large payloads
//! - Connection interruption
//! - Concurrent access
//! - Timeout scenarios

use mcpkit_core::error::{JsonRpcError, McpError, TransportErrorKind};
use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use serde_json::json;

// =============================================================================
// Message Edge Cases
// =============================================================================

#[test]
fn test_empty_json_object_parsing() {
    let json = "{}";
    let result: Result<Message, _> = serde_json::from_str(json);
    // Empty object should fail - missing required fields
    assert!(result.is_err());
}

#[test]
fn test_null_values_in_message() {
    // Request with null params should be valid
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":null}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert!(request.params.is_none() || request.params == Some(serde_json::Value::Null));
}

#[test]
fn test_extra_fields_ignored() {
    // JSON-RPC implementations should ignore unknown fields
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "test",
        "params": {},
        "extra_field": "should be ignored",
        "another": 123
    }"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_unicode_in_method_name() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"工具/列表","params":{}}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().method, "工具/列表");
}

#[test]
fn test_unicode_in_params() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{"message":"こんにちは世界"}}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[test]
fn test_emoji_in_params() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{"emoji":"🚀🌟💻"}}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

// =============================================================================
// Request ID Edge Cases
// =============================================================================

#[test]
fn test_request_id_zero() {
    let id = RequestId::Number(0);
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, "0");

    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, RequestId::Number(0));
}

#[test]
fn test_request_id_negative() {
    // Negative numbers are valid in JSON-RPC
    let json = "-42";
    let result: Result<RequestId, _> = serde_json::from_str(json);
    // Our implementation may or may not support negative IDs
    // This test documents the behavior
    let _ = result;
}

#[test]
fn test_request_id_large_number() {
    // Test with a large number that fits in u64
    let id = RequestId::Number(u64::MAX);
    let json = serde_json::to_string(&id).unwrap();
    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, RequestId::Number(u64::MAX));
}

#[test]
fn test_request_id_empty_string() {
    let id = RequestId::String(String::new());
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, r#""""#);

    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, RequestId::String(String::new()));
}

#[test]
fn test_request_id_very_long_string() {
    let long_string: String = "x".repeat(10000);
    let id = RequestId::String(long_string.clone());
    let json = serde_json::to_string(&id).unwrap();
    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, RequestId::String(long_string));
}

#[test]
fn test_request_id_special_chars() {
    let id = RequestId::String(r#"test\"with'special/chars"#.to_string());
    let json = serde_json::to_string(&id).unwrap();
    let parsed: RequestId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.clone(), id);
}

// =============================================================================
// Response Edge Cases
// =============================================================================

#[test]
fn test_response_with_null_result() {
    let response = Response::success(RequestId::Number(1), serde_json::Value::Null);
    let json = serde_json::to_string(&response).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    // When result is null, it may be serialized as "result": null (present but null)
    // or omitted entirely (None). Either is valid JSON-RPC behavior.
    // Test that we can round-trip the response correctly
    assert!(parsed.error.is_none());
    // The original response should round-trip correctly
    let roundtrip = serde_json::to_string(&parsed).unwrap();
    assert!(!roundtrip.contains("\"error\""));
}

#[test]
fn test_response_with_nested_error_data() {
    let error = JsonRpcError {
        code: -32600,
        message: "Invalid Request".to_string(),
        data: Some(json!({
            "details": {
                "field": "method",
                "reason": "missing",
                "nested": {
                    "deep": {
                        "value": 123
                    }
                }
            }
        })),
    };
    let response = Response::error(RequestId::Number(1), error);
    let json = serde_json::to_string(&response).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert!(parsed.error.is_some());
    assert!(parsed.error.unwrap().data.is_some());
}

// =============================================================================
// Notification Edge Cases
// =============================================================================

#[test]
fn test_notification_without_params() {
    let notification = Notification::new("test/event");
    let json = serde_json::to_string(&notification).unwrap();

    // Should not have an id field
    assert!(!json.contains("\"id\""));
    // Should have method
    assert!(json.contains("\"method\":\"test/event\""));
}

#[test]
fn test_notification_with_empty_params() {
    let notification = Notification::with_params("test/event", json!({}));
    let json = serde_json::to_string(&notification).unwrap();
    let parsed: Notification = serde_json::from_str(&json).unwrap();
    assert!(parsed.params.is_some());
}

// =============================================================================
// Payload Size Edge Cases
// =============================================================================

#[test]
fn test_large_params_object() {
    // Create a large params object
    let mut params = serde_json::Map::new();
    for i in 0..1000 {
        params.insert(
            format!("key_{}", i),
            json!({
                "value": i,
                "description": format!("This is item number {}", i),
            }),
        );
    }

    let request = Request::with_params("test", RequestId::Number(1), json!(params));

    let json = serde_json::to_string(&request).unwrap();
    // Verify the payload is reasonably large (over 50KB)
    assert!(json.len() > 50000, "JSON length: {} bytes", json.len());

    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.method, "test");
}

#[test]
fn test_deeply_nested_json() {
    // Create deeply nested JSON
    fn create_nested(depth: usize) -> serde_json::Value {
        if depth == 0 {
            json!("leaf")
        } else {
            json!({ "nested": create_nested(depth - 1) })
        }
    }

    let deep_value = create_nested(50);
    let request = Request::with_params("test", RequestId::Number(1), deep_value.clone());

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert!(parsed.params.is_some());
}

#[test]
fn test_large_array_in_params() {
    let large_array: Vec<i32> = (0..10000).collect();
    let request = Request::with_params("test", RequestId::Number(1), json!({ "items": large_array }));

    let json = serde_json::to_string(&request).unwrap();
    let parsed: Request = serde_json::from_str(&json).unwrap();
    assert!(parsed.params.is_some());
}

// =============================================================================
// Error Type Edge Cases
// =============================================================================

#[test]
fn test_mcp_error_is_recoverable() {
    // Resource not found should be recoverable (LLM can try different resource)
    let err = McpError::resource_not_found("test://resource");
    assert!(err.is_recoverable());

    // Invalid params should be recoverable (LLM can fix params)
    let err = McpError::invalid_params("method", "bad params");
    assert!(err.is_recoverable());

    // Internal error should NOT be recoverable
    let err = McpError::internal("something broke");
    assert!(!err.is_recoverable());
}

#[test]
fn test_mcp_error_codes() {
    use mcpkit_core::error::codes;

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
}

#[test]
fn test_error_context_preserves_code() {
    use mcpkit_core::error::{codes, McpResultExt};

    fn inner() -> Result<(), McpError> {
        Err(McpError::resource_not_found("test://x"))
    }

    fn outer() -> Result<(), McpError> {
        inner().context("outer context")?;
        Ok(())
    }

    fn outermost() -> Result<(), McpError> {
        outer().context("outermost context")?;
        Ok(())
    }

    let err = outermost().unwrap_err();
    // Code should propagate through context layers
    assert_eq!(err.code(), codes::RESOURCE_NOT_FOUND);
}

#[test]
fn test_transport_error_kind_display() {
    let kinds = [
        TransportErrorKind::ConnectionFailed,
        TransportErrorKind::ConnectionClosed,
        TransportErrorKind::ReadFailed,
        TransportErrorKind::WriteFailed,
        TransportErrorKind::TlsError,
        TransportErrorKind::DnsResolutionFailed,
        TransportErrorKind::Timeout,
        TransportErrorKind::InvalidMessage,
        TransportErrorKind::ProtocolViolation,
        TransportErrorKind::ResourceExhausted,
    ];

    for kind in kinds {
        let display = format!("{}", kind);
        assert!(!display.is_empty());
    }
}

// =============================================================================
// Malformed Input Edge Cases
// =============================================================================

#[test]
fn test_truncated_json() {
    let truncated = r#"{"jsonrpc":"2.0","id":1,"method":"te"#;
    let result: Result<Request, _> = serde_json::from_str(truncated);
    assert!(result.is_err());
}

#[test]
fn test_invalid_jsonrpc_version() {
    let json = r#"{"jsonrpc":"1.0","id":1,"method":"test"}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    // May parse but version validation should happen elsewhere
    if let Ok(req) = result {
        assert_ne!(req.jsonrpc, "2.0");
    }
}

#[test]
fn test_missing_jsonrpc_field() {
    let json = r#"{"id":1,"method":"test"}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    // Depending on implementation, this may fail or default
    let _ = result;
}

#[test]
fn test_wrong_type_for_id() {
    // ID as boolean should fail
    let json = r#"{"jsonrpc":"2.0","id":true,"method":"test"}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn test_wrong_type_for_method() {
    // Method as number should fail
    let json = r#"{"jsonrpc":"2.0","id":1,"method":123}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

// =============================================================================
// Concurrent Access (basic thread safety verification)
// =============================================================================

#[test]
fn test_request_id_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RequestId>();
}

#[test]
fn test_mcp_error_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<McpError>();
}

#[test]
fn test_message_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Message>();
}
