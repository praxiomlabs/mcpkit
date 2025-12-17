//! Error scenario tests for MCP server and client implementations.
//!
//! These tests verify correct error handling behavior:
//! - Protocol errors (parse, invalid request, method not found)
//! - Capability errors (unsupported operations)
//! - Tool execution errors (recoverable vs fatal)
//! - Resource access errors
//! - Error propagation and context preservation

use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::error::{McpError, McpResultExt, codes};
use mcpkit_core::protocol::{RequestId, Response};

// =============================================================================
// Protocol Error Scenarios
// =============================================================================

#[test]
fn test_parse_error_has_correct_code() {
    let err = McpError::parse("invalid JSON");
    assert_eq!(err.code(), codes::PARSE_ERROR);
    assert_eq!(err.code(), -32700);
}

#[test]
fn test_invalid_request_error_has_correct_code() {
    let err = McpError::invalid_request("missing jsonrpc field");
    assert_eq!(err.code(), codes::INVALID_REQUEST);
    assert_eq!(err.code(), -32600);
}

#[test]
fn test_method_not_found_error_has_correct_code() {
    let err = McpError::method_not_found("unknown/method");
    assert_eq!(err.code(), codes::METHOD_NOT_FOUND);
    assert_eq!(err.code(), -32601);
}

#[test]
fn test_invalid_params_error_has_correct_code() {
    let err = McpError::invalid_params("tools/call", "missing required field 'name'");
    assert_eq!(err.code(), codes::INVALID_PARAMS);
    assert_eq!(err.code(), -32602);
}

#[test]
fn test_internal_error_has_correct_code() {
    let err = McpError::internal("unexpected panic");
    assert_eq!(err.code(), codes::INTERNAL_ERROR);
    assert_eq!(err.code(), -32603);
}

// =============================================================================
// MCP-Specific Error Scenarios
// =============================================================================

#[test]
fn test_resource_not_found_error() {
    let err = McpError::resource_not_found("file://missing.txt");
    assert_eq!(err.code(), codes::RESOURCE_NOT_FOUND);

    // Should be recoverable - LLM can try a different resource
    assert!(err.is_recoverable());
}

#[test]
fn test_resource_access_denied_error() {
    let err = McpError::ResourceAccessDenied {
        uri: "file://secret.txt".to_string(),
        reason: Some("Insufficient permissions".to_string()),
    };

    // Access denied is NOT recoverable with same request
    assert!(!err.is_recoverable());
}

#[test]
fn test_method_not_found_for_unknown_tool() {
    // Tools are methods in MCP, so unknown tool returns method_not_found
    let err = McpError::method_not_found("tools/call/unknown_tool");

    // Should indicate method doesn't exist
    assert!(err.to_string().contains("unknown_tool"));
}

#[test]
fn test_capability_not_supported_error() {
    let err = McpError::CapabilityNotSupported {
        capability: "sampling".to_string(),
        available: Box::new(["tools".to_string(), "resources".to_string()]),
    };

    assert!(!err.is_recoverable());
    let msg = err.to_string();
    assert!(msg.contains("sampling"));
}

// =============================================================================
// Error Recoverability Tests
// =============================================================================

#[test]
fn test_recoverable_errors() {
    // Errors that an LLM can potentially recover from by adjusting its request
    let recoverable = vec![
        McpError::resource_not_found("test://x"),
        McpError::invalid_params("method", "bad value"),
    ];

    for err in recoverable {
        assert!(err.is_recoverable(), "Expected {err:?} to be recoverable");
    }
}

#[test]
fn test_non_recoverable_errors() {
    // Errors that indicate a fundamental problem
    let non_recoverable = vec![
        McpError::internal("crash"),
        McpError::parse("bad json"),
        McpError::invalid_request("malformed"),
        McpError::method_not_found("unknown"),
        McpError::CapabilityNotSupported {
            capability: "x".to_string(),
            available: Box::new([]),
        },
        McpError::ResourceAccessDenied {
            uri: "x".to_string(),
            reason: None,
        },
    ];

    for err in non_recoverable {
        assert!(
            !err.is_recoverable(),
            "Expected {err:?} to NOT be recoverable"
        );
    }
}

// =============================================================================
// Error Context Preservation Tests
// =============================================================================

#[test]
fn test_context_preserves_error_code() {
    // Context is a trait method on Result, not on McpError directly
    fn inner() -> Result<(), McpError> {
        Err(McpError::resource_not_found("test://resource"))
    }

    fn outer() -> Result<(), McpError> {
        inner().context("additional info")?;
        Ok(())
    }

    let err = outer().unwrap_err();

    // Original error code should be preserved
    assert_eq!(err.code(), codes::RESOURCE_NOT_FOUND);
}

#[test]
fn test_context_chain() {
    fn level3() -> Result<(), McpError> {
        Err(McpError::resource_not_found("deep://resource"))
    }

    fn level2() -> Result<(), McpError> {
        level3().context("level2 context")?;
        Ok(())
    }

    fn level1() -> Result<(), McpError> {
        level2().context("level1 context")?;
        Ok(())
    }

    let err = level1().unwrap_err();

    // Original error code should be preserved through all layers
    assert_eq!(err.code(), codes::RESOURCE_NOT_FOUND);

    // Context should be in the message
    let msg = err.to_string();
    assert!(msg.contains("level1") || msg.contains("level2"));
}

// =============================================================================
// Error Conversion Tests
// =============================================================================

#[test]
fn test_io_error_converts_to_server_error() {
    use std::io::{Error as IoError, ErrorKind};

    let io_err = IoError::new(ErrorKind::NotFound, "file not found");
    let mcp_err: McpError = io_err.into();

    // IO errors become server errors (in the -32000 to -32099 range)
    assert!(mcp_err.code() >= codes::SERVER_ERROR_END);
    assert!(mcp_err.code() <= codes::SERVER_ERROR_START);
}

#[test]
fn test_json_error_converts_to_parse() {
    let json_str = "{ invalid json }";
    let json_err = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();

    let mcp_err: McpError = json_err.into();

    // JSON errors become parse errors
    assert_eq!(mcp_err.code(), codes::PARSE_ERROR);
}

// =============================================================================
// Error Message Quality Tests
// =============================================================================

#[test]
fn test_error_messages_are_descriptive() {
    let errors = vec![
        (McpError::parse("unexpected token"), "token"),
        (McpError::invalid_request("missing method"), "method"),
        (McpError::method_not_found("test_method"), "test_method"),
        (McpError::resource_not_found("file://x"), "file://x"),
        (
            McpError::invalid_params("calculator", "missing arg"),
            "calculator",
        ),
    ];

    for (err, expected_substring) in errors {
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains(expected_substring),
            "Error message '{msg}' should contain '{expected_substring}'"
        );
    }
}

#[test]
fn test_debug_format_includes_details() {
    let err = McpError::invalid_params("tools/call", "missing 'name' field");
    let debug = format!("{err:?}");

    // Debug should include more details
    assert!(debug.len() > err.to_string().len());
}

// =============================================================================
// Response Error Conversion Tests
// =============================================================================

#[test]
fn test_error_to_response_conversion() {
    use mcpkit_core::error::JsonRpcError;

    let err = McpError::method_not_found("unknown/method");
    let json_err: JsonRpcError = (&err).into();
    let response = Response::error(RequestId::Number(1), json_err);

    assert!(response.result.is_none());
    assert!(response.error.is_some());

    let resp_err = response.error.unwrap();
    assert_eq!(resp_err.code, codes::METHOD_NOT_FOUND);
}

#[test]
fn test_error_data_preserved_in_response() {
    use mcpkit_core::error::JsonRpcError;

    // Use the detailed constructor to provide structured data
    let err = McpError::invalid_params_detailed(
        "tools/call",
        "invalid type for 'count'",
        Some("count".to_string()),
        Some("number".to_string()),
        Some("string".to_string()),
    );

    let json_err: JsonRpcError = (&err).into();

    // Error data should be preserved with structured details
    assert!(json_err.data.is_some());
    let data = json_err.data.unwrap();
    assert!(data.get("param_path").is_some() || data.get("expected").is_some());
}

// =============================================================================
// Capability Error Scenarios
// =============================================================================

#[test]
fn test_capabilities_mismatch_detection() {
    let server_caps = ServerCapabilities::new().with_tools().with_resources();

    // Server supports tools and resources
    assert!(server_caps.has_tools());
    assert!(server_caps.has_resources());
    assert!(!server_caps.has_prompts());
    assert!(!server_caps.has_tasks());
}

#[test]
fn test_client_capabilities_detection() {
    let client_caps = ClientCapabilities::new().with_sampling();

    assert!(client_caps.has_sampling());
    assert!(!client_caps.has_elicitation());
}

// =============================================================================
// Error Recovery Pattern Tests
// =============================================================================

#[test]
fn test_retry_with_modified_params() {
    // Simulate a scenario where initial params fail, but modified params succeed
    fn try_operation(attempt: u32) -> Result<String, McpError> {
        if attempt < 3 {
            Err(McpError::invalid_params(
                "search",
                format!("query too short (attempt {attempt})"),
            ))
        } else {
            Ok("success".to_string())
        }
    }

    let mut attempt = 0;
    let result = loop {
        attempt += 1;
        match try_operation(attempt) {
            Ok(value) => break Ok(value),
            Err(err) if err.is_recoverable() && attempt < 5 => {
                // Retry for recoverable errors - loop continues naturally
            }
            Err(err) => break Err(err),
        }
    };

    assert!(result.is_ok());
    assert_eq!(attempt, 3); // Should succeed on 3rd attempt
}

// =============================================================================
// Concurrent Error Handling Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_errors_are_isolated() {
    use std::sync::Arc;
    use tokio::sync::Barrier;

    let barrier = Arc::new(Barrier::new(10));
    let mut handles = vec![];

    for i in 0..10 {
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            // Each task creates its own error
            let err = McpError::internal(format!("error from task {i}"));

            // Error should contain task-specific info
            assert!(err.to_string().contains(&format!("{i}")));

            err
        }));
    }

    // All tasks should complete with their own error
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .collect();

    assert_eq!(results.len(), 10);
    for result in results {
        assert!(result.is_ok());
    }
}

// =============================================================================
// Error Chain Tests
// =============================================================================

#[test]
fn test_error_source_chain() {
    use std::error::Error;

    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let mcp_err: McpError = io_err.into();

    // Should be able to access error source
    let _source = mcp_err.source();
}

// =============================================================================
// Protocol Compliance Error Tests
// =============================================================================

#[test]
fn test_reserved_error_codes_range() {
    // JSON-RPC reserved error codes: -32000 to -32099
    // Standard error codes: -32700, -32600, -32601, -32602, -32603

    assert!(codes::PARSE_ERROR < -32600);
    assert!(codes::INVALID_REQUEST == -32600);
    assert!(codes::METHOD_NOT_FOUND == -32601);
    assert!(codes::INVALID_PARAMS == -32602);
    assert!(codes::INTERNAL_ERROR == -32603);
}

#[test]
fn test_mcp_error_codes_in_server_range() {
    // MCP-specific errors should be in the server error range (-32000 to -32099)
    // or use custom positive codes

    let mcp_codes = vec![codes::RESOURCE_NOT_FOUND, codes::USER_REJECTED];

    for code in mcp_codes {
        // Should not conflict with standard JSON-RPC codes
        assert!(code != -32700);
        assert!(code != -32600);
        assert!(code != -32601);
        assert!(code != -32602);
        assert!(code != -32603);
    }

    // RESOURCE_NOT_FOUND should be in server error range
    assert!(codes::RESOURCE_NOT_FOUND >= codes::SERVER_ERROR_END);
    assert!(codes::RESOURCE_NOT_FOUND <= codes::SERVER_ERROR_START);
}

// =============================================================================
// Error Suggestion Tests
// =============================================================================

#[test]
fn test_error_with_detailed_params() {
    use mcpkit_core::error::JsonRpcError;

    // Use detailed params to provide helpful error context
    let err = McpError::invalid_params_detailed(
        "tools/call",
        "Invalid tool name",
        Some("tool_name".to_string()),
        Some("existing tool name".to_string()),
        Some("calculator".to_string()),
    );

    let json_err: JsonRpcError = (&err).into();
    let data = json_err.data.unwrap();

    // Detailed params should be in data
    assert!(data.get("expected").is_some() || data.get("actual").is_some());
}

// =============================================================================
// Internal Error Tests
// =============================================================================

#[test]
fn test_internal_error_wraps_source() {
    let err = McpError::internal("database connection failed");
    assert_eq!(err.code(), codes::INTERNAL_ERROR);
    assert!(err.to_string().contains("database"));
}

#[test]
fn test_internal_error_not_recoverable() {
    let err = McpError::internal("panic occurred");
    assert!(!err.is_recoverable());
}
