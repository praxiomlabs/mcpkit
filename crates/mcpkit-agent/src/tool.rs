//! Tool trait and implementations for agent tool use.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::{AgentError, AgentResult};

/// Definition of a tool's schema for LLM consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// The name of the tool.
    pub name: String,
    /// A description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool's input parameters.
    pub parameters: serde_json::Value,
}

impl ToolSchema {
    /// Create a new tool schema.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    /// Set the parameters schema.
    #[must_use]
    pub fn parameters(mut self, schema: serde_json::Value) -> Self {
        self.parameters = schema;
        self
    }

    /// Add a parameter to the schema.
    #[must_use]
    pub fn add_parameter(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        let name = name.into();

        if let Some(props) = self.parameters.get_mut("properties") {
            if let Some(obj) = props.as_object_mut() {
                obj.insert(
                    name.clone(),
                    serde_json::json!({
                        "type": param_type.into(),
                        "description": description.into()
                    }),
                );
            }
        }

        if required {
            if let Some(req) = self.parameters.get_mut("required") {
                if let Some(arr) = req.as_array_mut() {
                    arr.push(serde_json::Value::String(name));
                }
            }
        }

        self
    }
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// The output content.
    pub content: String,
    /// Whether the execution was successful.
    pub success: bool,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ToolOutput {
    /// Create a successful output.
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            success: true,
            data: None,
        }
    }

    /// Create an error output.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            success: false,
            data: None,
        }
    }

    /// Add structured data to the output.
    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Trait for tools that can be executed by an agent.
///
/// Tools provide capabilities that agents can use to interact with the
/// environment, fetch data, or perform actions.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_agent::{Tool, ToolSchema, ToolOutput, AgentResult};
/// use async_trait::async_trait;
///
/// struct Calculator;
///
/// #[async_trait]
/// impl Tool for Calculator {
///     fn schema(&self) -> ToolSchema {
///         ToolSchema::new("calculator", "Perform basic arithmetic")
///             .add_parameter("expression", "string", "The math expression to evaluate", true)
///     }
///
///     async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
///         let expr = input.get("expression")
///             .and_then(|v| v.as_str())
///             .ok_or_else(|| AgentError::custom("Missing expression"))?;
///
///         // Evaluate expression...
///         Ok(ToolOutput::success("42"))
///     }
/// }
/// ```
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool's schema for LLM consumption.
    fn schema(&self) -> ToolSchema;

    /// Execute the tool with the given input.
    async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput>;

    /// Get the tool's name.
    fn name(&self) -> &str {
        // Default implementation extracts from schema
        // Implementations should override for efficiency
        "tool"
    }
}

/// A tool created from a function.
pub struct FnTool<F>
where
    F: Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = AgentResult<ToolOutput>> + Send>>
        + Send
        + Sync,
{
    schema: ToolSchema,
    func: F,
}

impl<F> FnTool<F>
where
    F: Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = AgentResult<ToolOutput>> + Send>>
        + Send
        + Sync,
{
    /// Create a new function-based tool.
    pub fn new(schema: ToolSchema, func: F) -> Self {
        Self { schema, func }
    }
}

#[async_trait]
impl<F> Tool for FnTool<F>
where
    F: Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = AgentResult<ToolOutput>> + Send>>
        + Send
        + Sync,
{
    fn schema(&self) -> ToolSchema {
        self.schema.clone()
    }

    async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
        (self.func)(input).await
    }

    fn name(&self) -> &str {
        &self.schema.name
    }
}

/// Helper macro to create a tool from an async closure.
#[macro_export]
macro_rules! tool_fn {
    ($schema:expr, $closure:expr) => {
        $crate::FnTool::new($schema, move |input| Box::pin($closure(input)))
    };
}

/// A registry of tools available to an agent.
#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.schema().name;
        self.tools.insert(name, Arc::new(tool));
    }

    /// Get a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Check if a tool exists.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all tool schemas.
    #[must_use]
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    /// Get all tool names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    /// Get the number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Execute a tool by name.
    pub async fn execute(&self, name: &str, input: serde_json::Value) -> AgentResult<ToolOutput> {
        match self.get(name) {
            Some(tool) => tool.execute(input).await,
            None => Err(AgentError::tool_not_found(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn schema(&self) -> ToolSchema {
            ToolSchema::new("echo", "Echo the input back")
                .add_parameter("message", "string", "The message to echo", true)
        }

        async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
            let message = input
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            Ok(ToolOutput::success(format!("Echo: {message}")))
        }

        fn name(&self) -> &str {
            "echo"
        }
    }

    #[test]
    fn test_tool_schema() {
        let schema = ToolSchema::new("test", "A test tool")
            .add_parameter("input", "string", "The input", true)
            .add_parameter("optional", "number", "An optional number", false);

        assert_eq!(schema.name, "test");
        assert_eq!(schema.description, "A test tool");

        let props = schema.parameters.get("properties").unwrap();
        assert!(props.get("input").is_some());
        assert!(props.get("optional").is_some());

        let required = schema.parameters.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::Value::String("input".to_string())));
        assert!(!required.contains(&serde_json::Value::String("optional".to_string())));
    }

    #[tokio::test]
    async fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        assert!(registry.contains("echo"));
        assert_eq!(registry.len(), 1);

        let result = registry
            .execute("echo", serde_json::json!({"message": "hello"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.content, "Echo: hello");
    }

    #[tokio::test]
    async fn test_tool_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", serde_json::json!({})).await;

        assert!(matches!(result, Err(AgentError::ToolNotFound { .. })));
    }

    #[test]
    fn test_tool_output() {
        let success = ToolOutput::success("result");
        assert!(success.success);
        assert_eq!(success.content, "result");

        let error = ToolOutput::error("failed");
        assert!(!error.success);
        assert_eq!(error.content, "failed");

        let with_data = ToolOutput::success("ok").with_data(serde_json::json!({"key": "value"}));
        assert!(with_data.data.is_some());
    }
}
