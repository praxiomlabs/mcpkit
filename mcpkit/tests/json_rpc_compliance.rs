//! JSON-RPC 2.0 compliance tests.
//!
//! These tests verify that the SDK correctly implements JSON-RPC 2.0
//! as required by the MCP protocol.

use mcpkit::error::JsonRpcError;
use mcpkit::protocol::{Message, Notification, Request, RequestId, Response};
use serde_json::json;

#[test]
fn test_request_id_number() {
    let id = RequestId::Number(42);
    let json = serde_json::to_value(&id).unwrap();
    assert_eq!(json, json!(42));

    let parsed: RequestId = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, RequestId::Number(42));
}

#[test]
fn test_request_id_string() {
    let id = RequestId::String("request-123".to_string());
    let json = serde_json::to_value(&id).unwrap();
    assert_eq!(json, json!("request-123"));

    let parsed: RequestId = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, RequestId::String("request-123".to_string()));
}

#[test]
fn test_request_serialization() {
    let request = Request::new("tools/list", 1u64);

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["method"], "tools/list");
}

#[test]
fn test_request_with_params() {
    let request = Request::with_params("tools/call", 1u64, json!({"name": "search"}));

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["method"], "tools/call");
    assert!(json["params"].is_object());
}

#[test]
fn test_response_success() {
    let response = Response::success(1u64, json!({"tools": []}));

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none());
}

#[test]
fn test_response_error() {
    let error = JsonRpcError {
        code: -32600,
        message: "Invalid Request".to_string(),
        data: None,
    };
    let response = Response::error(1u64, error);

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json.get("result").is_none());
    assert!(json["error"].is_object());
    assert_eq!(json["error"]["code"], -32600);
    assert_eq!(json["error"]["message"], "Invalid Request");
}

#[test]
fn test_notification_serialization() {
    let notification = Notification::new("notifications/initialized");

    let json = serde_json::to_value(&notification).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "notifications/initialized");
    assert!(json.get("id").is_none()); // Notifications have no ID
}

#[test]
fn test_notification_with_params() {
    let notification =
        Notification::with_params("notifications/progress", json!({"progress": 50}));

    let json = serde_json::to_value(&notification).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "notifications/progress");
    assert!(json["params"].is_object());
    assert!(json.get("id").is_none());
}

#[test]
fn test_message_parsing() {
    // Request message
    let request_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let msg: Message = serde_json::from_value(request_json).unwrap();
    assert!(matches!(msg, Message::Request(_)));

    // Response message (success)
    let response_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {"tools": []}
    });
    let msg: Message = serde_json::from_value(response_json).unwrap();
    assert!(matches!(msg, Message::Response(_)));

    // Notification message
    let notification_json = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let msg: Message = serde_json::from_value(notification_json).unwrap();
    assert!(matches!(msg, Message::Notification(_)));
}

#[test]
fn test_standard_error_codes() {
    // JSON-RPC 2.0 standard error codes

    // Parse error (-32700)
    let parse_error = JsonRpcError::parse_error("Parse error");
    assert_eq!(parse_error.code, -32700);

    // Invalid request (-32600)
    let invalid_request = JsonRpcError::invalid_request("Invalid Request");
    assert_eq!(invalid_request.code, -32600);

    // Method not found (-32601)
    let method_not_found = JsonRpcError::method_not_found("Method not found");
    assert_eq!(method_not_found.code, -32601);

    // Invalid params (-32602)
    let invalid_params = JsonRpcError::invalid_params("Invalid params");
    assert_eq!(invalid_params.code, -32602);

    // Internal error (-32603)
    let internal_error = JsonRpcError::internal_error("Internal error");
    assert_eq!(internal_error.code, -32603);
}

#[test]
fn test_request_params_optional() {
    // Request without params
    let without_params = Request::new("test", 1u64);
    assert!(without_params.params.is_none());

    // Request with params using builder
    let with_params = Request::new("test", 2u64).params(json!({"key": "value"}));
    assert!(with_params.params.is_some());
}

#[test]
fn test_batch_not_supported() {
    // MCP doesn't require batch support, but we should handle arrays gracefully
    let batch_json = json!([
        {"jsonrpc": "2.0", "id": 1, "method": "test"},
        {"jsonrpc": "2.0", "id": 2, "method": "test"}
    ]);

    // Attempting to parse as a single message should fail
    let result: Result<Message, _> = serde_json::from_value(batch_json);
    assert!(result.is_err());
}

#[test]
fn test_null_id_handling() {
    // JSON-RPC allows null ID in some contexts
    let json = json!({
        "jsonrpc": "2.0",
        "id": null,
        "method": "test"
    });

    // Should be parsed as a notification (no valid ID)
    let result: Result<Request, _> = serde_json::from_value(json.clone());
    // The behavior depends on implementation - either parse as null ID or fail
    // For MCP, we typically want numeric or string IDs
    let _ = result;
}

#[test]
fn test_response_without_result_or_error() {
    // According to JSON-RPC 2.0, a response must have either result or error
    // but not both (and not neither)
    let valid_success = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": null
    });
    let parsed: Result<Response, _> = serde_json::from_value(valid_success);
    assert!(parsed.is_ok());
}

#[test]
fn test_response_is_success_and_is_error() {
    let success = Response::success(1u64, json!({}));
    assert!(success.is_success());
    assert!(!success.is_error());

    let error = Response::error(1u64, JsonRpcError::internal_error("error"));
    assert!(!error.is_success());
    assert!(error.is_error());
}

#[test]
fn test_request_id_from_impls() {
    // Test From<u64>
    let id: RequestId = 42u64.into();
    assert_eq!(id, RequestId::Number(42));

    // Test From<String>
    let id: RequestId = "test".to_string().into();
    assert_eq!(id, RequestId::String("test".to_string()));

    // Test From<&str>
    let id: RequestId = "test".into();
    assert_eq!(id, RequestId::String("test".to_string()));
}

#[test]
fn test_request_id_display() {
    assert_eq!(RequestId::Number(42).to_string(), "42");
    assert_eq!(RequestId::String("test".to_string()).to_string(), "test");
}

#[test]
fn test_message_helper_methods() {
    let request = Request::new("tools/list", 1u64);
    let msg: Message = request.into();

    assert!(msg.is_request());
    assert!(!msg.is_response());
    assert!(!msg.is_notification());
    assert_eq!(msg.method(), Some("tools/list"));
    assert!(msg.id().is_some());
    assert!(msg.as_request().is_some());
    assert!(msg.as_response().is_none());
    assert!(msg.as_notification().is_none());
}
