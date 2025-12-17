//! HTTP handlers for MCP requests.

use crate::error::ExtensionError;
use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use actix_web::http::header::ContentType;
use actix_web::{HttpRequest, HttpResponse, web};
use futures::stream::{self, StreamExt};
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol::Message;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::{
    PromptHandler, ResourceHandler, ServerHandler, ToolHandler, route_prompts, route_resources,
    route_tools,
};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Handle MCP POST requests.
///
/// This handler processes JSON-RPC messages sent via HTTP POST.
///
/// # Headers
///
/// - `mcp-protocol-version`: Required. Must be a supported protocol version.
/// - `mcp-session-id`: Optional. Used to track sessions.
/// - `Content-Type`: Should be `application/json`.
///
/// # Response
///
/// Returns a JSON-RPC response for request messages, or 202 Accepted for notifications.
pub async fn handle_mcp_post<H>(
    req: HttpRequest,
    state: web::Data<McpState<H>>,
    body: String,
) -> Result<HttpResponse, ExtensionError>
where
    H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler + Send + Sync + 'static,
{
    // Validate protocol version
    let version = req
        .headers()
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok());

    if !is_supported_version(version) {
        let provided = version.unwrap_or("none");
        warn!(version = provided, "Unsupported protocol version");
        return Err(ExtensionError::UnsupportedVersion(format!(
            "{} (supported: {})",
            provided,
            SUPPORTED_VERSIONS.join(", ")
        )));
    }

    // Get or create session
    let session_id = req
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let session_id = match session_id {
        Some(id) => {
            state.sessions.touch(&id);
            id
        }
        None => state.sessions.create(),
    };

    debug!(session_id = %session_id, "Processing MCP request");

    // Parse message
    let msg: Message =
        serde_json::from_str(&body).map_err(|e| ExtensionError::InvalidMessage(e.to_string()))?;

    // Process message
    match msg {
        Message::Request(request) => {
            info!(
                method = %request.method,
                id = ?request.id,
                session_id = %session_id,
                "Handling MCP request"
            );

            // Create a basic response using the handler's capabilities
            let response = create_response_for_request(&state, &request).await;

            let body = serde_json::to_string(&Message::Response(response))
                .map_err(ExtensionError::Serialization)?;

            Ok(HttpResponse::Ok()
                .content_type(ContentType::json())
                .insert_header(("mcp-session-id", session_id))
                .body(body))
        }
        Message::Notification(notification) => {
            debug!(
                method = %notification.method,
                session_id = %session_id,
                "Received notification"
            );
            Ok(HttpResponse::Accepted()
                .insert_header(("mcp-session-id", session_id))
                .finish())
        }
        _ => {
            warn!("Unexpected message type received");
            Err(ExtensionError::InvalidMessage(
                "Expected request or notification".to_string(),
            ))
        }
    }
}

/// Create a response for a request.
///
/// Routes all MCP methods through the appropriate handler traits.
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
            if let Some(result) =
                route_prompts(state.handler.as_ref(), method, params, &ctx).await
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
/// This handler establishes a Server-Sent Events connection that can be used
/// to push notifications to the client.
///
/// # Headers
///
/// - `mcp-session-id`: Optional. If provided, reconnects to an existing session.
///
/// # Events
///
/// - `connected`: Sent when the connection is established, includes session ID.
/// - `message`: MCP notification messages.
pub async fn handle_sse<H>(req: HttpRequest, state: web::Data<McpState<H>>) -> HttpResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let session_id = req
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let (id, rx) = if let Some(id) = session_id {
        // Try to reconnect to existing session
        if let Some(rx) = state.sse_sessions.get_receiver(&id) {
            info!(session_id = %id, "Reconnected to SSE session");
            (id, rx)
        } else {
            // Session not found, create new
            let (new_id, rx) = state.sse_sessions.create_session();
            info!(session_id = %new_id, "Created new SSE session (requested not found)");
            (new_id, rx)
        }
    } else {
        let (id, rx) = state.sse_sessions.create_session();
        info!(session_id = %id, "Created new SSE session");
        (id, rx)
    };

    // Create the SSE stream
    let stream = create_sse_stream(id, rx);

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}

fn create_sse_stream(
    session_id: String,
    rx: tokio::sync::broadcast::Receiver<String>,
) -> impl futures::Stream<Item = Result<web::Bytes, actix_web::error::Error>> {
    // First, send the connected event
    let connected_event = format!("event: connected\ndata: {session_id}\n\n");

    // Create a stream that first yields the connected event, then messages
    let connected = stream::once(async move { Ok(web::Bytes::from(connected_event)) });

    // Create message stream
    let messages = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let event = format!("event: message\ndata: {msg}\n\n");
                    return Some((
                        Ok::<_, actix_web::error::Error>(web::Bytes::from(event)),
                        rx,
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged, skipped messages");
                    // Loop continues naturally
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("SSE channel closed");
                    return None;
                }
            }
        }
    });

    // Add periodic keep-alive comments
    let keepalive = stream::unfold((), |()| async {
        tokio::time::sleep(Duration::from_secs(15)).await;
        Some((
            Ok::<_, actix_web::error::Error>(web::Bytes::from_static(b": keepalive\n\n")),
            (),
        ))
    });

    // Merge the streams (connected first, then interleave messages and keepalive)
    connected.chain(stream::select(messages, keepalive))
}
