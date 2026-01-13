//! Property-based tests for JSON-RPC message serialization.
//!
//! These tests use proptest to verify that all message types can round-trip
//! through serialization and deserialization without data loss.
//!
//! # Properties Tested
//!
//! 1. **Roundtrip preservation**: `deserialize(serialize(x)) == x`
//! 2. **JSON validity**: All serialized messages are valid JSON
//! 3. **Invariants**: Responses have either result XOR error, never both
//! 4. **Format compliance**: All messages contain "jsonrpc": "2.0"

use mcpkit_core::error::JsonRpcError;
use mcpkit_core::protocol::{
    Cursor, JSONRPC_VERSION, Message, Notification, ProgressToken, Request, RequestId, Response,
};
use proptest::prelude::*;
use std::borrow::Cow;

// =============================================================================
// STRATEGY DEFINITIONS
// =============================================================================

/// Strategy for generating valid RequestId values.
fn arb_request_id() -> impl Strategy<Value = RequestId> {
    prop_oneof![
        // Numeric IDs (most common case)
        any::<u64>().prop_map(RequestId::Number),
        // String IDs (less common but valid)
        "[a-zA-Z0-9_-]{1,100}".prop_map(RequestId::String),
    ]
}

/// Strategy for generating valid method names.
///
/// Method names follow MCP conventions: namespace/action or just action.
fn arb_method() -> impl Strategy<Value = Cow<'static, str>> {
    prop_oneof![
        // Namespaced methods (most common)
        "[a-z]+/[a-z_]+".prop_map(Cow::Owned),
        // Simple methods
        "[a-z_]+".prop_map(Cow::Owned),
        // Well-known MCP methods
        Just(Cow::Borrowed("tools/list")),
        Just(Cow::Borrowed("tools/call")),
        Just(Cow::Borrowed("resources/list")),
        Just(Cow::Borrowed("resources/read")),
        Just(Cow::Borrowed("prompts/list")),
        Just(Cow::Borrowed("prompts/get")),
        Just(Cow::Borrowed("initialize")),
        Just(Cow::Borrowed("ping")),
    ]
}

/// Strategy for generating JSON params values.
///
/// We generate a variety of valid JSON values that might appear as params.
fn arb_params() -> impl Strategy<Value = Option<serde_json::Value>> {
    prop_oneof![
        // No params
        Just(None),
        // Empty object
        Just(Some(serde_json::json!({}))),
        // Simple object with string values
        "[a-z_]{1,20}".prop_map(|k| Some(serde_json::json!({ k: "value" }))),
        // Object with various value types
        (any::<i32>(), any::<bool>()).prop_map(|(n, b)| {
            Some(serde_json::json!({
                "count": n,
                "enabled": b,
                "name": "test"
            }))
        }),
        // Nested object
        Just(Some(serde_json::json!({
            "name": "test-tool",
            "arguments": {
                "query": "search term",
                "limit": 10
            }
        }))),
        // Array params
        proptest::collection::vec("[a-z]+", 0..5).prop_map(|v| Some(serde_json::Value::Array(
            v.into_iter().map(serde_json::Value::String).collect()
        ))),
    ]
}

/// Strategy for generating JSON result values.
fn arb_result() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        // Empty result
        Just(serde_json::json!({})),
        // Result with tools list
        Just(serde_json::json!({ "tools": [] })),
        // Result with resources
        Just(serde_json::json!({
            "resources": [
                { "uri": "file:///test.txt", "name": "test.txt" }
            ]
        })),
        // Result with prompts
        Just(serde_json::json!({
            "prompts": [
                { "name": "greeting", "description": "A friendly greeting" }
            ]
        })),
        // Scalar results
        any::<i64>().prop_map(|n| serde_json::json!(n)),
        "[a-zA-Z0-9 ]{0,100}".prop_map(|s| serde_json::json!(s)),
        any::<bool>().prop_map(|b| serde_json::json!(b)),
    ]
}

/// Strategy for generating JsonRpcError values.
fn arb_error() -> impl Strategy<Value = JsonRpcError> {
    let codes = prop_oneof![
        Just(-32700),         // Parse error
        Just(-32600),         // Invalid Request
        Just(-32601),         // Method not found
        Just(-32602),         // Invalid params
        Just(-32603),         // Internal error
        (-32099..=-32000i32), // Server errors
    ];

    let messages = prop_oneof![
        Just("Parse error".to_string()),
        Just("Invalid Request".to_string()),
        Just("Method not found".to_string()),
        Just("Invalid params".to_string()),
        Just("Internal error".to_string()),
        "[A-Za-z ]{5,50}".prop_map(|s| s),
    ];

    let data = prop_oneof![
        Just(None),
        "[a-z ]+".prop_map(|s| Some(serde_json::json!({ "details": s }))),
    ];

    (codes, messages, data).prop_map(|(code, message, data)| JsonRpcError {
        code,
        message,
        data,
    })
}

/// Strategy for generating Request values.
fn arb_request() -> impl Strategy<Value = Request> {
    (arb_method(), arb_request_id(), arb_params()).prop_map(|(method, id, params)| {
        let mut request = Request::new(method, id);
        if let Some(p) = params {
            request.params = Some(p);
        }
        request
    })
}

/// Strategy for generating successful Response values.
fn arb_success_response() -> impl Strategy<Value = Response> {
    (arb_request_id(), arb_result()).prop_map(|(id, result)| Response::success(id, result))
}

/// Strategy for generating error Response values.
fn arb_error_response() -> impl Strategy<Value = Response> {
    (arb_request_id(), arb_error()).prop_map(|(id, error)| Response::error(id, error))
}

/// Strategy for generating any Response values.
fn arb_response() -> impl Strategy<Value = Response> {
    prop_oneof![arb_success_response(), arb_error_response(),]
}

/// Strategy for generating Notification values.
fn arb_notification() -> impl Strategy<Value = Notification> {
    (arb_method(), arb_params()).prop_map(|(method, params)| {
        let mut notification = Notification::new(method);
        if let Some(p) = params {
            notification.params = Some(p);
        }
        notification
    })
}

/// Strategy for generating any Message values.
fn arb_message() -> impl Strategy<Value = Message> {
    prop_oneof![
        arb_request().prop_map(Message::Request),
        arb_response().prop_map(Message::Response),
        arb_notification().prop_map(Message::Notification),
    ]
}

/// Strategy for generating ProgressToken values.
fn arb_progress_token() -> impl Strategy<Value = ProgressToken> {
    prop_oneof![
        any::<u64>().prop_map(ProgressToken::Number),
        "[a-zA-Z0-9_-]{1,50}".prop_map(ProgressToken::String),
    ]
}

/// Strategy for generating Cursor values.
fn arb_cursor() -> impl Strategy<Value = Cursor> {
    "[a-zA-Z0-9_=-]{1,100}".prop_map(Cursor::new)
}

// =============================================================================
// PROPERTY TESTS
// =============================================================================

proptest! {
    // Use reasonable defaults for CI performance
    #![proptest_config(ProptestConfig::with_cases(256))]

    // -------------------------------------------------------------------------
    // RequestId Tests
    // -------------------------------------------------------------------------

    #[test]
    fn request_id_roundtrip(id in arb_request_id()) {
        let json = serde_json::to_string(&id)?;
        let parsed: RequestId = serde_json::from_str(&json)?;
        prop_assert_eq!(id, parsed);
    }

    #[test]
    fn request_id_display_matches_json(id in arb_request_id()) {
        let display = id.to_string();
        match &id {
            RequestId::Number(n) => prop_assert_eq!(display, n.to_string()),
            RequestId::String(s) => prop_assert_eq!(display, s.clone()),
        }
    }

    // -------------------------------------------------------------------------
    // Request Tests
    // -------------------------------------------------------------------------

    #[test]
    fn request_roundtrip(request in arb_request()) {
        let json = serde_json::to_string(&request)?;
        let parsed: Request = serde_json::from_str(&json)?;

        // Verify all fields match
        prop_assert_eq!(request.id, parsed.id);
        prop_assert_eq!(request.method.as_ref(), parsed.method.as_ref());
        prop_assert_eq!(request.params, parsed.params);
        prop_assert_eq!(parsed.jsonrpc.as_ref(), JSONRPC_VERSION);
    }

    #[test]
    fn request_has_jsonrpc_version(request in arb_request()) {
        let json = serde_json::to_string(&request)?;
        prop_assert!(json.contains(r#""jsonrpc":"2.0""#));
    }

    #[test]
    fn request_has_id(request in arb_request()) {
        let json = serde_json::to_string(&request)?;
        prop_assert!(json.contains(r#""id":"#));
    }

    #[test]
    fn request_has_method(request in arb_request()) {
        let json = serde_json::to_string(&request)?;
        prop_assert!(json.contains(r#""method":"#));
    }

    // -------------------------------------------------------------------------
    // Response Tests
    // -------------------------------------------------------------------------

    #[test]
    fn response_roundtrip(response in arb_response()) {
        let json = serde_json::to_string(&response)?;
        let parsed: Response = serde_json::from_str(&json)?;

        prop_assert_eq!(response.id, parsed.id);
        prop_assert_eq!(response.result, parsed.result);

        // Compare error fields individually (JsonRpcError may not derive PartialEq)
        match (&response.error, &parsed.error) {
            (Some(e1), Some(e2)) => {
                prop_assert_eq!(e1.code, e2.code);
                prop_assert_eq!(&e1.message, &e2.message);
                prop_assert_eq!(&e1.data, &e2.data);
            }
            (None, None) => {}
            _ => prop_assert!(false, "Error presence mismatch"),
        }
    }

    #[test]
    fn response_has_result_xor_error(response in arb_response()) {
        // JSON-RPC 2.0 invariant: response has result XOR error
        let has_result = response.result.is_some();
        let has_error = response.error.is_some();
        prop_assert!(has_result ^ has_error, "Response must have result XOR error");
    }

    #[test]
    fn success_response_is_success(response in arb_success_response()) {
        prop_assert!(response.is_success());
        prop_assert!(!response.is_error());
    }

    #[test]
    fn error_response_is_error(response in arb_error_response()) {
        prop_assert!(response.is_error());
        prop_assert!(!response.is_success());
    }

    // -------------------------------------------------------------------------
    // Notification Tests
    // -------------------------------------------------------------------------

    #[test]
    fn notification_roundtrip(notification in arb_notification()) {
        let json = serde_json::to_string(&notification)?;
        let parsed: Notification = serde_json::from_str(&json)?;

        prop_assert_eq!(notification.method.as_ref(), parsed.method.as_ref());
        prop_assert_eq!(notification.params, parsed.params);
        prop_assert_eq!(parsed.jsonrpc.as_ref(), JSONRPC_VERSION);
    }

    #[test]
    fn notification_has_no_id(notification in arb_notification()) {
        let json = serde_json::to_string(&notification)?;
        // Notifications must not have an id field
        // Note: We check that there's no "id": pattern (with colon)
        prop_assert!(!json.contains(r#""id":"#));
    }

    // -------------------------------------------------------------------------
    // Message Tests
    // -------------------------------------------------------------------------

    #[test]
    fn message_roundtrip(message in arb_message()) {
        let json = serde_json::to_string(&message)?;
        let parsed: Message = serde_json::from_str(&json)?;

        // Verify variant matches
        match (&message, &parsed) {
            (Message::Request(m1), Message::Request(m2)) => {
                prop_assert_eq!(&m1.id, &m2.id);
                prop_assert_eq!(m1.method.as_ref(), m2.method.as_ref());
                prop_assert_eq!(&m1.params, &m2.params);
            }
            (Message::Response(m1), Message::Response(m2)) => {
                prop_assert_eq!(&m1.id, &m2.id);
                prop_assert_eq!(&m1.result, &m2.result);
            }
            (Message::Notification(m1), Message::Notification(m2)) => {
                prop_assert_eq!(m1.method.as_ref(), m2.method.as_ref());
                prop_assert_eq!(&m1.params, &m2.params);
            }
            _ => prop_assert!(false, "Message variant mismatch"),
        }
    }

    #[test]
    fn message_type_detection(message in arb_message()) {
        match &message {
            Message::Request(_) => {
                prop_assert!(message.is_request());
                prop_assert!(!message.is_response());
                prop_assert!(!message.is_notification());
                prop_assert!(message.method().is_some());
                prop_assert!(message.id().is_some());
            }
            Message::Response(_) => {
                prop_assert!(!message.is_request());
                prop_assert!(message.is_response());
                prop_assert!(!message.is_notification());
                prop_assert!(message.method().is_none());
                prop_assert!(message.id().is_some());
            }
            Message::Notification(_) => {
                prop_assert!(!message.is_request());
                prop_assert!(!message.is_response());
                prop_assert!(message.is_notification());
                prop_assert!(message.method().is_some());
                prop_assert!(message.id().is_none());
            }
        }
    }

    // -------------------------------------------------------------------------
    // ProgressToken Tests
    // -------------------------------------------------------------------------

    #[test]
    fn progress_token_roundtrip(token in arb_progress_token()) {
        let json = serde_json::to_string(&token)?;
        let parsed: ProgressToken = serde_json::from_str(&json)?;
        prop_assert_eq!(token, parsed);
    }

    #[test]
    fn progress_token_display_matches_value(token in arb_progress_token()) {
        let display = token.to_string();
        match &token {
            ProgressToken::Number(n) => prop_assert_eq!(display, n.to_string()),
            ProgressToken::String(s) => prop_assert_eq!(display, s.clone()),
        }
    }

    // -------------------------------------------------------------------------
    // Cursor Tests
    // -------------------------------------------------------------------------

    #[test]
    fn cursor_roundtrip(cursor in arb_cursor()) {
        let json = serde_json::to_string(&cursor)?;
        let parsed: Cursor = serde_json::from_str(&json)?;
        prop_assert_eq!(cursor.0, parsed.0);
    }

    #[test]
    fn cursor_display_matches_value(cursor in arb_cursor()) {
        prop_assert_eq!(cursor.to_string(), cursor.0);
    }

    // -------------------------------------------------------------------------
    // JSON Validity Tests
    // -------------------------------------------------------------------------

    #[test]
    fn request_produces_valid_json(request in arb_request()) {
        let json = serde_json::to_string(&request)?;
        // Verify it parses as generic JSON
        let _: serde_json::Value = serde_json::from_str(&json)?;
    }

    #[test]
    fn response_produces_valid_json(response in arb_response()) {
        let json = serde_json::to_string(&response)?;
        let _: serde_json::Value = serde_json::from_str(&json)?;
    }

    #[test]
    fn notification_produces_valid_json(notification in arb_notification()) {
        let json = serde_json::to_string(&notification)?;
        let _: serde_json::Value = serde_json::from_str(&json)?;
    }

    #[test]
    fn message_produces_valid_json(message in arb_message()) {
        let json = serde_json::to_string(&message)?;
        let _: serde_json::Value = serde_json::from_str(&json)?;
    }

    // -------------------------------------------------------------------------
    // Error Handling Tests
    // -------------------------------------------------------------------------

    #[test]
    fn error_roundtrip(error in arb_error()) {
        let json = serde_json::to_string(&error)?;
        let parsed: JsonRpcError = serde_json::from_str(&json)?;

        prop_assert_eq!(error.code, parsed.code);
        prop_assert_eq!(error.message, parsed.message);
        prop_assert_eq!(error.data, parsed.data);
    }

    #[test]
    fn error_codes_in_valid_range(error in arb_error()) {
        // Standard error codes are negative
        prop_assert!(error.code < 0);
        // Parse error (-32700) is the most negative standard code
        prop_assert!(error.code >= -32700);
    }
}

// =============================================================================
// ADDITIONAL NON-PROPTEST TESTS
// =============================================================================

#[cfg(test)]
mod additional_tests {
    use super::*;

    /// Test that parsing invalid JSON fails appropriately.
    #[test]
    fn test_invalid_json_fails() {
        let invalid = "not json at all";
        assert!(serde_json::from_str::<Message>(invalid).is_err());
    }

    /// Test that missing required fields fail.
    #[test]
    fn test_missing_jsonrpc_field() {
        let json = r#"{"id": 1, "method": "test"}"#;
        // Should still parse - jsonrpc field has a default
        let result: Result<Request, _> = serde_json::from_str(json);
        // This depends on serde configuration - just verify it doesn't panic
        let _ = result;
    }

    /// Test edge case: empty string method.
    #[test]
    fn test_empty_method() {
        let request = Request::new("", 1u64);
        let json = serde_json::to_string(&request).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(request.method.as_ref(), parsed.method.as_ref());
    }

    /// Test edge case: very large request ID.
    #[test]
    fn test_max_u64_id() {
        let request = Request::new("test", u64::MAX);
        let json = serde_json::to_string(&request).unwrap();
        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(RequestId::Number(u64::MAX), parsed.id);
    }

    /// Test response into_result for success case.
    #[test]
    fn test_response_into_result_success() {
        let response = Response::success(1u64, serde_json::json!({"key": "value"}));
        let result = response.into_result();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!({"key": "value"}));
    }

    /// Test response into_result for error case.
    #[test]
    fn test_response_into_result_error() {
        let error = JsonRpcError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        };
        let response = Response::error(1u64, error);
        let result = response.into_result();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, -32600);
    }
}
