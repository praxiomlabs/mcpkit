//! Custom assertions for MCP testing.
//!
//! This module provides assertion helpers that make it easier to test
//! MCP responses and results.

use mcpkit_core::types::{CallToolResult, Content, ToolOutput};

/// Assert that a tool result is successful and contains expected text.
///
/// # Panics
///
/// Panics if the result is an error or doesn't contain the expected text.
pub fn assert_tool_success(result: &CallToolResult, expected_text: &str) {
    assert!(
        !result.is_error(),
        "Expected successful tool result, but got error"
    );
    assert!(!result.content.is_empty(), "Tool result has no content");

    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .collect::<Vec<_>>()
        .join("");

    assert!(
        text.contains(expected_text),
        "Expected tool result to contain '{}', but got '{}'",
        expected_text,
        text
    );
}

/// Assert that a tool result is an error with expected message.
///
/// # Panics
///
/// Panics if the result is successful or doesn't contain the expected message.
pub fn assert_tool_error(result: &CallToolResult, expected_message: &str) {
    assert!(
        result.is_error(),
        "Expected error tool result, but got success"
    );

    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .collect::<Vec<_>>()
        .join("");

    assert!(
        text.contains(expected_message),
        "Expected error message to contain '{}', but got '{}'",
        expected_message,
        text
    );
}

/// Assert that a ToolOutput is successful with expected text.
///
/// # Panics
///
/// Panics if the output is an error or doesn't contain the expected text.
pub fn assert_output_success(output: &ToolOutput, expected_text: &str) {
    match output {
        ToolOutput::Success(result) => assert_tool_success(result, expected_text),
        ToolOutput::RecoverableError { message, .. } => {
            panic!(
                "Expected successful output, but got error: {}",
                message
            );
        }
    }
}

/// Assert that a ToolOutput is an error.
///
/// # Panics
///
/// Panics if the output is successful.
pub fn assert_output_error(output: &ToolOutput, expected_message: &str) {
    match output {
        ToolOutput::Success(_) => {
            panic!("Expected error output, but got success");
        }
        ToolOutput::RecoverableError { message, .. } => {
            assert!(
                message.contains(expected_message),
                "Expected error message to contain '{}', but got '{}'",
                expected_message,
                message
            );
        }
    }
}

/// Assert that content is text with expected value.
///
/// # Panics
///
/// Panics if the content is not text or doesn't match.
pub fn assert_content_text(content: &Content, expected: &str) {
    match content {
        Content::Text(tc) => {
            assert_eq!(
                tc.text, expected,
                "Expected content text '{}', but got '{}'",
                expected, tc.text
            );
        }
        _ => panic!("Expected text content, but got other type"),
    }
}

/// Assert that content contains expected text.
///
/// # Panics
///
/// Panics if the content doesn't contain the expected text.
pub fn assert_content_contains(content: &Content, expected: &str) {
    match content {
        Content::Text(tc) => {
            assert!(
                tc.text.contains(expected),
                "Expected content to contain '{}', but got '{}'",
                expected,
                tc.text
            );
        }
        _ => panic!("Expected text content, but got other type"),
    }
}

/// Macro for asserting tool result success.
///
/// # Example
///
/// ```rust
/// use mcpkit_testing::assert_tool_result;
/// use mcpkit_core::types::CallToolResult;
///
/// let result = CallToolResult::text("expected text");
/// assert_tool_result!(result, "expected text");
/// ```
#[macro_export]
macro_rules! assert_tool_result {
    ($result:expr, $expected:expr) => {
        $crate::assertions::assert_tool_success(&$result, $expected)
    };
}

/// Macro for asserting tool result error.
///
/// # Example
///
/// ```rust
/// use mcpkit_testing::assert_tool_error_msg;
/// use mcpkit_core::types::CallToolResult;
///
/// let result = CallToolResult::error("error message");
/// assert_tool_error_msg!(result, "error message");
/// ```
#[macro_export]
macro_rules! assert_tool_error_msg {
    ($result:expr, $expected:expr) => {
        $crate::assertions::assert_tool_error(&$result, $expected)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_tool_success_passes() {
        let result = CallToolResult::text("Hello, world!");
        assert_tool_success(&result, "Hello");
    }

    #[test]
    #[should_panic(expected = "Expected successful tool result")]
    fn test_assert_tool_success_fails_on_error() {
        let result = CallToolResult::error("Something went wrong");
        assert_tool_success(&result, "Hello");
    }

    #[test]
    #[should_panic(expected = "Expected tool result to contain")]
    fn test_assert_tool_success_fails_on_wrong_text() {
        let result = CallToolResult::text("Goodbye!");
        assert_tool_success(&result, "Hello");
    }

    #[test]
    fn test_assert_tool_error_passes() {
        let result = CallToolResult::error("Something went wrong");
        assert_tool_error(&result, "went wrong");
    }

    #[test]
    #[should_panic(expected = "Expected error tool result")]
    fn test_assert_tool_error_fails_on_success() {
        let result = CallToolResult::text("Success!");
        assert_tool_error(&result, "error");
    }

    #[test]
    fn test_assert_output_success() {
        let output = ToolOutput::text("Result");
        assert_output_success(&output, "Result");
    }

    #[test]
    fn test_assert_output_error() {
        let output = ToolOutput::error("Failed");
        assert_output_error(&output, "Failed");
    }

    #[test]
    fn test_assert_content_text() {
        let content = Content::text("Hello");
        assert_content_text(&content, "Hello");
    }

    #[test]
    fn test_assert_content_contains() {
        let content = Content::text("Hello, world!");
        assert_content_contains(&content, "world");
    }
}
