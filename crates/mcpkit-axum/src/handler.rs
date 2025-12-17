//! HTTP handlers for MCP requests.

use crate::error::ExtensionError;
use crate::session::{EventStore, StoredEvent};
use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
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
    State(state): State<McpState<H>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse
where
    H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler + Send + Sync + 'static,
{
    // Validate protocol version
    let version = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok());

    if !is_supported_version(version) {
        let provided = version.unwrap_or("none");
        warn!(version = provided, "Unsupported protocol version");
        return ExtensionError::UnsupportedVersion(format!(
            "{} (supported: {})",
            provided,
            SUPPORTED_VERSIONS.join(", ")
        ))
        .into_response();
    }

    // Get or create session
    let session_id = headers
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
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            return ExtensionError::InvalidMessage(e.to_string()).into_response();
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

            // Create a basic response using the handler's capabilities
            // In a full implementation, this would route to the handler's methods
            let response = create_response_for_request(&state, &request).await;

            match serde_json::to_string(&Message::Response(response)) {
                Ok(body) => (
                    StatusCode::OK,
                    [
                        ("content-type", "application/json"),
                        ("mcp-session-id", session_id.as_str()),
                    ],
                    body,
                )
                    .into_response(),
                Err(e) => ExtensionError::Serialization(e).into_response(),
            }
        }
        Message::Notification(notification) => {
            debug!(
                method = %notification.method,
                session_id = %session_id,
                "Received notification"
            );
            (
                StatusCode::ACCEPTED,
                [("mcp-session-id", session_id.as_str())],
            )
                .into_response()
        }
        _ => {
            warn!("Unexpected message type received");
            ExtensionError::InvalidMessage("Expected request or notification".to_string())
                .into_response()
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
/// This handler establishes a Server-Sent Events connection that can be used
/// to push notifications to the client.
///
/// # Headers
///
/// - `mcp-session-id`: Optional. If provided, reconnects to an existing session.
/// - `last-event-id`: Optional. If provided with mcp-session-id, replays missed events.
///
/// # Events
///
/// - `connected`: Sent when the connection is established, includes session ID.
/// - `message`: MCP notification messages.
///
/// # Message Resumability
///
/// Per the MCP Streamable HTTP specification, clients can reconnect with
/// the `Last-Event-ID` header to receive events they may have missed during
/// a connection interruption. The server will replay stored events that
/// occurred after the specified event ID.
pub async fn handle_sse<H>(
    State(state): State<McpState<H>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let (id, rx, replay_events) = if let Some(id) = session_id {
        // Try to reconnect to existing session
        if let Some(rx) = state.sse_sessions.get_receiver(&id) {
            // Check if we need to replay events
            let replay = if let Some(last_id) = &last_event_id {
                info!(session_id = %id, last_event_id = %last_id, "Reconnecting with Last-Event-ID");
                state
                    .sse_sessions
                    .get_events_for_replay(&id, last_id)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            info!(
                session_id = %id,
                replay_count = replay.len(),
                "Reconnected to SSE session"
            );
            (id, rx, replay)
        } else {
            // Session not found, create new
            let (new_id, rx) = state.sse_sessions.create_session();
            info!(session_id = %new_id, "Created new SSE session (requested not found)");
            (new_id, rx, Vec::new())
        }
    } else {
        let (id, rx) = state.sse_sessions.create_session();
        info!(session_id = %id, "Created new SSE session");
        (id, rx, Vec::new())
    };

    let event_store = state.sse_sessions.get_event_store(&id);
    let stream = create_sse_stream_with_replay(id, rx, replay_events, event_store);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Create an SSE stream with support for event replay.
///
/// This function creates an SSE stream that:
/// 1. Sends any replay events (from Last-Event-ID reconnection)
/// 2. Sends the "connected" event with session ID
/// 3. Streams new messages as they arrive
///
/// All events include an `id` field for client-side tracking and reconnection.
fn create_sse_stream_with_replay(
    session_id: String,
    mut rx: tokio::sync::broadcast::Receiver<String>,
    replay_events: Vec<StoredEvent>,
    event_store: Option<Arc<EventStore>>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        // First, replay any missed events
        for stored in replay_events {
            debug!(event_id = %stored.id, "Replaying missed event");
            yield Ok(Event::default()
                .id(&stored.id)
                .event(&stored.event_type)
                .data(&stored.data));
        }

        // Send connected event with session ID and an event ID
        // Per MCP spec: servers MUST immediately send an SSE event with an id
        // to prime the client for reconnection
        let connected_event_id = event_store
            .as_ref()
            .map_or_else(|| "evt-connected".to_string(), |store| store.next_event_id());

        yield Ok(Event::default()
            .id(&connected_event_id)
            .event("connected")
            .data(&session_id));

        // Stream new messages with event IDs
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    // Generate event ID for the new message
                    let event_id = event_store
                        .as_ref()
                        .map_or_else(|| format!("evt-{}", uuid::Uuid::new_v4()), |store| store.next_event_id());

                    yield Ok(Event::default()
                        .id(&event_id)
                        .event("message")
                        .data(msg));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged, skipped messages");
                    // Note: Lagged events may be available in the event store
                    // for replay on reconnection
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("SSE channel closed");
                    break;
                }
            }
        }
    }
}
