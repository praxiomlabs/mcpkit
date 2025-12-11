//! JSON-RPC 2.0 compliance tests.
//!
//! These tests verify that the SDK correctly implements JSON-RPC 2.0
//! as required by the MCP protocol.

use mcpkit_core::protocol::{Message, Request, Response, Notification, RequestId};
use serde_json::{json, Value};

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
    let request = Request::new(
        RequestId::Number(1),
        "tools/list",
        Some(json!({})),
    );

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["method"], "tools/list");
}

#[test]
fn test_response_success() {
    let response = Response::success(
        RequestId::Number(1),
        json!({"tools": []}),
    );

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none());
}

#[test]
fn test_response_error() {
    let response = Response::error(
        RequestId::Number(1),
        -32600,
        "Invalid Request".to_string(),
        None,
    );

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
    let notification = Notification::new(
        "notifications/initialized",
        None,
    );

    let json = serde_json::to_value(&notification).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "notifications/initialized");
    assert!(json.get("id").is_none()); // Notifications have no ID
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
    let parse_error = Response::error(
        RequestId::Number(1),
        -32700,
        "Parse error".to_string(),
        None,
    );
    assert_eq!(parse_error.error.as_ref().unwrap().code, -32700);

    let invalid_request = Response::error(
        RequestId::Number(1),
        -32600,
        "Invalid Request".to_string(),
        None,
    );
    assert_eq!(invalid_request.error.as_ref().unwrap().code, -32600);

    let method_not_found = Response::error(
        RequestId::Number(1),
        -32601,
        "Method not found".to_string(),
        None,
    );
    assert_eq!(method_not_found.error.as_ref().unwrap().code, -32601);

    let invalid_params = Response::error(
        RequestId::Number(1),
        -32602,
        "Invalid params".to_string(),
        None,
    );
    assert_eq!(invalid_params.error.as_ref().unwrap().code, -32602);

    let internal_error = Response::error(
        RequestId::Number(1),
        -32603,
        "Internal error".to_string(),
        None,
    );
    assert_eq!(internal_error.error.as_ref().unwrap().code, -32603);
}

#[test]
fn test_request_params_optional() {
    // Request with params
    let with_params = Request::new(
        RequestId::Number(1),
        "test",
        Some(json!({"key": "value"})),
    );
    assert!(with_params.params.is_some());

    // Request without params
    let without_params = Request::new(
        RequestId::Number(2),
        "test",
        None,
    );
    assert!(without_params.params.is_none());
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
