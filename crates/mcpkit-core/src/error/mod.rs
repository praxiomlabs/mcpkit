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
//! - **Diagnostic-friendly**: Integrates with [`miette`] for detailed error reports
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

pub mod codes;
mod context;
mod details;
mod jsonrpc;
mod transport;
mod types;

// Re-export all public types
pub use codes::*;
pub use context::McpResultExt;
pub use details::{BoxError, HandshakeDetails, InvalidParamsDetails, ToolExecutionDetails, TransportDetails};
pub use jsonrpc::JsonRpcError;
pub use transport::{TransportContext, TransportErrorKind};
pub use types::McpError;

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
            "McpError is {size} bytes, should be <= 64 bytes. Consider boxing more variants."
        );

        // Also verify Result<(), McpError> is small
        let result_size = std::mem::size_of::<Result<(), McpError>>();
        assert!(
            result_size <= 72,
            "Result<(), McpError> is {result_size} bytes, should be <= 72 bytes."
        );
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(McpError::parse("test").code(), PARSE_ERROR);
        assert_eq!(
            McpError::invalid_request("test").code(),
            INVALID_REQUEST
        );
        assert_eq!(
            McpError::method_not_found("test").code(),
            METHOD_NOT_FOUND
        );
        assert_eq!(
            McpError::invalid_params("m", "test").code(),
            INVALID_PARAMS
        );
        assert_eq!(McpError::internal("test").code(), INTERNAL_ERROR);
        assert_eq!(
            McpError::transport(TransportErrorKind::ConnectionFailed, "test").code(),
            SERVER_ERROR_START
        );
        assert_eq!(
            McpError::tool_error("tool", "test").code(),
            SERVER_ERROR_START - 1
        );
        assert_eq!(
            McpError::handshake_failed("test").code(),
            SERVER_ERROR_START - 5
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
        assert_eq!(err.code(), RESOURCE_NOT_FOUND);
    }

    #[test]
    fn test_json_rpc_error_conversion() {
        let err = McpError::method_not_found_with_suggestions(
            "unknown_method",
            vec!["tools/list".to_string(), "resources/list".to_string()],
        );

        let json_err: JsonRpcError = (&err).into();
        assert_eq!(json_err.code, METHOD_NOT_FOUND);
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
        assert_eq!(json_err.code, INVALID_PARAMS);
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
        assert_eq!(json_err.code, SERVER_ERROR_START);
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
