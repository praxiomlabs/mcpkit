//! Tool capability implementation.
//!
//! This module provides utilities for managing and executing tools
//! in an MCP server.

use crate::context::Context;
use crate::handler::ToolHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::tool::{Tool, ToolOutput};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A boxed async function for tool execution.
pub type BoxedToolFn = Box<
    dyn for<'a> Fn(
            Value,
            &'a Context<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<ToolOutput, McpError>> + Send + 'a>>
        + Send
        + Sync,
>;

/// A registered tool with metadata and handler.
pub struct RegisteredTool {
    /// Tool metadata.
    pub tool: Tool,
    /// Handler function.
    pub handler: BoxedToolFn,
}

/// Service for managing tools.
///
/// This provides a registry for tools and handles dispatching
/// tool calls to the appropriate handlers.
pub struct ToolService {
    tools: HashMap<String, RegisteredTool>,
}

impl Default for ToolService {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolService {
    /// Create a new empty tool service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool with a handler function.
    pub fn register<F, Fut>(&mut self, tool: Tool, handler: F)
    where
        F: Fn(Value, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolOutput, McpError>> + Send + 'static,
    {
        let name = tool.name.clone();
        let boxed: BoxedToolFn = Box::new(move |args, ctx| Box::pin(handler(args, ctx)));
        self.tools.insert(
            name,
            RegisteredTool {
                tool,
                handler: boxed,
            },
        );
    }

    /// Register a tool with an Arc'd handler (for shared state).
    pub fn register_arc<H>(&mut self, tool: Tool, handler: Arc<H>)
    where
        H: for<'a> Fn(
                Value,
                &'a Context<'a>,
            )
                -> Pin<Box<dyn Future<Output = Result<ToolOutput, McpError>> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        let name = tool.name.clone();
        let boxed: BoxedToolFn = Box::new(move |args, ctx| (handler)(args, ctx));
        self.tools.insert(
            name,
            RegisteredTool {
                tool,
                handler: boxed,
            },
        );
    }

    /// Get a tool by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.tools.get(name)
    }

    /// Check if a tool exists.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all registered tools.
    #[must_use]
    pub fn list(&self) -> Vec<&Tool> {
        self.tools.values().map(|r| &r.tool).collect()
    }

    /// Get the number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the service has no tools.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Call a tool by name.
    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        let registered = self.tools.get(name).ok_or_else(|| {
            McpError::invalid_params("tools/call", format!("Unknown tool: {name}"))
        })?;

        (registered.handler)(arguments, ctx).await
    }
}

impl ToolHandler for ToolService {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(self.list().into_iter().cloned().collect())
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        self.call(name, arguments, ctx).await
    }
}

/// Builder for creating tools with a fluent API.
pub struct ToolBuilder {
    name: String,
    description: Option<String>,
    input_schema: Value,
    destructive: Option<bool>,
    idempotent: Option<bool>,
    read_only: Option<bool>,
}

impl ToolBuilder {
    /// Create a new tool builder.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
            destructive: None,
            idempotent: None,
            read_only: None,
        }
    }

    /// Set the tool description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the input schema.
    #[must_use]
    pub fn input_schema(mut self, schema: Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Mark this tool as destructive.
    ///
    /// Destructive tools modify data or state in ways that cannot be easily undone.
    /// When set to true, clients should warn users before executing.
    #[must_use]
    pub fn destructive(mut self, value: bool) -> Self {
        self.destructive = Some(value);
        self
    }

    /// Mark this tool as idempotent.
    ///
    /// Idempotent tools produce the same result when called multiple times
    /// with the same arguments.
    #[must_use]
    pub fn idempotent(mut self, value: bool) -> Self {
        self.idempotent = Some(value);
        self
    }

    /// Mark this tool as read-only.
    ///
    /// Read-only tools do not modify any data or state.
    #[must_use]
    pub fn read_only(mut self, value: bool) -> Self {
        self.read_only = Some(value);
        self
    }

    /// Build the tool.
    #[must_use]
    pub fn build(self) -> Tool {
        let has_annotations =
            self.destructive.is_some() || self.idempotent.is_some() || self.read_only.is_some();

        let annotations = if has_annotations {
            Some(mcpkit_core::types::tool::ToolAnnotations {
                title: None,
                read_only_hint: self.read_only.or(Some(false)),
                destructive_hint: self.destructive.or(Some(false)),
                idempotent_hint: self.idempotent.or(Some(false)),
                open_world_hint: None,
            })
        } else {
            None
        };

        Tool {
            name: self.name,
            description: self.description,
            input_schema: self.input_schema,
            annotations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{Context, NoOpPeer};
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;
    use mcpkit_core::types::tool::CallToolResult;

    fn make_context() -> (
        RequestId,
        ClientCapabilities,
        ServerCapabilities,
        ProtocolVersion,
        NoOpPeer,
    ) {
        (
            RequestId::Number(1),
            ClientCapabilities::default(),
            ServerCapabilities::default(),
            ProtocolVersion::LATEST,
            NoOpPeer,
        )
    }

    #[test]
    fn test_tool_builder() {
        let tool = ToolBuilder::new("test")
            .description("A test tool")
            .input_schema(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }))
            .build();

        assert_eq!(tool.name, "test");
        assert_eq!(tool.description.as_deref(), Some("A test tool"));
    }

    #[tokio::test]
    async fn test_tool_service() {
        let mut service = ToolService::new();

        let tool = ToolBuilder::new("echo")
            .description("Echo back input")
            .build();

        service.register(tool, |args, _ctx| async move {
            Ok(ToolOutput::text(args.to_string()))
        });

        assert!(service.contains("echo"));
        assert_eq!(service.len(), 1);

        let (req_id, client_caps, server_caps, protocol_version, peer) = make_context();
        let ctx = Context::new(
            &req_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
        );

        let result = service
            .call("echo", serde_json::json!({"hello": "world"}), &ctx)
            .await
            .unwrap();

        // Convert to CallToolResult to check content
        let call_result: CallToolResult = result.into();
        assert!(!call_result.content.is_empty());
    }
}
