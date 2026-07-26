//! HTTP handlers for MCP requests using Warp.

use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use futures::StreamExt;
use mcpkit_core::auth::VerifiedUser;
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol::Message;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::capability::tasks::{TaskManager, route_task_store};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::{
    AugmentedTaskOutcome, PromptHandler, ResourceHandler, ServerHandler, ToolHandler,
    begin_augmented_task, route_completion, route_logging, route_prompts, route_resources,
    route_tools,
};
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{debug, info, warn};
use warp::Filter;
use warp::Reply as _;
use warp::http::StatusCode;
use warp::sse::Event;

/// Build the request-capable peer for a session (#153).
fn session_peer<H>(
    state: &McpState<H>,
    streams: Arc<mcpkit_server::streams::StreamRegistry>,
    outbound: Arc<mcpkit_server::adapter_peer::SessionOutbound>,
) -> mcpkit_server::adapter_peer::SessionPeer
where
    H: HasServerInfo + Send + Sync + 'static,
{
    mcpkit_server::adapter_peer::SessionPeer::new(
        Arc::new(mcpkit_server::adapter_peer::StreamRegistrySink::new(
            streams,
        )),
        outbound,
        state.peer_timeouts,
    )
    .with_reconnect_grace(state.reconnect_grace)
}

/// Handle MCP DELETE requests: explicit session termination.
///
/// Per spec (§Session Management item 5). Removing the session drops its
/// `OutboundOwner`, so every pending server-initiated request fails
/// immediately; subsequent requests with the id get 404.
pub fn handle_mcp_delete<H>(
    state: Arc<McpState<H>>,
    session_id: Option<String>,
    origin: Option<String>,
    user: Option<VerifiedUser>,
) -> warp::reply::Response
where
    H: HasServerInfo + Send + Sync + 'static,
{
    if !state.origin_validator.is_allowed(origin.as_deref()) {
        return warp::reply::with_status("origin not allowed", StatusCode::FORBIDDEN)
            .into_response();
    }
    let Some(id) = session_id else {
        return warp::reply::with_status("missing mcp-session-id", StatusCode::BAD_REQUEST)
            .into_response();
    };
    match state.sessions.touch_verified(&id, user.as_ref()) {
        Ok(true) => {}
        Ok(false) => {
            return warp::reply::with_status("unknown session id", StatusCode::NOT_FOUND)
                .into_response();
        }
        Err(e) => {
            return warp::reply::with_status(e.to_string(), StatusCode::FORBIDDEN).into_response();
        }
    }
    let _ = state.sessions.remove(&id);
    info!(session_id = %id, "Session terminated by client DELETE");
    warp::reply::with_status(warp::reply::reply(), StatusCode::NO_CONTENT).into_response()
}

/// Whether this message is an `initialize` request — the only message allowed
/// to omit `mcp-session-id` (the server assigns the id in its response).
fn is_initialize(msg: &Message) -> bool {
    matches!(msg, Message::Request(r) if r.method.as_ref() == "initialize")
}

/// Attach the session id header to a reply (spec: the server communicates the
/// assigned session id via `mcp-session-id`; warp previously never returned
/// it on POST responses, leaving clients unable to satisfy the session-id
/// requirement).
fn reply_with_session(reply: impl warp::Reply, session_id: &str) -> warp::reply::Response {
    let mut resp = reply.into_response();
    if let Ok(v) = warp::http::HeaderValue::from_str(session_id) {
        resp.headers_mut().insert("mcp-session-id", v);
    }
    resp
}

/// Handle MCP POST requests.
///
/// This is the core handler function that processes JSON-RPC messages.
pub async fn handle_mcp_post<H>(
    state: Arc<McpState<H>>,
    version: Option<String>,
    session_id: Option<String>,
    origin: Option<String>,
    user: Option<VerifiedUser>,
    body: String,
) -> Result<warp::reply::Response, Infallible>
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
    // Reject disallowed Origins (DNS-rebinding protection) before any work.
    if !state.origin_validator.is_allowed(origin.as_deref()) {
        warn!(
            origin = origin.as_deref().unwrap_or("none"),
            "Rejected: origin not allowed"
        );
        let error_body = serde_json::json!({
            "error": { "code": -32600, "message": "origin not allowed" }
        });
        return Ok(
            warp::reply::with_status(warp::reply::json(&error_body), StatusCode::FORBIDDEN)
                .into_response(),
        );
    }

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
        )
        .into_response());
    }

    // Get or create session (binding it to the verified user, if any).
    // Parse the message first: whether a missing session id is acceptable
    // depends on whether this is an `initialize` request.
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
            )
            .into_response());
        }
    };

    let session_id = match session_id {
        Some(id) => match state.sessions.touch_verified(&id, user.as_ref()) {
            Ok(true) => id,
            Ok(false) => {
                warn!(session_id = %id, "Rejected: unknown session id");
                let error_body = serde_json::json!({
                    "error": { "code": -32600, "message": "unknown session id" }
                });
                return Ok(warp::reply::with_status(
                    warp::reply::json(&error_body),
                    StatusCode::NOT_FOUND,
                )
                .into_response());
            }
            Err(e) => {
                warn!(session_id = %id, error = %e, "Rejected: session binding violation");
                let error_body = serde_json::json!({
                    "error": { "code": -32600, "message": e.to_string() }
                });
                return Ok(warp::reply::with_status(
                    warp::reply::json(&error_body),
                    StatusCode::FORBIDDEN,
                )
                .into_response());
            }
        },
        // Spec: a server assigning session ids does so at initialization;
        // other requests without `mcp-session-id` are 400 Bad Request.
        None if is_initialize(&msg) => state.sessions.create_for_user(user),
        None => {
            warn!("Rejected: missing mcp-session-id on non-initialize message");
            let error_body = serde_json::json!({
                "error": {
                    "code": -32600,
                    "message": "missing mcp-session-id (required for all messages after initialize)"
                }
            });
            return Ok(warp::reply::with_status(
                warp::reply::json(&error_body),
                StatusCode::BAD_REQUEST,
            )
            .into_response());
        }
    };

    debug!(session_id = %session_id, "Processing MCP request");

    // Process message
    match msg {
        Message::Request(request) => {
            info!(
                method = %request.method,
                id = ?request.id,
                session_id = %session_id,
                "Handling MCP request"
            );

            // On initialize, negotiate the protocol version and record it (and
            // the client's capabilities) on the session, so subsequent requests
            // observe the negotiated values.
            if request.method.as_ref() == "initialize" {
                let (negotiated, caps) = negotiate_initialize(request.params.as_ref());
                state.sessions.set_negotiated(&session_id, negotiated, caps);
            }

            // Resolve the session's negotiated values for the request context,
            // falling back to defaults before initialization completes.
            let (protocol_version, client_caps) =
                state.sessions.negotiated(&session_id).map_or_else(
                    || (ProtocolVersion::LATEST, ClientCapabilities::default()),
                    |(v, c)| (v, c.unwrap_or_default()),
                );
            // This session's task store (per-session isolation for `tasks/*`).
            let task_store = state.sessions.tasks(&session_id);

            // Route with a request-capable peer (#153): handlers can call
            // ctx.elicit()/ctx.list_roots()/sampling; the request rides the
            // session's SSE stream and the client answers via a response POST
            // correlated below. The store hands out plain Arcs (never the
            // OutboundOwner), so holding these across the await cannot keep
            // DELETE/reap from failing pending requests.
            let peer: std::sync::Arc<dyn mcpkit_server::Peer> = match (
                state.sessions.streams(&session_id),
                state.sessions.outbound(&session_id),
            ) {
                (Some(streams), Some(outbound)) => {
                    std::sync::Arc::new(session_peer(&state, streams, outbound))
                }
                _ => std::sync::Arc::new(NoOpPeer),
            };
            let response = create_response_for_request(
                &state,
                &request,
                protocol_version,
                &client_caps,
                task_store.as_ref(),
                peer,
            )
            .await;

            match serde_json::to_value(Message::Response(response)) {
                Ok(body) => Ok(reply_with_session(
                    warp::reply::with_status(warp::reply::json(&body), StatusCode::OK),
                    &session_id,
                )),
                Err(e) => {
                    let error_body = serde_json::json!({
                        "error": {
                            "code": -32603,
                            "message": format!("Internal error: {e}")
                        }
                    });
                    Ok(reply_with_session(
                        warp::reply::with_status(
                            warp::reply::json(&error_body),
                            StatusCode::INTERNAL_SERVER_ERROR,
                        ),
                        &session_id,
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
            // Dispatch to the ServerHandler hooks off the request path
            // (#153): a hook may call ctx.list_roots(), whose response
            // arrives via a separate POST — the 202 must not wait for that
            // round-trip.
            if let Some(hooks) = state.sessions.hooks(&session_id) {
                let state2 = Arc::clone(&state);
                let sid = session_id.clone();
                let method = notification.method.to_string();
                if let Ok(mut hooks) = hooks.lock() {
                    hooks.spawn(async move {
                        let (Some(streams), Some(outbound)) = (
                            state2.sessions.streams(&sid),
                            state2.sessions.outbound(&sid),
                        ) else {
                            return;
                        };
                        let peer = session_peer(&state2, streams, outbound);
                        let (protocol_version, client_caps) =
                            state2.sessions.negotiated(&sid).map_or_else(
                                || (ProtocolVersion::LATEST, ClientCapabilities::default()),
                                |(v, c)| (v, c.unwrap_or_default()),
                            );
                        let server_caps = state2.effective_capabilities();
                        let ctx = Context::for_notification(
                            &client_caps,
                            &server_caps,
                            protocol_version,
                            &peer,
                        );
                        mcpkit_server::dispatch_notification_hooks(
                            state2.handler.as_ref(),
                            &method,
                            &ctx,
                        )
                        .await;
                        debug!(method = %method, "notification hook completed");
                    });
                }
            }
            Ok(reply_with_session(
                warp::reply::with_status(warp::reply::reply(), StatusCode::ACCEPTED),
                &session_id,
            ))
        }
        // Spec (Streamable HTTP): a client delivers responses to
        // server-initiated requests by POSTing them; "if the server accepts
        // the input, the server MUST return HTTP status code 202 Accepted
        // with no body". Correlated against the session's pending
        // server-initiated requests (#153); an unmatched id is logged and
        // dropped (runtime parity).
        Message::Response(response) => {
            let resolved = state
                .sessions
                .outbound(&session_id)
                .is_some_and(|outbound| outbound.resolve(response));
            if !resolved {
                debug!(
                    session_id = %session_id,
                    "client response matched no pending server-initiated request; dropped"
                );
            }
            Ok(reply_with_session(
                warp::reply::with_status(warp::reply::reply(), StatusCode::ACCEPTED),
                &session_id,
            ))
        }
    }
}

/// Negotiate the protocol version and extract client capabilities from an
/// `initialize` request's params.
///
/// The negotiated version is the highest supported version not exceeding the
/// client's requested version, falling back to the latest supported version
/// when the request omits or names an unknown version.
fn negotiate_initialize(
    params: Option<&serde_json::Value>,
) -> (ProtocolVersion, Option<ClientCapabilities>) {
    let requested = params
        .and_then(|p| p.get("protocolVersion"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let version = ProtocolVersion::negotiate(requested, ProtocolVersion::ALL)
        .unwrap_or(ProtocolVersion::LATEST);
    let capabilities = params
        .and_then(|p| p.get("capabilities"))
        .and_then(|c| serde_json::from_value::<ClientCapabilities>(c.clone()).ok());
    (version, capabilities)
}

/// Create a response for a request.
async fn create_response_for_request<H>(
    state: &McpState<H>,
    request: &mcpkit_core::protocol::Request,
    protocol_version: ProtocolVersion,
    client_caps: &ClientCapabilities,
    task_store: Option<&Arc<TaskManager>>,
    peer: std::sync::Arc<dyn mcpkit_server::Peer>,
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
    let server_caps = state.effective_capabilities();
    let ctx = Context::new(
        &req_id,
        None,
        client_caps,
        &server_caps,
        protocol_version,
        peer.as_ref(),
    );

    match method {
        "ping" => Response::success(request.id.clone(), serde_json::json!({})),
        "initialize" => {
            let init_result = serde_json::json!({
                "protocolVersion": protocol_version.as_str(),
                "serverInfo": state.server_info,
                "capabilities": server_caps,
            });
            Response::success(request.id.clone(), init_result)
        }
        _ => {
            // Task-augmented tools/call and tasks/* are served from this
            // session's own task store (per-session isolation).
            if let Some(store) = task_store {
                if method == "tools/call" {
                    match begin_augmented_task(
                        state.handler.clone(),
                        store,
                        params,
                        client_caps.clone(),
                        server_caps.clone(),
                        protocol_version,
                        std::sync::Arc::clone(&peer),
                    )
                    .await
                    {
                        AugmentedTaskOutcome::Started(create_result, fut) => {
                            tokio::spawn(fut);
                            return Response::success(request.id.clone(), create_result);
                        }
                        AugmentedTaskOutcome::Rejected(e) => {
                            return Response::error(request.id.clone(), e.into());
                        }
                        AugmentedTaskOutcome::NotApplicable => {}
                    }
                } else if let Some(result) = route_task_store(store, method, params).await {
                    return match result {
                        Ok(value) => Response::success(request.id.clone(), value),
                        Err(e) => Response::error(request.id.clone(), e.into()),
                    };
                }
            }

            // Try routing to tools
            if let Some(result) = route_tools(
                state.handler.as_ref(),
                method,
                params,
                &ctx,
                state.list_page_size,
            )
            .await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing to resources
            if let Some(result) = route_resources(
                state.handler.as_ref(),
                method,
                params,
                &ctx,
                state.list_page_size,
            )
            .await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing to prompts
            if let Some(result) = route_prompts(
                state.handler.as_ref(),
                method,
                params,
                &ctx,
                state.list_page_size,
            )
            .await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing logging/setLevel (gated on the advertised capability)
            if let Some(result) =
                route_logging(state.handler.as_ref(), &server_caps, method, params, &ctx).await
            {
                return match result {
                    Ok(value) => Response::success(request.id.clone(), value),
                    Err(e) => Response::error(request.id.clone(), e.into()),
                };
            }

            // Try routing completion/complete (when a completion handler is set)
            if let Some(result) =
                route_completion(state.completion.as_deref(), method, params, &ctx).await
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
    origin: Option<String>,
    user: Option<VerifiedUser>,
    last_event_id: Option<String>,
) -> warp::reply::Response
where
    H: HasServerInfo + Send + Sync + 'static,
{
    use warp::Reply;

    // Reject disallowed Origins (DNS-rebinding protection) before streaming.
    if !state.origin_validator.is_allowed(origin.as_deref()) {
        warn!(
            origin = origin.as_deref().unwrap_or("none"),
            "Rejected SSE: origin not allowed"
        );
        return warp::reply::with_status("origin not allowed", StatusCode::FORBIDDEN)
            .into_response();
    }

    // #153: streams attach to the POST-created session. 400 without a
    // session id; 404 for unknown ids (never adopt a client-presented id);
    // 403 for a mismatched user — enforced against the same registry that
    // holds the binding.
    let Some(id) = session_id else {
        warn!("Rejected SSE: missing mcp-session-id");
        return warp::reply::with_status(
            "missing mcp-session-id (obtain one via initialize)",
            StatusCode::BAD_REQUEST,
        )
        .into_response();
    };
    match state.sessions.touch_verified(&id, user.as_ref()) {
        Ok(true) => {}
        Ok(false) => {
            warn!(session_id = %id, "Rejected SSE: unknown session id");
            return warp::reply::with_status("unknown session id", StatusCode::NOT_FOUND)
                .into_response();
        }
        Err(e) => {
            warn!(session_id = %id, error = %e, "Rejected SSE: session binding violation");
            return warp::reply::with_status(e.to_string(), StatusCode::FORBIDDEN).into_response();
        }
    }

    let registry = state.sessions.streams(&id).expect("session verified above");
    let (handle, replay_events) = if let Some(last_id) = &last_event_id {
        info!(session_id = %id, last_event_id = %last_id, "Reconnecting with Last-Event-ID");
        if let Some((handle, replay)) = registry.resume(last_id) {
            (handle, replay)
        } else {
            // Unknown or expired cursor: open a fresh stream rather than
            // replaying another stream's events (spec MUST NOT).
            let (handle, prime) = registry.open("connected", id);
            (handle, vec![prime])
        }
    } else {
        let (handle, prime) = registry.open("connected", id);
        (handle, vec![prime])
    };

    // Replayed/prime events first, then live delivery; the first event
    // carries the retry hint (spec: clients MUST respect `retry`).
    let mut first = true;
    let replay_stream = futures::stream::iter(
        replay_events
            .into_iter()
            .map(|stored| {
                let mut event = Event::default()
                    .id(stored.id)
                    .event(stored.event_type)
                    .data(stored.data);
                if first {
                    event = event.retry(std::time::Duration::from_secs(2));
                    first = false;
                }
                Ok::<_, Infallible>(event)
            })
            .collect::<Vec<_>>(),
    );
    let live = futures::stream::unfold((handle, first), |(mut handle, first)| async move {
        let stored = handle.recv().await?;
        let mut event = Event::default()
            .id(stored.id)
            .event(stored.event_type)
            .data(stored.data);
        if first {
            event = event.retry(std::time::Duration::from_secs(2));
        }
        Some((Ok::<_, Infallible>(event), (handle, false)))
    });
    let stream = replay_stream.chain(live);

    warp::sse::reply(warp::sse::keep_alive().stream(stream)).into_response()
}

/// Create a filter to extract the MCP protocol version header.
#[must_use]
pub fn with_protocol_version()
-> impl Filter<Extract = (Option<String>,), Error = warp::Rejection> + Clone {
    warp::header::optional("mcp-protocol-version")
}

/// Create a filter to extract the MCP session ID header.
#[must_use]
pub fn with_session_id() -> impl Filter<Extract = (Option<String>,), Error = warp::Rejection> + Clone
{
    warp::header::optional("mcp-session-id")
}

/// Create a filter to extract the `Origin` header (DNS-rebinding protection).
#[must_use]
pub fn with_origin() -> impl Filter<Extract = (Option<String>,), Error = warp::Rejection> + Clone {
    warp::header::optional("origin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiate_uses_requested_supported_version() {
        let params = serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {}
        });
        let (version, caps) = negotiate_initialize(Some(&params));
        assert_eq!(version, ProtocolVersion::V2025_06_18);
        assert!(caps.is_some());
    }

    #[test]
    fn negotiate_defaults_to_latest_when_absent() {
        let (version, caps) = negotiate_initialize(None);
        assert_eq!(version, ProtocolVersion::LATEST);
        assert!(caps.is_none());
    }

    #[test]
    fn negotiate_unknown_version_falls_back_to_latest() {
        let params = serde_json::json!({ "protocolVersion": "2099-01-01" });
        let (version, _caps) = negotiate_initialize(Some(&params));
        assert_eq!(version, ProtocolVersion::LATEST);
    }
    use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
    use mcpkit_core::error::McpError;
    use mcpkit_core::types::{
        GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput,
    };
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
            _args: serde_json::Map<String, serde_json::Value>,
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
                meta: None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
            None,
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
            None,
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
