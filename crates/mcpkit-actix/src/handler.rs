//! HTTP handlers for MCP requests.

use crate::error::ExtensionError;
use crate::state::{HasServerInfo, McpConfig};
use crate::{is_supported_version, SUPPORTED_VERSIONS};
use actix_web::http::header::ContentType;
use actix_web::{web, HttpRequest, HttpResponse};
use futures::stream::{self, StreamExt};
use mcpkit_core::protocol::Message;
use mcpkit_server::ServerHandler;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Handle MCP POST requests.
///
/// This handler processes JSON-RPC messages sent via HTTP POST.
///
/// # Headers
///
/// - `MCP-Protocol-Version`: Required. Must be a supported protocol version.
/// - `Mcp-Session-Id`: Optional. Used to track sessions.
/// - `Content-Type`: Should be `application/json`.
///
/// # Response
///
/// Returns a JSON-RPC response for request messages, or 202 Accepted for notifications.
pub async fn handle_mcp_post<H>(
    req: HttpRequest,
    config: web::Data<McpConfig<H>>,
    body: String,
) -> Result<HttpResponse, ExtensionError>
where
    H: ServerHandler + Send + Sync + 'static,
{
    // Validate protocol version
    let version = req
        .headers()
        .get("MCP-Protocol-Version")
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
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let session_id = match session_id {
        Some(id) => {
            config.sessions.touch(&id);
            id
        }
        None => config.sessions.create(),
    };

    debug!(session_id = %session_id, "Processing MCP request");

    // Parse message
    let msg: Message = serde_json::from_str(&body)
        .map_err(|e| ExtensionError::InvalidMessage(e.to_string()))?;

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
            let response = create_response_for_request(&config, &request).await;

            let body = serde_json::to_string(&Message::Response(response))
                .map_err(ExtensionError::Serialization)?;

            Ok(HttpResponse::Ok()
                .content_type(ContentType::json())
                .insert_header(("Mcp-Session-Id", session_id))
                .body(body))
        }
        Message::Notification(notification) => {
            debug!(
                method = %notification.method,
                session_id = %session_id,
                "Received notification"
            );
            Ok(HttpResponse::Accepted()
                .insert_header(("Mcp-Session-Id", session_id))
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
/// This is a simplified implementation - a full implementation would
/// route through the `ServerHandler` properly.
async fn create_response_for_request<H>(
    config: &McpConfig<H>,
    request: &mcpkit_core::protocol::Request,
) -> mcpkit_core::protocol::Response
where
    H: ServerHandler + Send + Sync + 'static,
{
    use mcpkit_core::error::JsonRpcError;
    use mcpkit_core::protocol::Response;

    let method = request.method.as_ref();
    match method {
        "ping" => Response::success(request.id.clone(), serde_json::json!({})),
        "initialize" => {
            let init_result = serde_json::json!({
                "protocolVersion": "2025-06-18",
                "serverInfo": config.server_info,
                "capabilities": config.handler.capabilities(),
            });
            Response::success(request.id.clone(), init_result)
        }
        _ => {
            // For other methods, return method not found
            Response::error(
                request.id.clone(),
                JsonRpcError::method_not_found(format!("Method '{}' not found", method)),
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
/// - `Mcp-Session-Id`: Optional. If provided, reconnects to an existing session.
///
/// # Events
///
/// - `connected`: Sent when the connection is established, includes session ID.
/// - `message`: MCP notification messages.
pub async fn handle_sse<H>(
    req: HttpRequest,
    config: web::Data<McpConfig<H>>,
) -> HttpResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let session_id = req
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let (id, rx) = match session_id {
        Some(id) => {
            // Try to reconnect to existing session
            if let Some(rx) = config.sse_sessions.get_receiver(&id) {
                info!(session_id = %id, "Reconnected to SSE session");
                (id, rx)
            } else {
                // Session not found, create new
                let (new_id, rx) = config.sse_sessions.create_session();
                info!(session_id = %new_id, "Created new SSE session (requested not found)");
                (new_id, rx)
            }
        }
        None => {
            let (id, rx) = config.sse_sessions.create_session();
            info!(session_id = %id, "Created new SSE session");
            (id, rx)
        }
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
    let connected_event = format!("event: connected\ndata: {}\n\n", session_id);

    // Create a stream that first yields the connected event, then messages
    let connected = stream::once(async move { Ok(web::Bytes::from(connected_event)) });

    // Create message stream
    let messages = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let event = format!("event: message\ndata: {}\n\n", msg);
                    return Some((
                        Ok::<_, actix_web::error::Error>(web::Bytes::from(event)),
                        rx,
                    ));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged, skipped messages");
                    continue;
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
