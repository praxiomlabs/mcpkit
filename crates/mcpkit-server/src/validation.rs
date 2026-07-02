//! Opt-in JSON Schema validation of tool inputs and outputs.
//!
//! The MCP Tools spec requires servers to validate `tools/call` arguments
//! against each tool's `inputSchema`, and—when a tool declares an
//! `outputSchema`—to return `structuredContent` conforming to it. mcpkit's
//! generic [`ToolHandler`] path is an unchecked escape hatch: it receives raw
//! JSON and returns arbitrary JSON. This module provides an **opt-in**
//! [`ValidatingToolHandler`] decorator that enforces those schemas.
//!
//! Because it wraps the [`ToolHandler`] itself, it covers every dispatch path
//! uniformly—normal `tools/call`, task-augmented background execution, and the
//! HTTP adapters—since they all funnel through [`ToolHandler::call_tool`].
//!
//! Per the spec's error-handling section, arguments that fail a tool's
//! `inputSchema` are reported as a tool-execution error (`isError: true`), not a
//! JSON-RPC protocol error; malformed request envelopes and unknown tools remain
//! protocol errors and are handled upstream. An `outputSchema` violation is a
//! server-side bug: it is logged, the invalid `structuredContent` is dropped, and
//! the call returns `isError: true`.
//!
//! This module is gated behind the `schema-validation` feature.

use crate::context::Context;
use crate::handler::{PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, ResourceTemplate, Tool,
    ToolOutput,
};
use serde_json::Value;
use std::future::Future;

/// Which directions [`ValidatingToolHandler`] enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidationMode {
    /// Validate `tools/call` arguments against each tool's `inputSchema`.
    pub inputs: bool,
    /// Validate structured results against each tool's `outputSchema`.
    pub outputs: bool,
}

impl ValidationMode {
    /// Validate both inputs and outputs.
    #[must_use]
    pub const fn both() -> Self {
        Self {
            inputs: true,
            outputs: true,
        }
    }

    /// Validate inputs only.
    #[must_use]
    pub const fn inputs_only() -> Self {
        Self {
            inputs: true,
            outputs: false,
        }
    }

    /// Validate outputs only.
    #[must_use]
    pub const fn outputs_only() -> Self {
        Self {
            inputs: false,
            outputs: true,
        }
    }
}

/// A [`ToolHandler`] decorator that validates tool inputs and/or outputs against
/// their declared JSON Schemas.
///
/// Wrap a tool handler with [`ServerBuilder::validate_tool_io`] (or construct
/// directly with [`ValidatingToolHandler::new`] for adapter users who don't build
/// through [`Server`]). Schemas are resolved from the inner handler's
/// [`list_tools`](ToolHandler::list_tools) at call time; there is no cache, so a
/// dynamic tool list is always seen correctly.
///
/// [`ServerBuilder::validate_tool_io`]: crate::builder::ServerBuilder::validate_tool_io
/// [`Server`]: crate::builder::Server
pub struct ValidatingToolHandler<H> {
    inner: H,
    mode: ValidationMode,
}

impl<H> ValidatingToolHandler<H> {
    /// Wrap `inner`, enforcing the directions selected by `mode`.
    #[must_use]
    pub const fn new(inner: H, mode: ValidationMode) -> Self {
        Self { inner, mode }
    }

    /// Unwrap, returning the inner handler.
    pub fn into_inner(self) -> H {
        self.inner
    }
}

/// Validate `instance` against JSON Schema `schema`, returning the list of
/// violation messages (empty `Ok` means valid).
///
/// The schema's draft is auto-detected, defaulting to 2020-12 when no `$schema`
/// is present (matching MCP). String `format` assertions are **not** enforced
/// (the default for this validator). If `schema` itself cannot be compiled,
/// validation is skipped and `Ok(())` is returned after logging a warning—a
/// malformed schema is a server configuration bug and must not break tool calls.
///
/// # Errors
///
/// Returns the collected violation messages when `instance` does not conform.
pub fn validate_json(schema: &Value, instance: &Value) -> Result<(), Vec<String>> {
    match collect_errors(schema, instance) {
        None => Ok(()),
        Some(errors) => Err(errors),
    }
}

/// `None` = valid (or schema uncompilable → skipped); `Some` = violations.
fn collect_errors(schema: &Value, instance: &Value) -> Option<Vec<String>> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(validator) => validator,
        Err(error) => {
            tracing::warn!(%error, "tool schema failed to compile; skipping validation");
            return None;
        }
    };
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| e.to_string())
        .collect();
    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

impl<H: ToolHandler> ToolHandler for ValidatingToolHandler<H> {
    async fn list_tools(&self, ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        self.inner.list_tools(ctx).await
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        // Resolve this tool's declared schemas. If the list can't be fetched or
        // the tool isn't found, skip validation and let the inner handler decide
        // (an unknown tool stays a protocol error upstream).
        let tool = match self.inner.list_tools(ctx).await {
            Ok(tools) => tools.into_iter().find(|t| t.name == name),
            Err(error) => {
                tracing::warn!(%error, tool = name, "could not list tools; skipping validation");
                None
            }
        };

        if self.mode.inputs {
            if let Some(tool) = &tool {
                if let Some(errors) = collect_errors(&tool.input_schema, &args) {
                    let message = format!(
                        "Input does not conform to the tool's inputSchema:\n{}",
                        errors.join("\n")
                    );
                    return Ok(ToolOutput::Success(CallToolResult::error(message)));
                }
            }
        }

        let output = self.inner.call_tool(name, args, ctx).await?;

        if self.mode.outputs {
            if let (Some(tool), ToolOutput::Success(result)) = (&tool, &output) {
                if let (Some(schema), Some(structured)) =
                    (&tool.output_schema, &result.structured_content)
                {
                    if let Some(errors) = collect_errors(schema, structured) {
                        tracing::error!(
                            tool = name,
                            ?errors,
                            "tool output violates its declared outputSchema (server bug); \
                             dropping structuredContent"
                        );
                        let message = format!(
                            "The tool produced structured output that does not conform to its \
                             declared outputSchema:\n{}",
                            errors.join("\n")
                        );
                        return Ok(ToolOutput::Success(CallToolResult::error(message)));
                    }
                }
            }
        }

        Ok(output)
    }

    async fn on_tools_changed(&self) {
        self.inner.on_tools_changed().await;
    }
}

// Transparent forwarding of the other handler traits, so a single combined
// handler wrapped in `ValidatingToolHandler` remains a drop-in for the HTTP
// adapters (which require `ServerHandler + ToolHandler + ResourceHandler +
// PromptHandler` on one type). Only `ToolHandler` is intercepted above.

impl<H: ServerHandler> ServerHandler for ValidatingToolHandler<H> {
    fn server_info(&self) -> ServerInfo {
        self.inner.server_info()
    }

    fn capabilities(&self) -> ServerCapabilities {
        self.inner.capabilities()
    }

    fn instructions(&self) -> Option<String> {
        self.inner.instructions()
    }

    fn on_initialized(&self, ctx: &Context<'_>) -> impl Future<Output = ()> + Send {
        self.inner.on_initialized(ctx)
    }

    fn on_shutdown(&self) -> impl Future<Output = ()> + Send {
        self.inner.on_shutdown()
    }

    fn set_log_level(
        &self,
        level: crate::handler::LogLevel,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<(), McpError>> + Send {
        self.inner.set_log_level(level, ctx)
    }
}

impl<H: ResourceHandler> ResourceHandler for ValidatingToolHandler<H> {
    fn list_resources(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Resource>, McpError>> + Send {
        self.inner.list_resources(ctx)
    }

    fn list_resource_templates(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceTemplate>, McpError>> + Send {
        self.inner.list_resource_templates(ctx)
    }

    fn read_resource(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<ResourceContents>, McpError>> + Send {
        self.inner.read_resource(uri, ctx)
    }

    fn subscribe(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        self.inner.subscribe(uri, ctx)
    }

    fn unsubscribe(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<bool, McpError>> + Send {
        self.inner.unsubscribe(uri, ctx)
    }
}

impl<H: PromptHandler> PromptHandler for ValidatingToolHandler<H> {
    fn list_prompts(
        &self,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<Vec<Prompt>, McpError>> + Send {
        self.inner.list_prompts(ctx)
    }

    fn get_prompt(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, Value>>,
        ctx: &Context<'_>,
    ) -> impl Future<Output = Result<GetPromptResult, McpError>> + Send {
        self.inner.get_prompt(name, args, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::NoOpPeer;
    use crate::router::route_tools;
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;
    use serde_json::json;

    /// A tool "add" declaring an inputSchema (`{ n: number }`, required) and an
    /// outputSchema (`{ doubled: number }`, required). `call_tool` echoes back a
    /// configurable `structuredContent` so tests can drive output validation.
    struct SchemaHandler {
        structured: Value,
    }

    impl ToolHandler for SchemaHandler {
        async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
            Ok(vec![
                Tool::new("add")
                    .input_schema(json!({
                        "type": "object",
                        "properties": { "n": { "type": "number" } },
                        "required": ["n"]
                    }))
                    .output_schema(json!({
                        "type": "object",
                        "properties": { "doubled": { "type": "number" } },
                        "required": ["doubled"]
                    })),
            ])
        }

        async fn call_tool(
            &self,
            _name: &str,
            _args: Value,
            _ctx: &Context<'_>,
        ) -> Result<ToolOutput, McpError> {
            Ok(ToolOutput::Success(
                CallToolResult::text("ok").with_structured_content(self.structured.clone()),
            ))
        }
    }

    /// Run `f` with a throwaway `Context`.
    async fn with_ctx<F, Fut, T>(f: F) -> T
    where
        F: FnOnce(Context<'static>) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        // Leak the borrows for the duration of the test: simplest way to hand a
        // `Context` into an async closure without lifetime gymnastics.
        let request_id: &'static RequestId = Box::leak(Box::new(request_id));
        let client_caps: &'static ClientCapabilities = Box::leak(Box::new(client_caps));
        let server_caps: &'static ServerCapabilities = Box::leak(Box::new(server_caps));
        let peer: &'static NoOpPeer = Box::leak(Box::new(peer));
        let ctx = Context::new(
            request_id,
            None,
            client_caps,
            server_caps,
            ProtocolVersion::LATEST,
            peer,
        );
        f(ctx).await
    }

    #[tokio::test]
    async fn input_failure_is_a_tool_error_not_protocol_error() {
        let handler = SchemaHandler {
            structured: json!({ "doubled": 84 }),
        };
        let validating = ValidatingToolHandler::new(handler, ValidationMode::both());
        let out = with_ctx(|ctx| async move {
            // Missing required "n" -> fails inputSchema.
            validating
                .call_tool("add", json!({}), &ctx)
                .await
                .expect("input failure is Ok(isError), not a protocol Err")
        })
        .await;
        match out {
            ToolOutput::Success(result) => assert!(result.is_error(), "expected isError: true"),
            other => panic!("expected a Success(isError) result, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn valid_input_and_output_pass_through_untouched() {
        let handler = SchemaHandler {
            structured: json!({ "doubled": 84 }),
        };
        let validating = ValidatingToolHandler::new(handler, ValidationMode::both());
        let out = with_ctx(|ctx| async move {
            validating
                .call_tool("add", json!({ "n": 42 }), &ctx)
                .await
                .expect("routed")
        })
        .await;
        match out {
            ToolOutput::Success(result) => {
                assert!(!result.is_error());
                assert_eq!(result.structured_content, Some(json!({ "doubled": 84 })));
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn output_schema_violation_drops_structured_content() {
        let handler = SchemaHandler {
            // `doubled` must be a number; a string violates the outputSchema.
            structured: json!({ "doubled": "not a number" }),
        };
        let validating = ValidatingToolHandler::new(handler, ValidationMode::both());
        let out = with_ctx(|ctx| async move {
            validating
                .call_tool("add", json!({ "n": 42 }), &ctx)
                .await
                .expect("routed")
        })
        .await;
        match out {
            ToolOutput::Success(result) => {
                assert!(result.is_error(), "output violation must be isError: true");
                assert!(
                    result.structured_content.is_none(),
                    "invalid structuredContent must be dropped"
                );
            }
            other => panic!("expected success(isError), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn inputs_only_mode_ignores_bad_output() {
        let handler = SchemaHandler {
            structured: json!({ "doubled": "not a number" }),
        };
        let validating = ValidatingToolHandler::new(handler, ValidationMode::inputs_only());
        let out = with_ctx(|ctx| async move {
            validating
                .call_tool("add", json!({ "n": 42 }), &ctx)
                .await
                .expect("routed")
        })
        .await;
        match out {
            // outputs are not validated in inputs-only mode: bad structured passes.
            ToolOutput::Success(result) => assert!(!result.is_error()),
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn normal_tools_call_path_through_route_tools_is_validated() {
        let handler = SchemaHandler {
            structured: json!({ "doubled": 84 }),
        };
        let validating = ValidatingToolHandler::new(handler, ValidationMode::both());
        let result = with_ctx(|ctx| async move {
            route_tools(
                &validating,
                "tools/call",
                Some(&json!({ "name": "add", "arguments": {} })),
                &ctx,
                None,
            )
            .await
            .expect("tools/call is routed")
            .expect("ok result")
        })
        .await;
        assert_eq!(result["isError"], json!(true));
    }

    #[test]
    fn wrapped_combined_handler_satisfies_adapter_bounds() {
        // Compile-time proof of the adapter escape hatch: the HTTP adapters bound
        // a single handler on `ServerHandler + ToolHandler + ResourceHandler +
        // PromptHandler`. Wrapping such a handler must still satisfy that bound,
        // so `McpState::new(ValidatingToolHandler::new(h, mode))` type-checks.
        use crate::handler::{PromptHandler, ResourceHandler, ServerHandler};
        use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents};

        fn adapter_bound<H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler>(_h: &H) {
        }

        struct Combined;
        impl ServerHandler for Combined {
            fn server_info(&self) -> ServerInfo {
                ServerInfo::new("t", "1.0.0")
            }
        }
        impl ToolHandler for Combined {
            async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
                Ok(vec![])
            }
            async fn call_tool(
                &self,
                _name: &str,
                _args: Value,
                _ctx: &Context<'_>,
            ) -> Result<ToolOutput, McpError> {
                Ok(ToolOutput::text("x"))
            }
        }
        impl ResourceHandler for Combined {
            async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
                Ok(vec![])
            }
            async fn read_resource(
                &self,
                _uri: &str,
                _ctx: &Context<'_>,
            ) -> Result<Vec<ResourceContents>, McpError> {
                Ok(vec![])
            }
        }
        impl PromptHandler for Combined {
            async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
                Ok(vec![])
            }
            async fn get_prompt(
                &self,
                _name: &str,
                _args: Option<serde_json::Map<String, Value>>,
                _ctx: &Context<'_>,
            ) -> Result<GetPromptResult, McpError> {
                Ok(GetPromptResult {
                    description: None,
                    messages: vec![],
                    meta: None,
                })
            }
        }

        let wrapped = ValidatingToolHandler::new(Combined, ValidationMode::both());
        adapter_bound(&wrapped);
    }

    #[tokio::test]
    async fn unwrapped_handler_does_not_validate() {
        // The escape hatch: without the decorator, bad input is not rejected.
        // Proves validation is strictly opt-in (feature compiled in, not applied).
        let handler = SchemaHandler {
            structured: json!({ "doubled": 84 }),
        };
        let result = with_ctx(|ctx| async move {
            route_tools(
                &handler,
                "tools/call",
                Some(&json!({ "name": "add", "arguments": {} })),
                &ctx,
                None,
            )
            .await
            .expect("tools/call is routed")
            .expect("ok result")
        })
        .await;
        assert_ne!(result.get("isError"), Some(&json!(true)));
    }
}
