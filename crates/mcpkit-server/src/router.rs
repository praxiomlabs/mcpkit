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
use mcpkit_core::types::Object;
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
    /// Sent when a URL-mode elicitation's out-of-band interaction has completed.
    pub const ELICITATION_COMPLETE: &str = "notifications/elicitation/complete";
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
    pub arguments: Object,
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
    pub arguments: Option<Object>,
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

            let arguments = match params.get("arguments") {
                None => Object::new(),
                Some(Value::Object(map)) => map.clone(),
                Some(_) => {
                    return Err(McpError::invalid_params(
                        method,
                        "arguments must be an object",
                    ));
                }
            };

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

            let arguments = match params.get("arguments") {
                None => None,
                Some(Value::Object(map)) => Some(map.clone()),
                Some(_) => {
                    return Err(McpError::invalid_params(
                        method,
                        "arguments must be an object",
                    ));
                }
            };

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
use crate::dispatch::{
    DynCompletionHandler, DynPromptHandler, DynResourceHandler, DynTaskHandler, DynToolHandler,
};
use mcpkit_core::pagination::paginate;
use mcpkit_core::types::{
    CallToolResult, CompleteRequest, CompleteResult, SubscribeRequest, TaskId, UnsubscribeRequest,
};

/// Build a paginated list result: the items under `key` plus an optional
/// `nextCursor`.
fn list_result<T: serde::Serialize>(key: &str, items: Vec<T>, next: Option<String>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        key.to_string(),
        serde_json::to_value(items).unwrap_or_default(),
    );
    if let Some(cursor) = next {
        obj.insert("nextCursor".to_string(), Value::String(cursor));
    }
    Value::Object(obj)
}

/// The `cursor` string from list-request params, if present.
fn list_cursor(params: Option<&Value>) -> Option<&str> {
    params.and_then(|p| p.get("cursor")).and_then(Value::as_str)
}

/// Route tool-related requests to a handler implementing
/// [`ToolHandler`](crate::handler::ToolHandler).
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
pub async fn route_tools(
    handler: &dyn DynToolHandler,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
    page_size: Option<usize>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::TOOLS_LIST => {
            tracing::debug!("Listing available tools");
            let result = async {
                let tools = handler.list_tools(ctx).await?;
                let (page, next) =
                    paginate(tools, list_cursor(params), page_size, methods::TOOLS_LIST)?;
                tracing::debug!(count = page.len(), "Listed tools");
                Ok(list_result("tools", page, next))
            }
            .await;
            Some(result)
        }
        methods::TOOLS_CALL => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::TOOLS_CALL, "missing params")
                })?;
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::invalid_params(methods::TOOLS_CALL, "missing tool name")
                })?;
                let args = match params.get("arguments") {
                    None => Object::new(),
                    Some(Value::Object(map)) => map.clone(),
                    Some(_) => {
                        return Err(McpError::invalid_params(
                            methods::TOOLS_CALL,
                            "arguments must be an object",
                        ));
                    }
                };

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

/// The task-augmentation support a tool declares (`Tool.execution.taskSupport`).
///
/// Defaults to `Forbidden` when the tool is unknown or declares nothing. Used to
/// gate a task-augmented `tools/call` before creating the task. Shared by the
/// stdio runtime and the HTTP adapters.
pub async fn tool_task_support(
    handler: &dyn DynToolHandler,
    name: &str,
    ctx: &Context<'_>,
) -> mcpkit_core::types::TaskSupport {
    use mcpkit_core::types::TaskSupport;
    let tools = handler.list_tools(ctx).await.unwrap_or_default();
    tools
        .iter()
        .find(|t| t.name == name)
        .and_then(|t| t.execution.as_ref())
        .and_then(|e| e.task_support)
        .unwrap_or(TaskSupport::Forbidden)
}

/// Run a tool and return its `CallToolResult` as JSON (the `tasks/result`
/// payload shape). Shared by the stdio runtime and the HTTP adapters.
pub async fn call_tool_json(
    handler: &dyn DynToolHandler,
    name: &str,
    args: Object,
    ctx: &Context<'_>,
) -> Result<serde_json::Value, McpError> {
    let output = handler.call_tool(name, args, ctx).await?;
    let result: CallToolResult = output.into();
    Ok(serde_json::to_value(result).unwrap_or_default())
}

/// Run a task-augmented tool to completion, writing the result (or failure) back
/// through the task-store `handle`.
///
/// Built for HTTP adapters, which spawn this onto their own executor after
/// replying with the initial `CreateTaskResult`. The background context uses a
/// [`NoOpPeer`](crate::context::NoOpPeer): a task-augmented tool on an adapter
/// cannot make server-to-client requests (elicitation/sampling) and its
/// notifications/logging/progress are dropped. Cancellation still works — the
/// handle's cancellation token is wired into the context, so a cooperative tool
/// awaiting `ctx.cancelled()` observes `tasks/cancel`.
pub async fn run_augmented_tool(
    handler: std::sync::Arc<dyn DynToolHandler>,
    handle: crate::capability::tasks::TaskHandle,
    name: String,
    args: Object,
    client_caps: mcpkit_core::capability::ClientCapabilities,
    server_caps: mcpkit_core::capability::ServerCapabilities,
    protocol_version: mcpkit_core::protocol_version::ProtocolVersion,
) {
    use crate::context::{Context, NoOpPeer};
    let peer = NoOpPeer;
    let request_id = mcpkit_core::protocol::RequestId::String(handle.id().as_str().to_string());
    let ctx = match handle.cancel_token() {
        Some(token) => Context::with_cancellation(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
            token,
        ),
        None => Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
        ),
    };
    match call_tool_json(handler.as_ref(), &name, args, &ctx).await {
        // Per spec, a tool result with `isError: true` moves the task to
        // `failed`, while `tasks/result` still returns that result.
        Ok(payload)
            if payload
                .get("isError")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false) =>
        {
            let _ = handle.fail_with_result(payload, Some("tool reported an error".to_string()));
        }
        Ok(payload) => {
            let _ = handle.complete(payload);
        }
        // `tasks/result` must reproduce the JSON-RPC error the request would
        // have returned.
        Err(e) => {
            let _ = handle.fail_with_error(e.into());
        }
    }
}

/// Outcome of [`begin_augmented_task`] — the adapter's decision for a
/// (potentially) task-augmented `tools/call`.
pub enum AugmentedTaskOutcome {
    /// Not a task-augmented call (no non-null `task` field, or malformed) — the
    /// caller should fall through to the normal synchronous `tools/call` path.
    NotApplicable,
    /// Rejected before any task was created (e.g. the tool forbids task
    /// augmentation) — reply with this error.
    Rejected(McpError),
    /// A task was created. Reply immediately with the JSON `CreateTaskResult`
    /// (`.0`), then run the background future (`.1`) on the caller's executor
    /// (e.g. `tokio::spawn`); it writes the result back into the store.
    Started(serde_json::Value, futures::future::BoxFuture<'static, ()>),
}

/// Begin a task-augmented `tools/call` against a per-session task `store`.
///
/// Mirrors the stdio runtime's `try_begin_task`: detect the `task` field, gate on
/// the tool's declared `taskSupport`, create the task, and return the initial
/// `CreateTaskResult` plus a background future the caller spawns. Only call this
/// for the `tools/call` method. See [`run_augmented_tool`] for the background
/// context's limitations (no server-to-client from the tool).
pub async fn begin_augmented_task(
    handler: std::sync::Arc<dyn DynToolHandler>,
    store: &std::sync::Arc<crate::capability::tasks::TaskManager>,
    params: Option<&serde_json::Value>,
    client_caps: mcpkit_core::capability::ClientCapabilities,
    server_caps: mcpkit_core::capability::ServerCapabilities,
    protocol_version: mcpkit_core::protocol_version::ProtocolVersion,
) -> AugmentedTaskOutcome {
    use mcpkit_core::types::TaskSupport;

    let Some(task_meta) = params.and_then(|p| p.get("task")) else {
        return AugmentedTaskOutcome::NotApplicable;
    };
    if task_meta.is_null() {
        return AugmentedTaskOutcome::NotApplicable;
    }
    let Some(name) = params
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
    else {
        // Malformed; let the normal path report it.
        return AugmentedTaskOutcome::NotApplicable;
    };
    let args = match params.and_then(|p| p.get("arguments")) {
        None => Object::new(),
        Some(Value::Object(map)) => map.clone(),
        // Malformed; let the normal path report it.
        Some(_) => return AugmentedTaskOutcome::NotApplicable,
    };
    let ttl = task_meta.get("ttl").and_then(serde_json::Value::as_u64);

    // Gate on the tool's declared task support (a `forbidden` tool must not be
    // task-augmented). The gating context needs no real peer.
    let support = {
        use crate::context::{Context, NoOpPeer};
        let peer = NoOpPeer;
        let gate_id = mcpkit_core::protocol::RequestId::String("tasks/gate".to_string());
        let ctx = Context::new(
            &gate_id,
            None,
            &client_caps,
            &server_caps,
            protocol_version,
            &peer,
        );
        tool_task_support(handler.as_ref(), &name, &ctx).await
    };
    if support == TaskSupport::Forbidden {
        return AugmentedTaskOutcome::Rejected(McpError::invalid_params(
            "tools/call",
            format!("tool '{name}' does not support task-augmented execution"),
        ));
    }

    let handle = store.create(ttl);
    let task = handle
        .task()
        .unwrap_or_else(|| mcpkit_core::types::Task::new(handle.id().clone()));
    let create_result =
        serde_json::to_value(mcpkit_core::types::CreateTaskResult { task, meta: None })
            .unwrap_or_default();
    let fut = run_augmented_tool(
        handler,
        handle,
        name,
        args,
        client_caps,
        server_caps,
        protocol_version,
    );
    AugmentedTaskOutcome::Started(create_result, Box::pin(fut))
}

/// Route resource-related requests to a handler implementing
/// [`ResourceHandler`](crate::handler::ResourceHandler).
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
pub async fn route_resources(
    handler: &dyn DynResourceHandler,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
    page_size: Option<usize>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::RESOURCES_LIST => {
            tracing::debug!("Listing available resources");
            let result = async {
                let resources = handler.list_resources(ctx).await?;
                let (page, next) = paginate(
                    resources,
                    list_cursor(params),
                    page_size,
                    methods::RESOURCES_LIST,
                )?;
                tracing::debug!(count = page.len(), "Listed resources");
                Ok(list_result("resources", page, next))
            }
            .await;
            Some(result)
        }
        methods::RESOURCES_TEMPLATES_LIST => {
            tracing::debug!("Listing available resource templates");
            let result = async {
                let templates = handler.list_resource_templates(ctx).await?;
                let (page, next) = paginate(
                    templates,
                    list_cursor(params),
                    page_size,
                    methods::RESOURCES_TEMPLATES_LIST,
                )?;
                tracing::debug!(count = page.len(), "Listed resource templates");
                Ok(list_result("resourceTemplates", page, next))
            }
            .await;
            Some(result)
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
        methods::RESOURCES_SUBSCRIBE => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::RESOURCES_SUBSCRIBE, "missing params")
                })?;
                let req: SubscribeRequest =
                    serde_json::from_value(params.clone()).map_err(|_| {
                        McpError::invalid_params(methods::RESOURCES_SUBSCRIBE, "missing uri")
                    })?;
                tracing::info!(uri = %req.uri, "Subscribing to resource");
                if handler.subscribe(&req.uri, ctx).await? {
                    Ok(serde_json::json!({}))
                } else {
                    Err(McpError::internal(format!(
                        "subscription not established for {}",
                        req.uri
                    )))
                }
            }
            .await;
            Some(result)
        }
        methods::RESOURCES_UNSUBSCRIBE => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::RESOURCES_UNSUBSCRIBE, "missing params")
                })?;
                let req: UnsubscribeRequest =
                    serde_json::from_value(params.clone()).map_err(|_| {
                        McpError::invalid_params(methods::RESOURCES_UNSUBSCRIBE, "missing uri")
                    })?;
                tracing::info!(uri = %req.uri, "Unsubscribing from resource");
                if handler.unsubscribe(&req.uri, ctx).await? {
                    Ok(serde_json::json!({}))
                } else {
                    Err(McpError::internal(format!(
                        "unsubscribe not honored for {}",
                        req.uri
                    )))
                }
            }
            .await;
            Some(result)
        }
        _ => None,
    }
}

/// Route prompt-related requests to a handler implementing
/// [`PromptHandler`](crate::handler::PromptHandler).
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
pub async fn route_prompts(
    handler: &dyn DynPromptHandler,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
    page_size: Option<usize>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::PROMPTS_LIST => {
            tracing::debug!("Listing available prompts");
            let result = async {
                let prompts = handler.list_prompts(ctx).await?;
                let (page, next) = paginate(
                    prompts,
                    list_cursor(params),
                    page_size,
                    methods::PROMPTS_LIST,
                )?;
                tracing::debug!(count = page.len(), "Listed prompts");
                Ok(list_result("prompts", page, next))
            }
            .await;
            Some(result)
        }
        methods::PROMPTS_GET => {
            let result = async {
                let params = params.ok_or_else(|| {
                    McpError::invalid_params(methods::PROMPTS_GET, "missing params")
                })?;
                let name = params.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    McpError::invalid_params(methods::PROMPTS_GET, "missing prompt name")
                })?;
                let args = match params.get("arguments") {
                    None => None,
                    Some(Value::Object(map)) => Some(map.clone()),
                    Some(_) => {
                        return Err(McpError::invalid_params(
                            methods::PROMPTS_GET,
                            "arguments must be an object",
                        ));
                    }
                };

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

/// Route task-related requests to a handler implementing
/// [`TaskHandler`](crate::handler::TaskHandler).
///
/// Handles `tasks/list`, `tasks/get`, and `tasks/cancel`. Returns `None` if the
/// method is not task-related. (`tasks/result` is handled by the task-augmented
/// call flow, not here.)
pub async fn route_tasks(
    handler: &dyn DynTaskHandler,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    match method {
        methods::TASKS_LIST => {
            let result = handler.list_tasks(ctx).await;
            Some(result.map(|r| serde_json::to_value(r).unwrap_or_default()))
        }
        methods::TASKS_GET => {
            let result = async {
                let id = parse_task_id(params, methods::TASKS_GET)?;
                match handler.get_task(&id, ctx).await? {
                    Some(result) => Ok(serde_json::to_value(result).unwrap_or_default()),
                    None => Err(McpError::invalid_params(
                        methods::TASKS_GET,
                        format!("unknown task: {id}"),
                    )),
                }
            }
            .await;
            Some(result)
        }
        methods::TASKS_CANCEL => {
            let result = async {
                let id = parse_task_id(params, methods::TASKS_CANCEL)?;
                // `Ok(None)` is an unknown task; a real cancellation failure
                // propagates as `Err` from the handler.
                match handler.cancel_task(&id, ctx).await? {
                    Some(result) => Ok(serde_json::to_value(result).unwrap_or_default()),
                    None => Err(McpError::invalid_params(
                        methods::TASKS_CANCEL,
                        format!("unknown task: {id}"),
                    )),
                }
            }
            .await;
            Some(result)
        }
        _ => None,
    }
}

/// Route `logging/setLevel` to the base handler's
/// [`set_log_level`](crate::handler::ServerHandler::set_log_level), but only when
/// the server advertises the `logging` capability.
///
/// Returns `None` for any other method (or when logging is not advertised) so the
/// caller falls through to its normal not-found handling. Shared by the runtime
/// router and the HTTP adapters so `logging/setLevel` behaves the same on every
/// surface.
pub async fn route_logging<H: crate::handler::ServerHandler>(
    handler: &H,
    server_caps: &mcpkit_core::capability::ServerCapabilities,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    if method != methods::LOGGING_SET_LEVEL || !server_caps.has_logging() {
        return None;
    }
    let result = async {
        let params = params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;
        let req: mcpkit_core::types::SetLevelRequest = serde_json::from_value(params.clone())
            .map_err(|_| McpError::invalid_params(method, "invalid or missing level"))?;
        handler.set_log_level(req.level, ctx).await?;
        Ok(serde_json::json!({}))
    }
    .await;
    Some(result)
}

/// Route `completion/complete` to a registered completion handler.
///
/// Returns `None` when the method is not `completion/complete` or no completion
/// handler is registered (so the caller falls through to its normal not-found
/// handling). Shared by the runtime and the framework adapters so completion is
/// dispatched consistently wherever the server routes requests. The response's
/// `values` are capped at [`MAX_COMPLETION_VALUES`](mcpkit_core::types::MAX_COMPLETION_VALUES).
pub async fn route_completion(
    handler: Option<&dyn DynCompletionHandler>,
    method: &str,
    params: Option<&serde_json::Value>,
    ctx: &Context<'_>,
) -> Option<Result<serde_json::Value, McpError>> {
    if method != methods::COMPLETION_COMPLETE {
        return None;
    }
    let handler = handler?;
    let result = async {
        let params = params.ok_or_else(|| McpError::invalid_params(method, "missing params"))?;
        let req: CompleteRequest = serde_json::from_value(params.clone()).map_err(|e| {
            McpError::invalid_params(method, format!("invalid completion request: {e}"))
        })?;
        // Cap the values to the spec limit while preserving any result-level
        // `_meta` the handler attached.
        let CompleteResult { completion, meta } = handler.complete(&req, ctx).await?;
        let result = CompleteResult {
            completion: completion.capped(),
            meta,
        };
        Ok(serde_json::to_value(result).unwrap_or_default())
    }
    .await;
    Some(result)
}

/// Extract a required `taskId` parameter.
fn parse_task_id(
    params: Option<&serde_json::Value>,
    method: &'static str,
) -> Result<TaskId, McpError> {
    let task_id = params
        .and_then(|p| p.get("taskId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params(method, "missing taskId"))?;
    Ok(TaskId::new(task_id))
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
    fn test_parse_tools_call_rejects_non_object_arguments() {
        let request = make_request(
            "tools/call",
            Some(serde_json::json!({ "name": "search", "arguments": 5 })),
        );
        let err = parse_request(&request).expect_err("non-object arguments must be rejected");
        assert!(
            err.to_string().contains("arguments must be an object"),
            "expected invalid-params on arguments, got: {err}"
        );
    }

    #[test]
    fn test_parse_prompts_get_rejects_non_object_arguments() {
        let request = make_request(
            "prompts/get",
            Some(serde_json::json!({ "name": "code-review", "arguments": ["rust"] })),
        );
        let err = parse_request(&request).expect_err("non-object arguments must be rejected");
        assert!(
            err.to_string().contains("arguments must be an object"),
            "expected invalid-params on arguments, got: {err}"
        );
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

    #[tokio::test]
    async fn route_tasks_dispatches_list_get_cancel() {
        use crate::capability::tasks::TaskService;
        use crate::context::NoOpPeer;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;

        let service = TaskService::new();
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );

        // tasks/list -> { "tasks": [] } (no tasks registered).
        let listed = route_tasks(&service, methods::TASKS_LIST, None, &ctx)
            .await
            .expect("tasks/list is routed")
            .expect("ok");
        assert_eq!(listed, serde_json::json!({ "tasks": [] }));

        // tasks/get with a missing taskId is a routed error.
        let got = route_tasks(&service, methods::TASKS_GET, None, &ctx)
            .await
            .expect("tasks/get is routed");
        assert!(got.is_err());

        // An unknown task id is a routed error, not a panic.
        let cancel = route_tasks(
            &service,
            methods::TASKS_CANCEL,
            Some(&serde_json::json!({ "taskId": "nope" })),
            &ctx,
        )
        .await
        .expect("tasks/cancel is routed");
        assert!(cancel.is_err());

        // A non-task method is not handled here.
        assert!(
            route_tasks(&service, methods::TOOLS_LIST, None, &ctx)
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn route_tasks_list_get_and_cancel_emit_result_meta() {
        use crate::context::NoOpPeer;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;
        use mcpkit_core::types::{
            CancelTaskResult, GetTaskResult, ListTasksResult, Meta, Task, TaskId,
        };

        // A handler that attaches result-level `_meta` on list/get/cancel and a
        // `nextCursor` on list.
        struct MetaTaskHandler;
        impl crate::handler::TaskHandler for MetaTaskHandler {
            async fn list_tasks(&self, _ctx: &Context<'_>) -> Result<ListTasksResult, McpError> {
                Ok(ListTasksResult {
                    tasks: vec![],
                    next_cursor: Some("page-2".to_string()),
                    meta: Some(Meta::new().with("origin", serde_json::json!("test"))),
                })
            }
            async fn get_task(
                &self,
                id: &TaskId,
                _ctx: &Context<'_>,
            ) -> Result<Option<GetTaskResult>, McpError> {
                let meta = Meta::new().with("origin", serde_json::json!("test"));
                Ok(Some(
                    GetTaskResult::from(Task::new(id.clone())).with_meta(meta),
                ))
            }
            async fn cancel_task(
                &self,
                id: &TaskId,
                _ctx: &Context<'_>,
            ) -> Result<Option<CancelTaskResult>, McpError> {
                let meta = Meta::new().with("origin", serde_json::json!("test"));
                Ok(Some(
                    CancelTaskResult::from(Task::new(id.clone())).with_meta(meta),
                ))
            }
        }

        let handler = MetaTaskHandler;
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );
        let params = serde_json::json!({ "taskId": "t-1" });

        for method in [methods::TASKS_GET, methods::TASKS_CANCEL] {
            let resp = route_tasks(&handler, method, Some(&params), &ctx)
                .await
                .expect("routed")
                .expect("ok");
            // Task fields flattened at the top level, plus result-level `_meta`.
            assert_eq!(resp["taskId"], "t-1", "{method}");
            assert_eq!(resp["_meta"]["origin"], "test", "{method}");
        }

        // tasks/list now carries `nextCursor` + result-level `_meta` through the
        // `ListTasksResult` wrapper (previously hand-built as `{ "tasks": .. }`).
        let listed = route_tasks(&handler, methods::TASKS_LIST, None, &ctx)
            .await
            .expect("routed")
            .expect("ok");
        assert_eq!(listed["nextCursor"], "page-2");
        assert_eq!(listed["_meta"]["origin"], "test");
    }

    #[tokio::test]
    async fn route_completion_dispatches_with_context_and_caps_values() {
        use crate::context::NoOpPeer;
        use crate::dispatch::DynCompletionHandler;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;
        use mcpkit_core::types::{CompleteRequest, CompleteResult, Completion, Meta};

        // Reads `context.arguments.owner`, returns more than the 100-value cap,
        // and attaches result-level `_meta`.
        struct CtxCompletion;
        impl crate::handler::CompletionHandler for CtxCompletion {
            async fn complete(
                &self,
                request: &CompleteRequest,
                _ctx: &Context<'_>,
            ) -> Result<CompleteResult, McpError> {
                let owner = request
                    .context
                    .as_ref()
                    .and_then(|c| c.arguments.as_ref())
                    .and_then(|a| a.get("owner").cloned())
                    .unwrap_or_default();
                let completion = Completion {
                    values: (0..150).map(|i| format!("{owner}-{i}")).collect(),
                    total: Some(150),
                    has_more: Some(false),
                };
                Ok(CompleteResult::from(completion)
                    .with_meta(Meta::new().with("origin", serde_json::json!("test"))))
            }
        }

        let handler = CtxCompletion;
        let dyn_handler: &dyn DynCompletionHandler = &handler;
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );
        let params = serde_json::json!({
            "ref": {"type": "ref/prompt", "name": "p"},
            "argument": {"name": "a", "value": "x"},
            "context": {"arguments": {"owner": "acme"}}
        });

        // No handler registered -> not dispatched (caller yields method-not-found).
        assert!(
            route_completion(None, methods::COMPLETION_COMPLETE, Some(&params), &ctx)
                .await
                .is_none()
        );
        // A non-completion method is not handled here.
        assert!(
            route_completion(Some(dyn_handler), methods::TOOLS_LIST, None, &ctx)
                .await
                .is_none()
        );

        // Dispatched: context propagates and the 100-value cap is enforced.
        let resp = route_completion(
            Some(dyn_handler),
            methods::COMPLETION_COMPLETE,
            Some(&params),
            &ctx,
        )
        .await
        .expect("routed")
        .expect("ok");
        let values = resp["completion"]["values"].as_array().expect("values");
        assert_eq!(values.len(), 100); // capped from 150
        assert_eq!(values[0], "acme-0"); // context.arguments propagated
        assert_eq!(resp["completion"]["hasMore"], true); // forced true by the cap
        assert_eq!(resp["completion"]["total"], 150); // handler total preserved
        assert_eq!(resp["_meta"]["origin"], "test"); // handler result-level _meta
    }

    #[tokio::test]
    async fn route_tools_paginates_tools_list() {
        use crate::context::NoOpPeer;
        use crate::handler::ToolHandler;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;
        use mcpkit_core::types::{Tool, ToolOutput};

        struct Tools;
        impl ToolHandler for Tools {
            async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
                Ok(vec![Tool::new("a"), Tool::new("b"), Tool::new("c")])
            }
            async fn call_tool(
                &self,
                _name: &str,
                _args: serde_json::Map<String, Value>,
                _ctx: &Context<'_>,
            ) -> Result<ToolOutput, McpError> {
                Ok(ToolOutput::text("x"))
            }
        }

        let handler = Tools;
        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );

        // Page 1 of size 2 -> two tools plus a nextCursor.
        let page1 = route_tools(&handler, methods::TOOLS_LIST, None, &ctx, Some(2))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(page1["tools"].as_array().unwrap().len(), 2);
        let cursor = page1["nextCursor"]
            .as_str()
            .expect("nextCursor")
            .to_string();

        // Page 2 via the cursor -> the last tool, no further cursor.
        let params = serde_json::json!({ "cursor": cursor });
        let page2 = route_tools(&handler, methods::TOOLS_LIST, Some(&params), &ctx, Some(2))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(page2["tools"].as_array().unwrap().len(), 1);
        assert!(page2.get("nextCursor").is_none());

        // Pagination disabled (None) -> all three, no cursor.
        let all = route_tools(&handler, methods::TOOLS_LIST, None, &ctx, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(all["tools"].as_array().unwrap().len(), 3);
        assert!(all.get("nextCursor").is_none());

        // An invalid cursor is a routed error.
        let bad = serde_json::json!({ "cursor": "not-a-cursor" });
        let err = route_tools(&handler, methods::TOOLS_LIST, Some(&bad), &ctx, Some(2))
            .await
            .unwrap();
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn route_resources_dispatches_subscribe_and_unsubscribe() {
        use crate::context::NoOpPeer;
        use crate::handler::ResourceHandler;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;
        use mcpkit_core::types::{Resource, ResourceContents};

        struct Res {
            ok: bool,
        }
        impl ResourceHandler for Res {
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
            async fn subscribe(&self, _uri: &str, _ctx: &Context<'_>) -> Result<bool, McpError> {
                Ok(self.ok)
            }
            async fn unsubscribe(&self, _uri: &str, _ctx: &Context<'_>) -> Result<bool, McpError> {
                Ok(self.ok)
            }
        }

        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;
        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );
        let params = serde_json::json!({ "uri": "file:///x" });

        // subscribe -> Ok(true) yields an empty result (no longer method-not-found).
        let ok = route_resources(
            &Res { ok: true },
            methods::RESOURCES_SUBSCRIBE,
            Some(&params),
            &ctx,
            None,
        )
        .await
        .expect("subscribe is routed")
        .expect("ok");
        assert_eq!(ok, serde_json::json!({}));

        // subscribe -> Ok(false) is an error, not a fake success.
        assert!(
            route_resources(
                &Res { ok: false },
                methods::RESOURCES_SUBSCRIBE,
                Some(&params),
                &ctx,
                None,
            )
            .await
            .expect("routed")
            .is_err()
        );

        // Missing uri -> invalid params.
        assert!(
            route_resources(
                &Res { ok: true },
                methods::RESOURCES_SUBSCRIBE,
                Some(&serde_json::json!({})),
                &ctx,
                None,
            )
            .await
            .expect("routed")
            .is_err()
        );

        // unsubscribe -> Ok(true) yields an empty result.
        let ok = route_resources(
            &Res { ok: true },
            methods::RESOURCES_UNSUBSCRIBE,
            Some(&params),
            &ctx,
            None,
        )
        .await
        .expect("unsubscribe is routed")
        .expect("ok");
        assert_eq!(ok, serde_json::json!({}));
    }
}
