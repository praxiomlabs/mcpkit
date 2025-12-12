//! Tool types for MCP servers.
//!
//! Tools are functions that MCP servers expose for AI assistants to invoke.
//! Each tool has a name, description, and JSON Schema defining its input.

use super::content::Content;
use serde::{Deserialize, Serialize};

/// A tool definition exposed by an MCP server.
///
/// Tools are callable functions with defined input schemas. AI assistants
/// can invoke tools to perform actions like searching databases, sending
/// emails, or executing code.
///
/// # Example
///
/// ```rust
/// use mcpkit_core::types::Tool;
///
/// let tool = Tool::new("search")
///     .description("Search the database")
///     .input_schema(serde_json::json!({
///         "type": "object",
///         "properties": {
///             "query": { "type": "string" }
///         },
///         "required": ["query"]
///     }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Unique name of the tool.
    pub name: String,
    /// Human-readable description of what the tool does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema defining the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// Optional annotations providing hints about tool behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

impl Tool {
    /// Create a new tool with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            annotations: None,
        }
    }

    /// Set the tool's description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the tool's input schema.
    #[must_use]
    pub fn input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Set the tool's annotations.
    #[must_use]
    pub fn annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = Some(annotations);
        self
    }

    /// Check if this tool is marked as read-only.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.annotations
            .as_ref()
            .and_then(|a| a.read_only_hint)
            .unwrap_or(false)
    }

    /// Check if this tool is marked as destructive.
    #[must_use]
    pub fn is_destructive(&self) -> bool {
        self.annotations
            .as_ref()
            .and_then(|a| a.destructive_hint)
            .unwrap_or(false)
    }

    /// Add a string parameter to the tool's input schema.
    ///
    /// # Panics
    ///
    /// Panics if the input schema doesn't have a "properties" object.
    /// This should not happen unless `input_schema()` was called with
    /// a schema that lacks the "properties" field.
    #[must_use]
    pub fn with_string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.ensure_properties();
        let props = self.input_schema.get_mut("properties").expect(
            "input_schema missing 'properties' - use Tool::new() or ensure schema has properties",
        );
        props[&name] = serde_json::json!({
            "type": "string",
            "description": description.into()
        });
        if required {
            self.add_required(&name);
        }
        self
    }

    /// Add a number parameter to the tool's input schema.
    ///
    /// # Panics
    ///
    /// Panics if the input schema doesn't have a "properties" object.
    #[must_use]
    pub fn with_number_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.ensure_properties();
        let props = self.input_schema.get_mut("properties").expect(
            "input_schema missing 'properties' - use Tool::new() or ensure schema has properties",
        );
        props[&name] = serde_json::json!({
            "type": "number",
            "description": description.into()
        });
        if required {
            self.add_required(&name);
        }
        self
    }

    /// Add a boolean parameter to the tool's input schema.
    ///
    /// # Panics
    ///
    /// Panics if the input schema doesn't have a "properties" object.
    #[must_use]
    pub fn with_boolean_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.ensure_properties();
        let props = self.input_schema.get_mut("properties").expect(
            "input_schema missing 'properties' - use Tool::new() or ensure schema has properties",
        );
        props[&name] = serde_json::json!({
            "type": "boolean",
            "description": description.into()
        });
        if required {
            self.add_required(&name);
        }
        self
    }

    /// Ensure the `input_schema` has a "properties" object.
    fn ensure_properties(&mut self) {
        if self.input_schema.get("properties").is_none() {
            self.input_schema["properties"] = serde_json::json!({});
        }
    }

    /// Helper to add a parameter name to the required list.
    fn add_required(&mut self, name: &str) {
        let schema = &mut self.input_schema;
        if let Some(required) = schema.get_mut("required") {
            if let Some(arr) = required.as_array_mut() {
                arr.push(serde_json::Value::String(name.to_string()));
            }
        } else {
            schema["required"] = serde_json::json!([name]);
        }
    }
}

/// Annotations providing hints about tool behavior.
///
/// These hints help AI assistants make better decisions about when
/// and how to use tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAnnotations {
    /// Human-readable title for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// If true, the tool only reads data (no side effects).
    #[serde(rename = "readOnlyHint", skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    /// If true, the tool may perform destructive operations.
    #[serde(rename = "destructiveHint", skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    /// If true, repeated calls with the same input yield the same result.
    #[serde(rename = "idempotentHint", skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    /// If true, the tool can accept arbitrary additional properties.
    #[serde(rename = "openWorldHint", skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    /// Create annotations for a read-only tool.
    #[must_use]
    pub fn read_only() -> Self {
        Self {
            read_only_hint: Some(true),
            ..Default::default()
        }
    }

    /// Create annotations for a destructive tool.
    #[must_use]
    pub fn destructive() -> Self {
        Self {
            destructive_hint: Some(true),
            ..Default::default()
        }
    }

    /// Create annotations for an idempotent tool.
    #[must_use]
    pub fn idempotent() -> Self {
        Self {
            idempotent_hint: Some(true),
            ..Default::default()
        }
    }

    /// Mark this tool as read-only.
    #[must_use]
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only_hint = Some(read_only);
        self
    }

    /// Mark this tool as destructive.
    #[must_use]
    pub const fn with_destructive(mut self, destructive: bool) -> Self {
        self.destructive_hint = Some(destructive);
        self
    }

    /// Mark this tool as idempotent.
    #[must_use]
    pub const fn with_idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent_hint = Some(idempotent);
        self
    }
}

/// The result of calling a tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallToolResult {
    /// The content returned by the tool.
    pub content: Vec<Content>,
    /// If true, this result represents an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl CallToolResult {
    /// Create a successful text result.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(text)],
            is_error: None,
        }
    }

    /// Create a successful result with multiple content items.
    #[must_use]
    pub const fn content(content: Vec<Content>) -> Self {
        Self {
            content,
            is_error: None,
        }
    }

    /// Create an error result.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(message)],
            is_error: Some(true),
        }
    }

    /// Check if this result indicates an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.is_error.unwrap_or(false)
    }
}

/// A simplified tool output type for handler implementations.
///
/// This type provides a more ergonomic API for tool handlers, with
/// automatic conversion to [`CallToolResult`].
#[derive(Debug, Clone)]
pub enum ToolOutput {
    /// Successful output.
    Success(CallToolResult),
    /// Recoverable error (visible to LLM for self-correction).
    RecoverableError {
        /// The error message.
        message: String,
        /// An optional suggestion for how to fix the error.
        suggestion: Option<String>,
    },
}

impl ToolOutput {
    /// Create a text result.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Success(CallToolResult::text(text))
    }

    /// Create a result with multiple content items.
    #[must_use]
    pub const fn content(content: Vec<Content>) -> Self {
        Self::Success(CallToolResult::content(content))
    }

    /// Create a JSON result.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string_pretty(value)?;
        Ok(Self::text(json))
    }

    /// Create a recoverable error.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::RecoverableError {
            message: message.into(),
            suggestion: None,
        }
    }

    /// Create a recoverable error with a suggestion.
    #[must_use]
    pub fn error_with_suggestion(
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::RecoverableError {
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }
}

impl From<ToolOutput> for CallToolResult {
    fn from(output: ToolOutput) -> Self {
        match output {
            ToolOutput::Success(result) => result,
            ToolOutput::RecoverableError {
                message,
                suggestion,
            } => {
                let mut text = message;
                if let Some(sug) = suggestion {
                    text = format!("{text}\n\nSuggestion: {sug}");
                }
                Self::error(text)
            }
        }
    }
}

/// Request parameters for listing tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListToolsRequest {
    /// Cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for listing tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    /// The list of available tools.
    pub tools: Vec<Tool>,
    /// Cursor for the next page, if more tools exist.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Request parameters for calling a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolRequest {
    /// Name of the tool to call.
    pub name: String,
    /// Arguments to pass to the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_builder() {
        let tool = Tool::new("search")
            .description("Search the database")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }))
            .annotations(ToolAnnotations::read_only());

        assert_eq!(tool.name, "search");
        assert_eq!(tool.description, Some("Search the database".to_string()));
        assert!(tool.is_read_only());
        assert!(!tool.is_destructive());
    }

    #[test]
    fn test_tool_result() {
        let result = CallToolResult::text("Found 5 results");
        assert!(!result.is_error());
        assert_eq!(result.content.len(), 1);

        let error = CallToolResult::error("Query failed");
        assert!(error.is_error());
    }

    #[test]
    fn test_tool_output_conversion() {
        let output = ToolOutput::text("Success");
        let result: CallToolResult = output.into();
        assert!(!result.is_error());

        let output = ToolOutput::error_with_suggestion(
            "Invalid query",
            "Try using quotation marks around phrases",
        );
        let result: CallToolResult = output.into();
        assert!(result.is_error());
        assert!(result.content[0].as_text().unwrap().contains("Suggestion"));
    }

    #[test]
    fn test_tool_serialization() {
        let tool = Tool::new("test")
            .description("A test tool")
            .input_schema(serde_json::json!({"type": "object"}));

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"inputSchema\""));
    }
}
