//! Test fixtures for MCP testing.
//!
//! This module provides pre-built fixtures for common testing scenarios.

use crate::mock::{MockPrompt, MockResource, MockTool};
use mcpkit_core::types::{Prompt, PromptArgument, Resource, Tool, ToolAnnotations, ToolOutput};

/// Create a set of sample tools for testing.
///
/// Returns tools:
/// - `echo`: Echoes back the input
/// - `add`: Adds two numbers
/// - `multiply`: Multiplies two numbers
/// - `fail`: Always returns an error
#[must_use]
pub fn sample_tools() -> Vec<MockTool> {
    vec![
        MockTool::new("echo")
            .description("Echo back the input")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }))
            .handler(|args| {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no message)");
                Ok(ToolOutput::text(message))
            }),
        MockTool::new("add")
            .description("Add two numbers")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                },
                "required": ["a", "b"]
            }))
            .annotations(ToolAnnotations::read_only())
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(ToolOutput::text(format!("{}", a + b)))
            }),
        MockTool::new("multiply")
            .description("Multiply two numbers")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" }
                },
                "required": ["a", "b"]
            }))
            .annotations(ToolAnnotations::read_only())
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(ToolOutput::text(format!("{}", a * b)))
            }),
        MockTool::new("fail")
            .description("Always fails")
            .returns_error("This tool always fails"),
    ]
}

/// Create a set of sample resources for testing.
///
/// Returns resources:
/// - `test://readme`: A sample README file
/// - `test://config`: A sample configuration file
/// - `test://data`: Sample JSON data
#[must_use]
pub fn sample_resources() -> Vec<MockResource> {
    vec![
        MockResource::new("test://readme", "README")
            .description("A sample README file")
            .mime_type("text/markdown")
            .content("# Test Server\n\nThis is a test server."),
        MockResource::new("test://config", "Configuration")
            .description("Server configuration")
            .mime_type("application/json")
            .content(r#"{"debug": true, "port": 8080}"#),
        MockResource::new("test://data", "Sample Data")
            .description("Sample data for testing")
            .mime_type("application/json")
            .content(r#"{"items": [1, 2, 3], "count": 3}"#),
    ]
}

/// Create a set of sample prompts for testing.
///
/// Returns prompts:
/// - `summarize`: A prompt for summarizing text
/// - `translate`: A prompt for translation
#[must_use]
pub fn sample_prompts() -> Vec<MockPrompt> {
    vec![
        MockPrompt::new("summarize")
            .description("Summarize the given text")
            .template("Please summarize the following text:\n\n{{text}}"),
        MockPrompt::new("translate")
            .description("Translate text to another language")
            .template("Translate the following text to {{language}}:\n\n{{text}}"),
    ]
}

/// Create a standard tool definition for testing.
#[must_use]
pub fn tool_definition(name: &str, description: &str) -> Tool {
    Tool::new(name).description(description)
}

/// Create a standard resource definition for testing.
#[must_use]
pub fn resource_definition(uri: &str, name: &str) -> Resource {
    Resource::new(uri, name)
}

/// Create a standard prompt definition for testing.
#[must_use]
pub fn prompt_definition(name: &str) -> Prompt {
    Prompt::new(name)
}

/// Create a prompt with arguments.
#[must_use]
pub fn prompt_with_args(name: &str, description: &str, args: Vec<(&str, &str, bool)>) -> Prompt {
    let mut prompt = Prompt::new(name).description(description);
    for (arg_name, arg_desc, required) in args {
        let arg = if required {
            PromptArgument::required(arg_name, arg_desc)
        } else {
            PromptArgument::optional(arg_name, arg_desc)
        };
        prompt = prompt.argument(arg);
    }
    prompt
}

/// Create the calculator tool set.
///
/// This is a common fixture for testing arithmetic operations.
#[must_use]
pub fn calculator_tools() -> Vec<MockTool> {
    vec![
        MockTool::new("add")
            .description("Add two numbers together")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number", "description": "First number" },
                    "b": { "type": "number", "description": "Second number" }
                },
                "required": ["a", "b"]
            }))
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(ToolOutput::text(format!("{}", a + b)))
            }),
        MockTool::new("subtract")
            .description("Subtract two numbers")
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(ToolOutput::text(format!("{}", a - b)))
            }),
        MockTool::new("multiply")
            .description("Multiply two numbers")
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                Ok(ToolOutput::text(format!("{}", a * b)))
            }),
        MockTool::new("divide")
            .description("Divide two numbers")
            .handler(|args| {
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                if b == 0.0 {
                    Ok(ToolOutput::error("Cannot divide by zero"))
                } else {
                    Ok(ToolOutput::text(format!("{}", a / b)))
                }
            }),
    ]
}

/// Create a file system resource set.
///
/// This is a common fixture for testing file operations.
#[must_use]
pub fn filesystem_resources() -> Vec<MockResource> {
    vec![
        MockResource::new("file:///project/src/main.rs", "main.rs")
            .mime_type("text/x-rust")
            .content("fn main() {\n    println!(\"Hello, world!\");\n}"),
        MockResource::new("file:///project/Cargo.toml", "Cargo.toml")
            .mime_type("text/x-toml")
            .content("[package]\nname = \"test\"\nversion = \"0.1.0\""),
        MockResource::new("file:///project/README.md", "README.md")
            .mime_type("text/markdown")
            .content("# Test Project\n\nA test project."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_tools() {
        let tools = sample_tools();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().any(|t| t.name == "echo"));
        assert!(tools.iter().any(|t| t.name == "add"));
    }

    #[test]
    fn test_sample_resources() {
        let resources = sample_resources();
        assert_eq!(resources.len(), 3);
        assert!(resources.iter().any(|r| r.uri == "test://readme"));
    }

    #[test]
    fn test_calculator_tools() {
        let tools = calculator_tools();
        assert_eq!(tools.len(), 4);

        // Test the add tool
        let add = tools.iter().find(|t| t.name == "add").unwrap();
        let result = add.call(serde_json::json!({"a": 5, "b": 3})).unwrap();
        match result {
            ToolOutput::Success(r) => {
                if let mcpkit_core::types::Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "8");
                }
            }
            _ => panic!("Expected success"),
        }

        // Test the divide tool with zero
        let divide = tools.iter().find(|t| t.name == "divide").unwrap();
        let result = divide.call(serde_json::json!({"a": 5, "b": 0})).unwrap();
        match result {
            ToolOutput::RecoverableError { message, .. } => {
                assert!(message.contains("zero"));
            }
            _ => panic!("Expected error"),
        }
    }

    #[test]
    fn test_prompt_with_args() {
        let prompt = prompt_with_args(
            "test",
            "A test prompt",
            vec![
                ("required_arg", "A required argument", true),
                ("optional_arg", "An optional argument", false),
            ],
        );

        assert_eq!(prompt.name, "test");
        let args = prompt.arguments.unwrap();
        assert_eq!(args.len(), 2);
        assert!(args[0].required.unwrap());
        assert!(!args[1].required.unwrap());
    }
}
