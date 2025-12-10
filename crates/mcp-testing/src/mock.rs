//! Mock implementations for testing.
//!
//! This module provides mock servers and tools that can be used in unit tests.
//! The mocks are designed to be flexible and configurable.

use mcp_core::capability::{ServerCapabilities, ServerInfo};
use mcp_core::error::McpError;
use mcp_core::types::{
    Content, GetPromptResult, Prompt, PromptMessage, Resource, ResourceContents,
    Tool, ToolAnnotations, ToolOutput,
};
use mcp_server::{
    Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler,
};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// A mock tool with configurable behavior.
pub struct MockTool {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: Option<String>,
    /// Input schema.
    pub input_schema: Value,
    /// Annotations.
    pub annotations: Option<ToolAnnotations>,
    /// Response to return.
    pub response: MockResponse,
}

/// Type of response a mock tool should return.
#[derive(Clone)]
pub enum MockResponse {
    /// Return a successful text response.
    Text(String),
    /// Return a successful JSON response.
    Json(Value),
    /// Return an error.
    Error(String),
    /// Return a dynamic response based on input.
    Dynamic(Arc<dyn Fn(Value) -> Result<ToolOutput, McpError> + Send + Sync>),
}

impl MockTool {
    /// Create a new mock tool.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            annotations: None,
            response: MockResponse::Text("OK".to_string()),
        }
    }

    /// Set the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the input schema.
    pub fn input_schema(mut self, schema: Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Set annotations.
    pub fn annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = Some(annotations);
        self
    }

    /// Set the tool to return a text response.
    pub fn returns_text(mut self, text: impl Into<String>) -> Self {
        self.response = MockResponse::Text(text.into());
        self
    }

    /// Set the tool to return a JSON response.
    pub fn returns_json(mut self, json: Value) -> Self {
        self.response = MockResponse::Json(json);
        self
    }

    /// Set the tool to return an error.
    pub fn returns_error(mut self, message: impl Into<String>) -> Self {
        self.response = MockResponse::Error(message.into());
        self
    }

    /// Set a dynamic handler.
    pub fn handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(Value) -> Result<ToolOutput, McpError> + Send + Sync + 'static,
    {
        self.response = MockResponse::Dynamic(Arc::new(handler));
        self
    }

    /// Convert to a Tool definition.
    pub fn to_tool(&self) -> Tool {
        Tool {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            annotations: self.annotations.clone(),
        }
    }

    /// Call the tool.
    pub fn call(&self, args: Value) -> Result<ToolOutput, McpError> {
        match &self.response {
            MockResponse::Text(text) => Ok(ToolOutput::text(text.clone())),
            MockResponse::Json(json) => Ok(ToolOutput::text(serde_json::to_string_pretty(json)?)),
            MockResponse::Error(msg) => Ok(ToolOutput::error(msg.clone())),
            MockResponse::Dynamic(f) => f(args),
        }
    }
}

/// A mock resource.
pub struct MockResource {
    /// Resource URI.
    pub uri: String,
    /// Resource name.
    pub name: String,
    /// Resource description.
    pub description: Option<String>,
    /// MIME type.
    pub mime_type: Option<String>,
    /// Resource content.
    pub content: String,
}

impl MockResource {
    /// Create a new mock resource.
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            content: String::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Set the content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Convert to a Resource definition.
    pub fn to_resource(&self) -> Resource {
        Resource {
            uri: self.uri.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            mime_type: self.mime_type.clone(),
            size: Some(self.content.len() as u64),
            annotations: None,
        }
    }

    /// Get the resource contents.
    pub fn to_contents(&self) -> ResourceContents {
        ResourceContents {
            uri: self.uri.clone(),
            mime_type: self.mime_type.clone(),
            text: Some(self.content.clone()),
            blob: None,
        }
    }
}

/// A mock prompt.
pub struct MockPrompt {
    /// Prompt name.
    pub name: String,
    /// Prompt description.
    pub description: Option<String>,
    /// Message template.
    pub template: String,
}

impl MockPrompt {
    /// Create a new mock prompt.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            template: String::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the message template.
    pub fn template(mut self, template: impl Into<String>) -> Self {
        self.template = template.into();
        self
    }

    /// Convert to a Prompt definition.
    pub fn to_prompt(&self) -> Prompt {
        Prompt {
            name: self.name.clone(),
            description: self.description.clone(),
            arguments: None,
        }
    }
}

/// Builder for constructing mock servers.
pub struct MockServerBuilder {
    name: String,
    version: String,
    tools: Vec<MockTool>,
    resources: Vec<MockResource>,
    prompts: Vec<MockPrompt>,
}

impl Default for MockServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockServerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            name: "mock-server".to_string(),
            version: "1.0.0".to_string(),
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
        }
    }

    /// Set the server name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the server version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add a mock tool.
    pub fn tool(mut self, tool: MockTool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add multiple mock tools.
    pub fn tools(mut self, tools: impl IntoIterator<Item = MockTool>) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Add a mock resource.
    pub fn resource(mut self, resource: MockResource) -> Self {
        self.resources.push(resource);
        self
    }

    /// Add a mock prompt.
    pub fn prompt(mut self, prompt: MockPrompt) -> Self {
        self.prompts.push(prompt);
        self
    }

    /// Build the mock server.
    pub fn build(self) -> MockServer {
        let tools: HashMap<String, MockTool> = self
            .tools
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect();

        let resources: HashMap<String, MockResource> = self
            .resources
            .into_iter()
            .map(|r| (r.uri.clone(), r))
            .collect();

        let prompts: HashMap<String, MockPrompt> = self
            .prompts
            .into_iter()
            .map(|p| (p.name.clone(), p))
            .collect();

        MockServer {
            name: self.name,
            version: self.version,
            tools,
            resources,
            prompts,
        }
    }
}

/// A mock MCP server for testing.
///
/// The mock server implements all handler traits and can be used
/// with MemoryTransport for testing.
pub struct MockServer {
    name: String,
    version: String,
    tools: HashMap<String, MockTool>,
    resources: HashMap<String, MockResource>,
    prompts: HashMap<String, MockPrompt>,
}

impl MockServer {
    /// Create a new builder.
    pub fn builder() -> MockServerBuilder {
        MockServerBuilder::new()
    }

    /// Create a simple mock server.
    pub fn new() -> MockServerBuilder {
        MockServerBuilder::new()
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the server version.
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl ServerHandler for MockServer {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new(&self.name, &self.version)
    }

    fn capabilities(&self) -> ServerCapabilities {
        let mut caps = ServerCapabilities::new();
        if !self.tools.is_empty() {
            caps = caps.with_tools();
        }
        if !self.resources.is_empty() {
            caps = caps.with_resources();
        }
        if !self.prompts.is_empty() {
            caps = caps.with_prompts();
        }
        caps
    }
}

impl ToolHandler for MockServer {
    fn list_tools(
        &self,
        _ctx: &Context,
    ) -> impl Future<Output = Result<Vec<Tool>, McpError>> + Send {
        let tools: Vec<Tool> = self.tools.values().map(MockTool::to_tool).collect();
        async move { Ok(tools) }
    }

    fn call_tool(
        &self,
        name: &str,
        args: Value,
        _ctx: &Context,
    ) -> impl Future<Output = Result<ToolOutput, McpError>> + Send {
        let result = if let Some(tool) = self.tools.get(name) {
            tool.call(args)
        } else {
            Err(McpError::method_not_found_with_suggestions(
                name,
                self.tools.keys().cloned().collect(),
            ))
        };
        async move { result }
    }
}

impl ResourceHandler for MockServer {
    fn list_resources(
        &self,
        _ctx: &Context,
    ) -> impl Future<Output = Result<Vec<Resource>, McpError>> + Send {
        let resources: Vec<Resource> = self
            .resources
            .values()
            .map(MockResource::to_resource)
            .collect();
        async move { Ok(resources) }
    }

    fn read_resource(
        &self,
        uri: &str,
        _ctx: &Context,
    ) -> impl Future<Output = Result<Vec<ResourceContents>, McpError>> + Send {
        let result = if let Some(resource) = self.resources.get(uri) {
            Ok(vec![resource.to_contents()])
        } else {
            Err(McpError::resource_not_found(uri))
        };
        async move { result }
    }
}

impl PromptHandler for MockServer {
    fn list_prompts(
        &self,
        _ctx: &Context,
    ) -> impl Future<Output = Result<Vec<Prompt>, McpError>> + Send {
        let prompts: Vec<Prompt> = self.prompts.values().map(MockPrompt::to_prompt).collect();
        async move { Ok(prompts) }
    }

    fn get_prompt(
        &self,
        name: &str,
        _args: Option<serde_json::Map<String, Value>>,
        _ctx: &Context,
    ) -> impl Future<Output = Result<GetPromptResult, McpError>> + Send {
        let result = if let Some(prompt) = self.prompts.get(name) {
            Ok(GetPromptResult {
                description: prompt.description.clone(),
                messages: vec![PromptMessage {
                    role: mcp_core::types::Role::User,
                    content: Content::text(&prompt.template),
                }],
            })
        } else {
            Err(McpError::method_not_found_with_suggestions(
                name,
                self.prompts.keys().cloned().collect(),
            ))
        };
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_tool_text() {
        let tool = MockTool::new("greet").returns_text("Hello!");
        let result = tool.call(serde_json::json!({})).unwrap();
        match result {
            ToolOutput::Success(r) => {
                assert!(!r.is_error());
            }
            ToolOutput::RecoverableError { .. } => panic!("Expected success"),
        }
    }

    #[test]
    fn test_mock_tool_error() {
        let tool = MockTool::new("fail").returns_error("Something went wrong");
        let result = tool.call(serde_json::json!({})).unwrap();
        match result {
            ToolOutput::RecoverableError { message, .. } => {
                assert!(message.contains("went wrong"));
            }
            ToolOutput::Success(_) => panic!("Expected error"),
        }
    }

    #[test]
    fn test_mock_tool_dynamic() {
        let tool = MockTool::new("add").handler(|args| {
            let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Ok(ToolOutput::text(format!("{}", a + b)))
        });

        let result = tool.call(serde_json::json!({"a": 1, "b": 2})).unwrap();
        match result {
            ToolOutput::Success(r) => {
                if let Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "3");
                }
            }
            ToolOutput::RecoverableError { .. } => panic!("Expected success"),
        }
    }

    #[test]
    fn test_mock_server_builder() {
        let server = MockServer::new()
            .name("test-server")
            .version("2.0.0")
            .tool(MockTool::new("test").returns_text("ok"))
            .resource(
                MockResource::new("test://resource", "Test Resource").content("Test content"),
            )
            .build();

        assert_eq!(server.name(), "test-server");
        assert_eq!(server.version(), "2.0.0");

        let caps = server.capabilities();
        assert!(caps.has_tools());
        assert!(caps.has_resources());
        assert!(!caps.has_prompts());
    }
}
