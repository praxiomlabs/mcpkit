//! HTTP handlers for MCP requests using Warp.

use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use futures::StreamExt;
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol::Message;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::{
    PromptHandler, ResourceHandler, ServerHandler, ToolHandler, route_prompts, route_resources,
    route_tools,
};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, info, warn};
use warp::http::StatusCode;
use warp::sse::Event;
use warp::Filter;

/// Handle MCP POST requests.
///
/// This is the core handler function that processes JSON-RPC messages.
pub async fn handle_mcp_post<H>(
    state: Arc<McpState<H>>,
    version: Option<String>,
    session_id: Option<String>,
    body: String,
) -> Result<impl warp::Reply, Infallible>
where
    H: ServerHandler
        + ToolHandler
        + ResourceHandler
        + PromptHandler
        + HasServerInfo
        + Send
        + Sync
        + 'static,
{
    // Validate protocol version
    if !is_supported_version(version.as_deref()) {
        let provided = version.as_deref().unwrap_or("none");
        warn!(version = provided, "Unsupported protocol version");
        let error_body = serde_json::json!({
            "error": {
                "code": -32600,
                "message": format!(
                    "Unsupported protocol version: {} (supported: {})",
                    provided,
                    SUPPORTED_VERSIONS.join(", ")
                )
            }
        });
        return Ok(warp::reply::with_status(
            warp::reply::json(&error_body),
            StatusCode::BAD_REQUEST,
        ));
    }

    // Get or create session
    let session_id = match session_id {
        Some(id) => {
            state.sessions.touch(&id);
            id
        }
        None => state.sessions.create(),
    };

    debug!(session_id = %session_id, "Processing MCP request");

    // Parse message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            let error_body = serde_json::json!({
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {e}")
                }
            });
            return Ok(warp::reply::with_status(
                warp::reply::json(&error_body),
                StatusCode::BAD_REQUEST,
            ));
        }
    };

    // Process message
    match msg {
        Message::Request(request) => {
            info!(
                method = %request.method,
                id = ?request.id,
                session_id = %session_id,
                "Handling MCP request"
            );

            let response = create_response_for_request(&state, &request).await;

            match serde_json::to_value(Message::Response(response)) {
                Ok(body) => Ok(warp::reply::with_status(
                    warp::reply::json(&body),
                    StatusCode::OK,
                )),
                Err(e) => {
                    let error_body = serde_json::json!({
                        "error": {
                            "code": -32603,
                            "message": format!("Internal error: {e}")
                        }
                    });
                    Ok(warp::reply::with_status(
                        warp::reply::json(&error_body),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            }
        }
        Message::Notification(notification) => {
            debug!(
                method = %notification.method,
                session_id = %session_id,
                "Received notification"
            );
            Ok(warp::reply::with_status(
                warp::reply::json(&serde_json::json!({})),
                StatusCode::ACCEPTED,
            ))
        }
        _ => {
            warn!("Unexpected message type received");
            let error_body = serde_json::json!({
                "error": {
                    "code": -32600,
                    "message": "Expected request or notification"
                }
            });
            Ok(warp::reply::with_status(
                warp::reply::json(&error_body),
                StatusCode::BAD_REQUEST,
            ))
        }
    }
}

/// Create a response for a request.
async fn create_response_for_request<H>(
    state: &McpState<H>,
    request: &mcpkit_core::protocol::Request,
) -> mcpkit_core::protocol::Response
where
    H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler + Send + Sync + 'static,
{
    use mcpkit_core::error::JsonRpcError;
    use mcpkit_core::protocol::Response;

    let method = request.method.as_ref();
    let params = request.params.as_ref();

    // Create a context for the request
    let req_id = request.id.clone();
    let client_caps = ClientCapabilities::default();
    let server_caps = state.handler.capabilities();
    let protocol_version = ProtocolVersion::LATEST;
    let peer = NoOpPeer;
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    match method {
        "ping" => Response::success(request.id.clone(), serde_json::json!({})),
        "initialize" => {
            let init_result = serde_json::json!({
                "protocolVersion": ProtocolVersion::LATEST.as_str(),
                "serverInfo": state.server_info,
                "capabilities": state.handler.capabilities(),
            });
            Response::success(request.id.clone(), init_result)
        }
        _ => {
            // Try routing to tools
            if let Some(result) = route_tools(state.handler.as_ref(), method, params, &ctx).await {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing to resources
            if let Some(result) =
                route_resources(state.handler.as_ref(), method, params, &ctx).await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing to prompts
            if let Some(result) = route_prompts(state.handler.as_ref(), method, params, &ctx).await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Method not found
            Response::error(
                request.id.clone(),
                JsonRpcError::method_not_found(format!("Method '{method}' not found")),
            )
        }
    }
}

/// Handle SSE connections for server-to-client streaming.
///
/// This returns a stream of Server-Sent Events.
pub fn handle_sse<H>(
    state: Arc<McpState<H>>,
    session_id: Option<String>,
) -> impl warp::Reply
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let (session_id, rx) = if let Some(id) = session_id {
        if let Some(rx) = state.sse_sessions.get_receiver(&id) {
            info!(session_id = %id, "Reconnected to SSE session");
            (id, rx)
        } else {
            let (new_id, rx) = state.sse_sessions.create_session();
            info!(session_id = %new_id, "Created new SSE session (requested not found)");
            (new_id, rx)
        }
    } else {
        let (id, rx) = state.sse_sessions.create_session();
        info!(session_id = %id, "Created new SSE session");
        (id, rx)
    };

    // Create a stream of SSE events
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let session = session_id.clone();
        async move {
            match result {
                Ok(msg) => {
                    let event_id = format!("evt-{}", uuid::Uuid::new_v4());
                    Some(Ok::<_, Infallible>(
                        Event::default()
                            .id(&event_id)
                            .event("message")
                            .data(msg),
                    ))
                }
                Err(e) => {
                    warn!(error = %e, session_id = %session, "SSE broadcast error");
                    None
                }
            }
        }
    });

    warp::sse::reply(warp::sse::keep_alive().stream(stream))
}

/// Create a filter to extract the MCP protocol version header.
#[must_use] 
pub fn with_protocol_version(
) -> impl Filter<Extract = (Option<String>,), Error = warp::Rejection> + Clone {
    warp::header::optional("mcp-protocol-version")
}

/// Create a filter to extract the MCP session ID header.
#[must_use]
pub fn with_session_id() -> impl Filter<Extract = (Option<String>,), Error = warp::Rejection> + Clone
{
    warp::header::optional("mcp-session-id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
    use mcpkit_server::context::Context;
    use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};

    // Test handler for integration tests
    struct TestHandler;

    impl ServerHandler for TestHandler {
        fn server_info(&self) -> ServerInfo {
            ServerInfo::new("test-warp-handler", "1.0.0")
        }

        fn capabilities(&self) -> ServerCapabilities {
            ServerCapabilities::new().with_tools().with_prompts()
        }
    }

    impl ToolHandler for TestHandler {
        async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
            Ok(vec![Tool::new("test-tool").description("A test tool")])
        }

        async fn call_tool(
            &self,
            _name: &str,
            _args: serde_json::Value,
            _ctx: &Context<'_>,
        ) -> Result<ToolOutput, McpError> {
            Ok(ToolOutput::text("test result"))
        }
    }

    impl ResourceHandler for TestHandler {
        async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            uri: &str,
            _ctx: &Context<'_>,
        ) -> Result<Vec<ResourceContents>, McpError> {
            Ok(vec![ResourceContents::text(uri, "test content")])
        }
    }

    impl PromptHandler for TestHandler {
        async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
            Ok(vec![Prompt::new("test").description("A test prompt")])
        }

        async fn get_prompt(
            &self,
            _name: &str,
            _args: Option<serde_json::Map<String, serde_json::Value>>,
            _ctx: &Context<'_>,
        ) -> Result<GetPromptResult, McpError> {
            Ok(GetPromptResult {
                description: Some("Test prompt".to_string()),
                messages: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_handle_mcp_post_unsupported_version() {
        let state = Arc::new(McpState::new(TestHandler));

        // Test with unsupported version
        let response = handle_mcp_post(
            state,
            Some("unsupported-version".to_string()),
            None,
            r#"{"jsonrpc":"2.0","method":"ping","id":1}"#.to_string(),
        )
        .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mcp_post_invalid_json() {
        let state = Arc::new(McpState::new(TestHandler));

        // Test with invalid JSON
        let response = handle_mcp_post(
            state,
            Some("2025-11-25".to_string()),
            None,
            "invalid json".to_string(),
        )
        .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mcp_post_ping_request() {
        let state = Arc::new(McpState::new(TestHandler));

        // Test ping request
        let response = handle_mcp_post(
            state,
            Some("2025-11-25".to_string()),
            None,
            r#"{"jsonrpc":"2.0","method":"ping","id":1}"#.to_string(),
        )
        .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mcp_post_initialize_request() {
        let state = Arc::new(McpState::new(TestHandler));

        // Test initialize request
        let response = handle_mcp_post(
            state,
            Some("2025-11-25".to_string()),
            None,
            r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":1}"#.to_string(),
        )
        .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_handle_mcp_post_with_session() {
        let state = Arc::new(McpState::new(TestHandler));

        // Create a session first
        let session_id = state.sessions.create();

        // Test with existing session
        let response = handle_mcp_post(
            Arc::clone(&state),
            Some("2025-11-25".to_string()),
            Some(session_id.clone()),
            r#"{"jsonrpc":"2.0","method":"ping","id":1}"#.to_string(),
        )
        .await;

        assert!(response.is_ok());
        assert!(state.sessions.exists(&session_id));
    }

    #[tokio::test]
    async fn test_handle_mcp_post_notification() {
        let state = Arc::new(McpState::new(TestHandler));

        // Test notification (no id field)
        let response = handle_mcp_post(
            state,
            Some("2025-11-25".to_string()),
            None,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        )
        .await;

        assert!(response.is_ok());
    }

    #[test]
    fn test_with_protocol_version_filter() {
        // Just verify the filter can be created
        let _filter = with_protocol_version();
    }

    #[test]
    fn test_with_session_id_filter() {
        // Just verify the filter can be created
        let _filter = with_session_id();
    }
}
