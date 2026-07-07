//! Tool types for MCP servers.
//!
//! Tools are functions that MCP servers expose for AI assistants to invoke.
//! Each tool has a name, description, and JSON Schema defining its input.

use super::content::Content;
use super::meta::Meta;
use super::metadata::Icon;
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
    /// Optional human-readable display title.
    ///
    /// Display-name precedence is `title`, then `annotations.title`, then
    /// `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Human-readable description of what the tool does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema defining the tool's input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// Optional icons the client can display for this tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
    /// Optional annotations providing hints about tool behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
    /// Optional execution-related properties (e.g. task-augmented execution
    /// support).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ToolExecution>,
    /// JSON Schema describing the tool's structured output, if it produces any.
    ///
    /// When set, a successful result's `structuredContent` is expected to
    /// conform to this schema (MCP structured output).
    #[serde(rename = "outputSchema", skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl Tool {
    /// Create a new tool with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            icons: None,
            annotations: None,
            execution: None,
            output_schema: None,
            meta: None,
        }
    }

    /// Set the tool's display title (`title`).
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add an icon the client can display for this tool.
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icons.get_or_insert_with(Vec::new).push(icon);
        self
    }

    /// Set the tool's icons, replacing any already set.
    #[must_use]
    pub fn icons(mut self, icons: impl IntoIterator<Item = Icon>) -> Self {
        self.icons = Some(icons.into_iter().collect());
        self
    }

    /// Set the tool's execution properties.
    #[must_use]
    pub fn execution(mut self, execution: ToolExecution) -> Self {
        self.execution = Some(execution);
        self
    }

    /// Declare the tool's task-augmented execution support.
    #[must_use]
    pub fn task_support(mut self, task_support: TaskSupport) -> Self {
        self.execution
            .get_or_insert_with(ToolExecution::default)
            .task_support = Some(task_support);
        self
    }

    /// Set the tool's output schema (`outputSchema`).
    #[must_use]
    pub fn output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
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
    /// If `input_schema` is not an object (or its `properties` is not an
    /// object), it is coerced to a fresh object first.
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

    /// Ensure `input_schema` is an object with an object `properties` field.
    ///
    /// Coerces non-object values (missing, or set to a non-object by a caller)
    /// rather than panicking when a parameter is subsequently inserted.
    fn ensure_properties(&mut self) {
        if !self.input_schema.is_object() {
            self.input_schema = serde_json::json!({});
        }
        if !self.input_schema["properties"].is_object() {
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

/// Whether a tool supports task-augmented execution.
///
/// Task-augmented execution lets clients drive long-running tool calls through
/// the task system (poll for status instead of blocking on the response).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskSupport {
    /// The tool does not support task-augmented execution (spec default).
    #[default]
    Forbidden,
    /// The tool may be executed with task augmentation.
    Optional,
    /// The tool must be executed with task augmentation.
    Required,
}

/// Execution-related properties for a tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecution {
    /// Whether this tool supports task-augmented execution.
    ///
    /// Absent is equivalent to [`TaskSupport::Forbidden`].
    #[serde(rename = "taskSupport", skip_serializing_if = "Option::is_none")]
    pub task_support: Option<TaskSupport>,
}

impl ToolExecution {
    /// Create execution properties declaring the given task support.
    #[must_use]
    pub const fn with_task_support(task_support: TaskSupport) -> Self {
        Self {
            task_support: Some(task_support),
        }
    }
}

/// The result of calling a tool.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallToolResult {
    /// The content returned by the tool.
    ///
    /// Defaults to empty when omitted, so a result that carries only `isError`
    /// (or is `{}`) still deserializes — some peers send empty results without a
    /// `content` field.
    #[serde(default)]
    pub content: Vec<Content>,
    /// If true, this result represents an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Structured result conforming to the tool's `outputSchema`, if any.
    #[serde(
        rename = "structuredContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<super::object::Object>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl CallToolResult {
    /// Create a successful text result.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(text)],
            is_error: None,
            structured_content: None,
            meta: None,
        }
    }

    /// Create a successful result with multiple content items.
    #[must_use]
    pub const fn content(content: Vec<Content>) -> Self {
        Self {
            content,
            is_error: None,
            structured_content: None,
            meta: None,
        }
    }

    /// Create an error result.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(message)],
            is_error: Some(true),
            structured_content: None,
            meta: None,
        }
    }

    /// Attach structured content (matching the tool's `outputSchema`).
    #[must_use]
    pub fn with_structured_content(mut self, value: super::object::Object) -> Self {
        self.structured_content = Some(value);
        self
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

/// Typed structured-output wrapper for tool return values.
///
/// Returning `Json(value)` from a tool serializes `value` into the result's
/// `structuredContent`, with a pretty-printed JSON fallback in `content`.
///
/// When a `#[tool]` method returns `Json<T>`, the `#[mcp_server]` macro also
/// derives the tool's `outputSchema` from `T`, so `T` should derive `ToolInput`
/// in addition to `serde::Serialize`.
///
/// The spec requires `structuredContent` to be a JSON **object**. If `T`
/// serializes to a non-object (e.g. a bare number or array), only the text
/// fallback is emitted and `structuredContent` is omitted.
#[derive(Debug, Clone)]
pub struct Json<T>(pub T);

impl<T: Serialize> From<Json<T>> for ToolOutput {
    fn from(json: Json<T>) -> Self {
        let text = serde_json::to_string_pretty(&json.0).unwrap_or_default();
        let result = match serde_json::to_value(&json.0) {
            Ok(serde_json::Value::Object(map)) => {
                CallToolResult::text(text).with_structured_content(map)
            }
            _ => CallToolResult::text(text),
        };
        Self::Success(result)
    }
}

impl From<String> for ToolOutput {
    /// Convert a string into a text `ToolOutput`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::types::ToolOutput;
    ///
    /// let output: ToolOutput = "Hello, world!".to_string().into();
    /// ```
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&str> for ToolOutput {
    /// Convert a string slice into a text `ToolOutput`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use mcpkit_core::types::ToolOutput;
    ///
    /// let output: ToolOutput = "Hello, world!".into();
    /// ```
    fn from(text: &str) -> Self {
        Self::text(text)
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
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// Request parameters for calling a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolRequest {
    /// Name of the tool to call.
    pub name: String,
    /// Arguments to pass to the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<super::object::Object>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_meta_round_trips_and_omits() -> Result<(), Box<dyn std::error::Error>> {
        let t: Tool = serde_json::from_value(
            serde_json::json!({"name":"n","inputSchema":{"type":"object"},"_meta":{"k":"v"}}),
        )?;
        assert_eq!(serde_json::to_value(&t)?["_meta"]["k"], "v");
        assert!(serde_json::to_value(Tool::new("n"))?.get("_meta").is_none());
        Ok(())
    }

    #[test]
    fn with_param_coerces_non_object_properties_instead_of_panicking() {
        // #18: `properties` present but not an object used to panic on insert.
        let tool = Tool::new("t")
            .input_schema(serde_json::json!({ "properties": 5 }))
            .with_string_param("name", "the name", true);

        assert!(tool.input_schema["properties"].is_object());
        assert_eq!(tool.input_schema["properties"]["name"]["type"], "string");
        assert_eq!(tool.input_schema["required"], serde_json::json!(["name"]));
    }

    #[test]
    fn with_param_coerces_non_object_input_schema() {
        // #18: a non-object input_schema must be coerced, not panicked on.
        let tool = Tool::new("t")
            .input_schema(serde_json::json!(42))
            .with_number_param("x", "a number", false);

        assert!(tool.input_schema.is_object());
        assert_eq!(tool.input_schema["properties"]["x"]["type"], "number");
    }

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
    fn test_output_schema_and_structured_content_serde() {
        // Omitted by default.
        let tool = Tool::new("t");
        let j = serde_json::to_value(&tool).unwrap();
        assert!(j.get("outputSchema").is_none());

        // Present + camelCase when set.
        let tool = Tool::new("t").output_schema(serde_json::json!({"type": "object"}));
        let j = serde_json::to_value(&tool).unwrap();
        assert_eq!(j["outputSchema"], serde_json::json!({"type": "object"}));

        let res = CallToolResult::text("ok").with_structured_content(
            serde_json::from_value(serde_json::json!({"answer": 42})).expect("object"),
        );
        let j = serde_json::to_value(&res).unwrap();
        assert_eq!(j["structuredContent"], serde_json::json!({"answer": 42}));
        // Round-trips and stays absent when unset.
        let plain: CallToolResult = serde_json::from_str(r#"{"content":[]}"#).expect("deserialize");
        assert!(plain.structured_content.is_none());
    }

    #[test]
    fn test_tool_title_icons_execution_serde() {
        use super::super::metadata::{Icon, IconTheme};

        // Absent by default.
        let tool = Tool::new("t");
        let j = serde_json::to_value(&tool).unwrap();
        assert!(j.get("title").is_none());
        assert!(j.get("icons").is_none());
        assert!(j.get("execution").is_none());

        let tool = Tool::new("t")
            .title("Pretty Name")
            .icon(
                Icon::new("https://e/i.png")
                    .mime_type("image/png")
                    .theme(IconTheme::Light),
            )
            .task_support(TaskSupport::Optional);
        let j = serde_json::to_value(&tool).unwrap();
        assert_eq!(j["title"], "Pretty Name");
        assert_eq!(j["icons"][0]["src"], "https://e/i.png");
        assert_eq!(j["icons"][0]["theme"], "light");
        // `execution.taskSupport` is camelCase and lowercase-valued.
        assert_eq!(j["execution"]["taskSupport"], "optional");

        // Round-trips.
        let back: Tool = serde_json::from_value(j).unwrap();
        assert_eq!(back.title.as_deref(), Some("Pretty Name"));
        assert_eq!(
            back.execution.and_then(|e| e.task_support),
            Some(TaskSupport::Optional)
        );
    }

    #[test]
    fn test_task_support_serde_values() {
        assert_eq!(
            serde_json::to_value(TaskSupport::Forbidden).unwrap(),
            serde_json::json!("forbidden")
        );
        assert_eq!(
            serde_json::to_value(TaskSupport::Required).unwrap(),
            serde_json::json!("required")
        );
        assert_eq!(TaskSupport::default(), TaskSupport::Forbidden);
    }

    #[test]
    fn test_tool_result_deserializes_without_content() {
        // A peer may send an empty result with no `content` field.
        let empty: CallToolResult = serde_json::from_str("{}").expect("empty object");
        assert!(empty.content.is_empty());
        assert_eq!(empty.is_error, None);

        let only_flag: CallToolResult =
            serde_json::from_str(r#"{"isError":true}"#).expect("isError only");
        assert!(only_flag.content.is_empty());
        assert_eq!(only_flag.is_error, Some(true));
    }

    #[test]
    fn test_tool_output_conversion() -> Result<(), Box<dyn std::error::Error>> {
        let output = ToolOutput::text("Success");
        let result: CallToolResult = output.into();
        assert!(!result.is_error());

        let output = ToolOutput::error_with_suggestion(
            "Invalid query",
            "Try using quotation marks around phrases",
        );
        let result: CallToolResult = output.into();
        assert!(result.is_error());
        assert!(
            result.content[0]
                .as_text()
                .ok_or("Expected text")?
                .contains("Suggestion")
        );
        Ok(())
    }

    #[test]
    fn test_tool_output_from_string() -> Result<(), Box<dyn std::error::Error>> {
        // From<String>
        let output: ToolOutput = "Hello, world!".to_string().into();
        let result: CallToolResult = output.into();
        assert!(!result.is_error());
        assert_eq!(
            result.content[0].as_text().ok_or("Expected text")?,
            "Hello, world!"
        );

        // From<&str>
        let output: ToolOutput = "Hello again!".into();
        let result: CallToolResult = output.into();
        assert!(!result.is_error());
        assert_eq!(
            result.content[0].as_text().ok_or("Expected text")?,
            "Hello again!"
        );
        Ok(())
    }

    #[test]
    fn test_tool_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let tool = Tool::new("test")
            .description("A test tool")
            .input_schema(serde_json::json!({"type": "object"}));

        let json = serde_json::to_string(&tool)?;
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"inputSchema\""));
        Ok(())
    }
}
