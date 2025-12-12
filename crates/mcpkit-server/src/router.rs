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
    fn test_parse_ping() {
        let request = make_request("ping", None);
        let parsed = parse_request(&request).unwrap();
        assert!(matches!(parsed, ParsedRequest::Ping));
    }

    #[test]
    fn test_parse_tools_list() {
        let request = make_request("tools/list", None);
        let parsed = parse_request(&request).unwrap();
        assert!(matches!(parsed, ParsedRequest::ToolsList(_)));
    }

    #[test]
    fn test_parse_tools_call() {
        let request = make_request(
            "tools/call",
            Some(serde_json::json!({
                "name": "search",
                "arguments": {"query": "test"}
            })),
        );
        let parsed = parse_request(&request).unwrap();

        if let ParsedRequest::ToolsCall(params) = parsed {
            assert_eq!(params.name, "search");
        } else {
            panic!("Expected ToolsCall");
        }
    }

    #[test]
    fn test_parse_unknown_method() {
        let request = make_request("unknown/method", None);
        let parsed = parse_request(&request).unwrap();

        if let ParsedRequest::Unknown(method) = parsed {
            assert_eq!(method, "unknown/method");
        } else {
            panic!("Expected Unknown");
        }
    }

    #[test]
    fn test_parse_initialize() {
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
        let parsed = parse_request(&request).unwrap();

        if let ParsedRequest::Initialize(params) = parsed {
            assert_eq!(params.protocol_version, "2025-11-25");
            assert_eq!(params.client_info.name, "test-client");
        } else {
            panic!("Expected Initialize");
        }
    }
}
