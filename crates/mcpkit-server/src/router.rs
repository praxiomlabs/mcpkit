//! Request routing for MCP servers.
//!
//! This module provides the routing infrastructure that dispatches
//! incoming requests to the appropriate handler methods.
//!
//! # MCP Method Categories
//!
//! - **Initialization**: `initialize`, `ping`
//! - **Tools**: `tools/list`, `tools/call`
//! - **Resources**: `resources/list`, `resources/read`, `resources/subscribe`
//! - **Prompts**: `prompts/list`, `prompts/get`
//! - **Tasks**: `tasks/list`, `tasks/get`, `tasks/cancel`
//! - **Sampling**: `sampling/createMessage`
//! - **Completions**: `completion/complete`

use mcpkit_core::error::McpError;
use mcpkit_core::protocol::Request;
use serde_json::Value;

/// Standard MCP method names as defined in the MCP specification.
pub mod methods {
    /// Initialize the connection and negotiate capabilities.
    pub const INITIALIZE: &str = "initialize";
    /// Ping to check if the connection is alive.
    pub const PING: &str = "ping";

    /// List available tools.
    pub const TOOLS_LIST: &str = "tools/list";
    /// Call a specific tool with arguments.
    pub const TOOLS_CALL: &str = "tools/call";

    /// List available resources.
    pub const RESOURCES_LIST: &str = "resources/list";
    /// Read the contents of a resource.
    pub const RESOURCES_READ: &str = "resources/read";
    /// List available resource templates.
    pub const RESOURCES_TEMPLATES_LIST: &str = "resources/templates/list";
    /// Subscribe to resource updates.
    pub const RESOURCES_SUBSCRIBE: &str = "resources/subscribe";
    /// Unsubscribe from resource updates.
    pub const RESOURCES_UNSUBSCRIBE: &str = "resources/unsubscribe";

    /// List available prompts.
    pub const PROMPTS_LIST: &str = "prompts/list";
    /// Get a specific prompt with arguments.
    pub const PROMPTS_GET: &str = "prompts/get";

    /// List running tasks.
    pub const TASKS_LIST: &str = "tasks/list";
    /// Get status of a specific task.
    pub const TASKS_GET: &str = "tasks/get";
    /// Cancel a running task.
    pub const TASKS_CANCEL: &str = "tasks/cancel";

    /// Request the client to sample from a language model.
    pub const SAMPLING_CREATE_MESSAGE: &str = "sampling/createMessage";

    /// Request completion suggestions.
    pub const COMPLETION_COMPLETE: &str = "completion/complete";

    /// Set the logging level.
    pub const LOGGING_SET_LEVEL: &str = "logging/setLevel";

    /// Create an elicitation request.
    pub const ELICITATION_CREATE: &str = "elicitation/create";
}

/// Standard MCP notification names as defined in the MCP specification.
pub mod notifications {
    /// Sent by client after successful initialization.
    pub const INITIALIZED: &str = "notifications/initialized";
    /// Sent when a request is cancelled.
    pub const CANCELLED: &str = "notifications/cancelled";
    /// Sent to report progress on a long-running operation.
    pub const PROGRESS: &str = "notifications/progress";
    /// Sent to deliver a log message.
    pub const MESSAGE: &str = "notifications/message";
    /// Sent when a resource's content has changed.
    pub const RESOURCES_UPDATED: &str = "notifications/resources/updated";
    /// Sent when the list of available resources has changed.
    pub const RESOURCES_LIST_CHANGED: &str = "notifications/resources/list_changed";
    /// Sent when the list of available tools has changed.
    pub const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
    /// Sent when the list of available prompts has changed.
    pub const PROMPTS_LIST_CHANGED: &str = "notifications/prompts/list_changed";
}

/// Represents a parsed MCP request with typed parameters.
///
/// This enum provides a type-safe representation of all MCP request types,
/// with parameters parsed into their appropriate structures.
#[derive(Debug)]
pub enum ParsedRequest {
    /// Initialize request to establish connection.
    Initialize(InitializeParams),
    /// Ping request to check connection health.
    Ping,

    /// Request to list available tools.
    ToolsList(ListParams),
    /// Request to call a specific tool.
    ToolsCall(ToolCallParams),

    /// Request to list available resources.
    ResourcesList(ListParams),
    /// Request to read a resource's contents.
    ResourcesRead(ResourceReadParams),
    /// Request to list resource templates.
    ResourcesTemplatesList(ListParams),
    /// Request to subscribe to resource updates.
    ResourcesSubscribe(ResourceSubscribeParams),
    /// Request to unsubscribe from resource updates.
    ResourcesUnsubscribe(ResourceUnsubscribeParams),

    /// Request to list available prompts.
    PromptsList(ListParams),
    /// Request to get a specific prompt.
    PromptsGet(PromptGetParams),

    /// Request to list running tasks.
    TasksList(ListParams),
    /// Request to get a task's status.
    TasksGet(TaskGetParams),
    /// Request to cancel a running task.
    TasksCancel(TaskCancelParams),

    /// Request for the client to sample from a language model.
    SamplingCreateMessage(SamplingParams),

    /// Request for completion suggestions.
    CompletionComplete(CompletionParams),

    /// Request to set the logging level.
    LoggingSetLevel(LogLevelParams),

    /// An unrecognized method name.
    Unknown(String),
}

/// Common list parameters with optional cursor for pagination.
#[derive(Debug, Default)]
pub struct ListParams {
    /// Optional cursor for pagination.
    pub cursor: Option<String>,
}

/// Initialize request parameters.
#[derive(Debug)]
pub struct InitializeParams {
    /// The protocol version requested by the client.
    pub protocol_version: String,
    /// Information about the client.
    pub client_info: ClientInfo,
    /// Client capabilities.
    pub capabilities: Value,
}

/// Client info from initialize request.
#[derive(Debug)]
pub struct ClientInfo {
    /// The name of the client application.
    pub name: String,
    /// The version of the client application.
    pub version: String,
}

/// Tool call parameters.
#[derive(Debug)]
pub struct ToolCallParams {
    /// The name of the tool to call.
    pub name: String,
    /// Arguments to pass to the tool.
    pub arguments: Value,
}

/// Resource read parameters.
#[derive(Debug)]
pub struct ResourceReadParams {
    /// The URI of the resource to read.
    pub uri: String,
}

/// Resource subscribe parameters.
#[derive(Debug)]
pub struct ResourceSubscribeParams {
    /// The URI of the resource to subscribe to.
    pub uri: String,
}

/// Resource unsubscribe parameters.
#[derive(Debug)]
pub struct ResourceUnsubscribeParams {
    /// The URI of the resource to unsubscribe from.
    pub uri: String,
}

/// Prompt get parameters.
#[derive(Debug)]
pub struct PromptGetParams {
    /// The name of the prompt to get.
    pub name: String,
    /// Optional arguments to pass to the prompt.
    pub arguments: Option<Value>,
}

/// Task get parameters.
#[derive(Debug)]
pub struct TaskGetParams {
    /// The ID of the task to get.
    pub task_id: String,
}

/// Task cancel parameters.
#[derive(Debug)]
pub struct TaskCancelParams {
    /// The ID of the task to cancel.
    pub task_id: String,
}

/// Sampling create message parameters.
#[derive(Debug)]
pub struct SamplingParams {
    /// The messages to sample from.
    pub messages: Vec<Value>,
    /// Optional model preferences.
    pub model_preferences: Option<Value>,
    /// Optional system prompt.
    pub system_prompt: Option<String>,
    /// Optional maximum number of tokens.
    pub max_tokens: Option<u32>,
}

/// Completion parameters.
#[derive(Debug)]
pub struct CompletionParams {
    /// The type of reference (e.g., "ref/resource", "ref/prompt").
    pub ref_type: String,
    /// The value of the reference (URI or name).
    pub ref_value: String,
    /// Optional argument for completion context.
    pub argument: Option<CompletionArgument>,
}

/// Completion argument providing context for completion.
#[derive(Debug)]
pub struct CompletionArgument {
    /// The name of the argument.
    pub name: String,
    /// The current value being completed.
    pub value: String,
}

/// Log level parameters.
#[derive(Debug)]
pub struct LogLevelParams {
    /// The log level to set (e.g., "debug", "info", "warn", "error").
    pub level: String,
}

/// Parse a request into a typed representation.
pub fn parse_request(request: &Request) -> Result<ParsedRequest, McpError> {
    let method = request.method.as_ref();
    let params = request.params.as_ref();

    match method {
        methods::INITIALIZE => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            Ok(ParsedRequest::Initialize(InitializeParams {
                protocol_version: params
                    .get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                client_info: ClientInfo {
                    name: params
                        .get("clientInfo")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    version: params
                        .get("clientInfo")
                        .and_then(|v| v.get("version"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                },
                capabilities: params
                    .get("capabilities")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
            }))
        }

        methods::PING => Ok(ParsedRequest::Ping),

        methods::TOOLS_LIST => Ok(ParsedRequest::ToolsList(parse_list_params(params))),

        methods::TOOLS_CALL => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing name"))?
                .to_string();

            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

            Ok(ParsedRequest::ToolsCall(ToolCallParams { name, arguments }))
        }

        methods::RESOURCES_LIST => Ok(ParsedRequest::ResourcesList(parse_list_params(params))),

        methods::RESOURCES_READ => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let uri = params
                .get("uri")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing uri"))?
                .to_string();

            Ok(ParsedRequest::ResourcesRead(ResourceReadParams { uri }))
        }

        methods::RESOURCES_TEMPLATES_LIST => Ok(ParsedRequest::ResourcesTemplatesList(
            parse_list_params(params),
        )),

        methods::RESOURCES_SUBSCRIBE => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let uri = params
                .get("uri")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing uri"))?
                .to_string();

            Ok(ParsedRequest::ResourcesSubscribe(ResourceSubscribeParams {
                uri,
            }))
        }

        methods::RESOURCES_UNSUBSCRIBE => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let uri = params
                .get("uri")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing uri"))?
                .to_string();

            Ok(ParsedRequest::ResourcesUnsubscribe(
                ResourceUnsubscribeParams { uri },
            ))
        }

        methods::PROMPTS_LIST => Ok(ParsedRequest::PromptsList(parse_list_params(params))),

        methods::PROMPTS_GET => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing name"))?
                .to_string();

            let arguments = params.get("arguments").cloned();

            Ok(ParsedRequest::PromptsGet(PromptGetParams {
                name,
                arguments,
            }))
        }

        methods::TASKS_LIST => Ok(ParsedRequest::TasksList(parse_list_params(params))),

        methods::TASKS_GET => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing taskId"))?
                .to_string();

            Ok(ParsedRequest::TasksGet(TaskGetParams { task_id }))
        }

        methods::TASKS_CANCEL => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let task_id = params
                .get("taskId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing taskId"))?
                .to_string();

            Ok(ParsedRequest::TasksCancel(TaskCancelParams { task_id }))
        }

        methods::SAMPLING_CREATE_MESSAGE => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let messages = params
                .get("messages")
                .and_then(|v| v.as_array())
                .ok_or_else(|| McpError::invalid_params(method, "missing messages"))?
                .clone();

            Ok(ParsedRequest::SamplingCreateMessage(SamplingParams {
                messages,
                model_preferences: params.get("modelPreferences").cloned(),
                system_prompt: params
                    .get("systemPrompt")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                max_tokens: params
                    .get("maxTokens")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as u32),
            }))
        }

        methods::COMPLETION_COMPLETE => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let ref_obj = params
                .get("ref")
                .ok_or_else(|| McpError::invalid_params(method, "missing ref"))?;

            Ok(ParsedRequest::CompletionComplete(CompletionParams {
                ref_type: ref_obj
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ref_value: ref_obj
                    .get("uri")
                    .or_else(|| ref_obj.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                argument: params.get("argument").map(|arg| CompletionArgument {
                    name: arg
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    value: arg
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
            }))
        }

        methods::LOGGING_SET_LEVEL => {
            let params =
                params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;

            let level = params
                .get("level")
                .and_then(|v| v.as_str())
                .ok_or_else(|| McpError::invalid_params(method, "missing level"))?
                .to_string();

            Ok(ParsedRequest::LoggingSetLevel(LogLevelParams { level }))
        }

        _ => Ok(ParsedRequest::Unknown(method.to_string())),
    }
}

/// Parse common list parameters.
fn parse_list_params(params: Option<&Value>) -> ListParams {
    ListParams {
        cursor: params
            .and_then(|p| p.get("cursor"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

// =============================================================================
// Public routing functions for HTTP integrations
//
// These functions allow HTTP handlers (axum, actix, etc.) to properly route
// requests to handler trait implementations.
// =============================================================================

use crate::context::Context;
use crate::handler::{PromptHandler, ResourceHandler, ToolHandler};
use mcpkit_core::types::CallToolResult;

/// Route tool-related requests to a handler implementing [`ToolHandler`].
///
/// This function handles `tools/list` and `tools/call` methods.
/// Returns `None` if the method is not tool-related.
///
/// # Example
///
/// ```ignore
/// if let Some(result) = route_tools(&handler, method, params, &ctx).await {
///     return result;
/// }
/// ```
pub async fn route_tools<TH: ToolHandler + Send + Sync>(
    handler: &TH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::TOOLS_LIST => {
            tracing::debug!("Listing available tools");
            let result = handler.list_tools(ctx).await;
            match &result {
                Ok(tools) => tracing::debug!(count = tools.len(), "Listed tools"),
                Err(e) => tracing::warn!(error = %e, "Failed to list tools"),
            }
            Some(result.map(|tools| serde_json::json!({ "tools": tools })))
        }
        methods::TOOLS_CALL => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::TOOLS_CALL, "missing params")
                })?;
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::invalid_params(methods::TOOLS_CALL, "missing tool name")
                })?;
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                tracing::info!(tool = %name, "Calling tool");
                let start = std::time::Instant::now();
                let output = handler.call_tool(name, args, ctx).await;
                let duration = start.elapsed();

                match &output {
                    Ok(_) => tracing::info!(
                        tool = %name,
                        duration_ms = duration.as_millis(),
                        "Tool call completed"
                    ),
                    Err(e) => tracing::warn!(
                        tool = %name,
                        duration_ms = duration.as_millis(),
                        error = %e,
                        "Tool call failed"
                    ),
                }

                let output = output?;
                let result: CallToolResult = output.into();
                Ok(serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
            }
            .await;
            Some(result)
        }
        _ => None,
    }
}

/// Route resource-related requests to a handler implementing [`ResourceHandler`].
///
/// This function handles `resources/list`, `resources/templates/list`, and `resources/read` methods.
/// Returns `None` if the method is not resource-related.
///
/// # Example
///
/// ```ignore
/// if let Some(result) = route_resources(&handler, method, params, &ctx).await {
///     return result;
/// }
/// ```
pub async fn route_resources<RH: ResourceHandler + Send + Sync>(
    handler: &RH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::RESOURCES_LIST => {
            tracing::debug!("Listing available resources");
            let result = handler.list_resources(ctx).await;
            match &result {
                Ok(resources) => tracing::debug!(count = resources.len(), "Listed resources"),
                Err(e) => tracing::warn!(error = %e, "Failed to list resources"),
            }
            Some(result.map(|resources| serde_json::json!({ "resources": resources })))
        }
        methods::RESOURCES_TEMPLATES_LIST => {
            tracing::debug!("Listing available resource templates");
            let result = handler.list_resource_templates(ctx).await;
            match &result {
                Ok(templates) => {
                    tracing::debug!(count = templates.len(), "Listed resource templates");
                }
                Err(e) => tracing::warn!(error = %e, "Failed to list resource templates"),
            }
            Some(result.map(|templates| serde_json::json!({ "resourceTemplates": templates })))
        }
        methods::RESOURCES_READ => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::RESOURCES_READ, "missing params")
                })?;
                let uri = params.get("uri").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::invalid_params(methods::RESOURCES_READ, "missing uri")
                })?;

                tracing::info!(uri = %uri, "Reading resource");
                let start = std::time::Instant::now();
                let contents = handler.read_resource(uri, ctx).await;
                let duration = start.elapsed();

                match &contents {
                    Ok(_) => tracing::info!(
                        uri = %uri,
                        duration_ms = duration.as_millis(),
                        "Resource read completed"
                    ),
                    Err(e) => tracing::warn!(
                        uri = %uri,
                        duration_ms = duration.as_millis(),
                        error = %e,
                        "Resource read failed"
                    ),
                }

                let contents = contents?;
                Ok(serde_json::json!({ "contents": contents }))
            }
            .await;
            Some(result)
        }
        _ => None,
    }
}

/// Route prompt-related requests to a handler implementing [`PromptHandler`].
///
/// This function handles `prompts/list` and `prompts/get` methods.
/// Returns `None` if the method is not prompt-related.
///
/// # Example
///
/// ```ignore
/// if let Some(result) = route_prompts(&handler, method, params, &ctx).await {
///     return result;
/// }
/// ```
pub async fn route_prompts<PH: PromptHandler + Send + Sync>(
    handler: &PH,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::PROMPTS_LIST => {
            tracing::debug!("Listing available prompts");
            let result = handler.list_prompts(ctx).await;
            match &result {
                Ok(prompts) => tracing::debug!(count = prompts.len(), "Listed prompts"),
                Err(e) => tracing::warn!(error = %e, "Failed to list prompts"),
            }
            Some(result.map(|prompts| serde_json::json!({ "prompts": prompts })))
        }
        methods::PROMPTS_GET => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::PROMPTS_GET, "missing params")
                })?;
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::invalid_params(methods::PROMPTS_GET, "missing prompt name")
                })?;
                let args = params.get("arguments").and_then(|v| v.as_object()).cloned();

                tracing::info!(prompt = %name, "Getting prompt");
                let start = std::time::Instant::now();
                let prompt_result = handler.get_prompt(name, args, ctx).await;
                let duration = start.elapsed();

                match &prompt_result {
                    Ok(_) => tracing::info!(
                        prompt = %name,
                        duration_ms = duration.as_millis(),
                        "Prompt retrieval completed"
                    ),
                    Err(e) => tracing::warn!(
                        prompt = %name,
                        duration_ms = duration.as_millis(),
                        error = %e,
                        "Prompt retrieval failed"
                    ),
                }

                let result = prompt_result?;
                Ok(serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({})))
            }
            .await;
            Some(result)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::protocol::Request;

    fn make_request(method: &'static str, params: Option<Value>) -> Request {
        if let Some(p) = params {
            Request::with_params(method, 1u64, p)
        } else {
            Request::new(method, 1u64)
        }
    }

    #[test]
    fn test_parse_ping() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("ping", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::Ping));

        Ok(())
    }

    #[test]
    fn test_parse_tools_list() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("tools/list", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::ToolsList(_)));

        Ok(())
    }

    #[test]
    fn test_parse_tools_list_with_cursor() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "tools/list",
            Some(serde_json::json!({ "cursor": "abc123" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ToolsList(params) = parsed {
            assert_eq!(params.cursor, Some("abc123".to_string()));
        } else {
            panic!("Expected ToolsList");
        }

        Ok(())
    }

    #[test]
    fn test_parse_tools_call() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "tools/call",
            Some(serde_json::json!({
                "name": "search",
                "arguments": {"query": "test"}
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ToolsCall(params) = parsed {
            assert_eq!(params.name, "search");
            assert_eq!(params.arguments["query"], "test");
        } else {
            panic!("Expected ToolsCall");
        }

        Ok(())
    }

    #[test]
    fn test_parse_tools_call_missing_params() {
        let request = make_request("tools/call", None);
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tools_call_missing_name() {
        let request = make_request("tools/call", Some(serde_json::json!({"arguments": {}})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unknown_method() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("unknown/method", None);
        let parsed = parse_request(&request)?;

        if let ParsedRequest::Unknown(method) = parsed {
            assert_eq!(method, "unknown/method");
        } else {
            panic!("Expected Unknown");
        }

        Ok(())
    }

    #[test]
    fn test_parse_initialize() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2025-11-25",
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                },
                "capabilities": {}
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::Initialize(params) = parsed {
            assert_eq!(params.protocol_version, "2025-11-25");
            assert_eq!(params.client_info.name, "test-client");
            assert_eq!(params.client_info.version, "1.0.0");
        } else {
            panic!("Expected Initialize");
        }

        Ok(())
    }

    #[test]
    fn test_parse_initialize_missing_params() {
        let request = make_request("initialize", None);
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    // =========================================================================
    // Resource Methods
    // =========================================================================

    #[test]
    fn test_parse_resources_list() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("resources/list", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::ResourcesList(_)));
        Ok(())
    }

    #[test]
    fn test_parse_resources_list_with_cursor() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "resources/list",
            Some(serde_json::json!({ "cursor": "page2" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ResourcesList(params) = parsed {
            assert_eq!(params.cursor, Some("page2".to_string()));
        } else {
            panic!("Expected ResourcesList");
        }
        Ok(())
    }

    #[test]
    fn test_parse_resources_read() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "resources/read",
            Some(serde_json::json!({ "uri": "file:///etc/hosts" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ResourcesRead(params) = parsed {
            assert_eq!(params.uri, "file:///etc/hosts");
        } else {
            panic!("Expected ResourcesRead");
        }
        Ok(())
    }

    #[test]
    fn test_parse_resources_read_missing_uri() {
        let request = make_request("resources/read", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_resources_templates_list() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("resources/templates/list", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::ResourcesTemplatesList(_)));
        Ok(())
    }

    #[test]
    fn test_parse_resources_subscribe() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "resources/subscribe",
            Some(serde_json::json!({ "uri": "file:///var/log/app.log" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ResourcesSubscribe(params) = parsed {
            assert_eq!(params.uri, "file:///var/log/app.log");
        } else {
            panic!("Expected ResourcesSubscribe");
        }
        Ok(())
    }

    #[test]
    fn test_parse_resources_unsubscribe() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "resources/unsubscribe",
            Some(serde_json::json!({ "uri": "file:///var/log/app.log" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::ResourcesUnsubscribe(params) = parsed {
            assert_eq!(params.uri, "file:///var/log/app.log");
        } else {
            panic!("Expected ResourcesUnsubscribe");
        }
        Ok(())
    }

    // =========================================================================
    // Prompt Methods
    // =========================================================================

    #[test]
    fn test_parse_prompts_list() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("prompts/list", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::PromptsList(_)));
        Ok(())
    }

    #[test]
    fn test_parse_prompts_get() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "prompts/get",
            Some(serde_json::json!({
                "name": "code-review",
                "arguments": { "language": "rust" }
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::PromptsGet(params) = parsed {
            assert_eq!(params.name, "code-review");
            assert!(params.arguments.is_some());
        } else {
            panic!("Expected PromptsGet");
        }
        Ok(())
    }

    #[test]
    fn test_parse_prompts_get_without_arguments() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "prompts/get",
            Some(serde_json::json!({ "name": "simple-prompt" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::PromptsGet(params) = parsed {
            assert_eq!(params.name, "simple-prompt");
            assert!(params.arguments.is_none());
        } else {
            panic!("Expected PromptsGet");
        }
        Ok(())
    }

    #[test]
    fn test_parse_prompts_get_missing_name() {
        let request = make_request("prompts/get", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    // =========================================================================
    // Task Methods
    // =========================================================================

    #[test]
    fn test_parse_tasks_list() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request("tasks/list", None);
        let parsed = parse_request(&request)?;
        assert!(matches!(parsed, ParsedRequest::TasksList(_)));
        Ok(())
    }

    #[test]
    fn test_parse_tasks_get() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "tasks/get",
            Some(serde_json::json!({ "taskId": "task-123" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::TasksGet(params) = parsed {
            assert_eq!(params.task_id, "task-123");
        } else {
            panic!("Expected TasksGet");
        }
        Ok(())
    }

    #[test]
    fn test_parse_tasks_get_missing_id() {
        let request = make_request("tasks/get", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_tasks_cancel() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "tasks/cancel",
            Some(serde_json::json!({ "taskId": "task-456" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::TasksCancel(params) = parsed {
            assert_eq!(params.task_id, "task-456");
        } else {
            panic!("Expected TasksCancel");
        }
        Ok(())
    }

    // =========================================================================
    // Sampling Methods
    // =========================================================================

    #[test]
    fn test_parse_sampling_create_message() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "sampling/createMessage",
            Some(serde_json::json!({
                "messages": [
                    { "role": "user", "content": { "type": "text", "text": "Hello" } }
                ],
                "maxTokens": 100,
                "systemPrompt": "You are a helpful assistant."
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::SamplingCreateMessage(params) = parsed {
            assert_eq!(params.messages.len(), 1);
            assert_eq!(params.max_tokens, Some(100));
            assert_eq!(
                params.system_prompt,
                Some("You are a helpful assistant.".to_string())
            );
        } else {
            panic!("Expected SamplingCreateMessage");
        }
        Ok(())
    }

    #[test]
    fn test_parse_sampling_create_message_missing_messages() {
        let request = make_request("sampling/createMessage", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    // =========================================================================
    // Completion Methods
    // =========================================================================

    #[test]
    fn test_parse_completion_complete() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "completion/complete",
            Some(serde_json::json!({
                "ref": {
                    "type": "ref/resource",
                    "uri": "file:///home"
                },
                "argument": {
                    "name": "path",
                    "value": "/home/user"
                }
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::CompletionComplete(params) = parsed {
            assert_eq!(params.ref_type, "ref/resource");
            assert_eq!(params.ref_value, "file:///home");
            assert!(params.argument.is_some());
            let arg = params.argument.unwrap();
            assert_eq!(arg.name, "path");
            assert_eq!(arg.value, "/home/user");
        } else {
            panic!("Expected CompletionComplete");
        }
        Ok(())
    }

    #[test]
    fn test_parse_completion_complete_prompt_ref() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "completion/complete",
            Some(serde_json::json!({
                "ref": {
                    "type": "ref/prompt",
                    "name": "code-review"
                }
            })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::CompletionComplete(params) = parsed {
            assert_eq!(params.ref_type, "ref/prompt");
            assert_eq!(params.ref_value, "code-review");
            assert!(params.argument.is_none());
        } else {
            panic!("Expected CompletionComplete");
        }
        Ok(())
    }

    #[test]
    fn test_parse_completion_complete_missing_ref() {
        let request = make_request("completion/complete", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    // =========================================================================
    // Logging Methods
    // =========================================================================

    #[test]
    fn test_parse_logging_set_level() -> Result<(), Box<dyn std::error::Error>> {
        let request = make_request(
            "logging/setLevel",
            Some(serde_json::json!({ "level": "debug" })),
        );
        let parsed = parse_request(&request)?;

        if let ParsedRequest::LoggingSetLevel(params) = parsed {
            assert_eq!(params.level, "debug");
        } else {
            panic!("Expected LoggingSetLevel");
        }
        Ok(())
    }

    #[test]
    fn test_parse_logging_set_level_missing_level() {
        let request = make_request("logging/setLevel", Some(serde_json::json!({})));
        let result = parse_request(&request);
        assert!(result.is_err());
    }

    // =========================================================================
    // Method Constants
    // =========================================================================

    #[test]
    fn test_method_constants() {
        // Verify method constants match the strings used in parsing
        assert_eq!(methods::INITIALIZE, "initialize");
        assert_eq!(methods::PING, "ping");
        assert_eq!(methods::TOOLS_LIST, "tools/list");
        assert_eq!(methods::TOOLS_CALL, "tools/call");
        assert_eq!(methods::RESOURCES_LIST, "resources/list");
        assert_eq!(methods::RESOURCES_READ, "resources/read");
        assert_eq!(
            methods::RESOURCES_TEMPLATES_LIST,
            "resources/templates/list"
        );
        assert_eq!(methods::RESOURCES_SUBSCRIBE, "resources/subscribe");
        assert_eq!(methods::RESOURCES_UNSUBSCRIBE, "resources/unsubscribe");
        assert_eq!(methods::PROMPTS_LIST, "prompts/list");
        assert_eq!(methods::PROMPTS_GET, "prompts/get");
        assert_eq!(methods::TASKS_LIST, "tasks/list");
        assert_eq!(methods::TASKS_GET, "tasks/get");
        assert_eq!(methods::TASKS_CANCEL, "tasks/cancel");
        assert_eq!(methods::SAMPLING_CREATE_MESSAGE, "sampling/createMessage");
        assert_eq!(methods::COMPLETION_COMPLETE, "completion/complete");
        assert_eq!(methods::LOGGING_SET_LEVEL, "logging/setLevel");
    }

    // =========================================================================
    // Notification Constants
    // =========================================================================

    #[test]
    fn test_notification_constants() {
        assert_eq!(notifications::INITIALIZED, "notifications/initialized");
        assert_eq!(notifications::CANCELLED, "notifications/cancelled");
        assert_eq!(notifications::PROGRESS, "notifications/progress");
        assert_eq!(notifications::MESSAGE, "notifications/message");
        assert_eq!(
            notifications::RESOURCES_UPDATED,
            "notifications/resources/updated"
        );
        assert_eq!(
            notifications::RESOURCES_LIST_CHANGED,
            "notifications/resources/list_changed"
        );
        assert_eq!(
            notifications::TOOLS_LIST_CHANGED,
            "notifications/tools/list_changed"
        );
        assert_eq!(
            notifications::PROMPTS_LIST_CHANGED,
            "notifications/prompts/list_changed"
        );
    }
}
