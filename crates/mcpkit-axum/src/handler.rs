//! HTTP handlers for MCP requests.
//!
//! # Authenticated sessions
//!
//! To bind sessions to a verified user (MCP security best practices), have your
//! authentication middleware validate the bearer token and insert a
//! [`mcpkit_core::auth::VerifiedUser`] into the request extensions (e.g. with a
//! `tower` layer that calls `req.extensions_mut().insert(user)`). The handlers
//! read it and bind the session: a session created for a user may then only be
//! used by that same user, and a request presenting a mismatched, missing, or
//! unexpected identity is rejected. Requests without a `VerifiedUser` extension
//! are treated as anonymous. Token validation stays your application's
//! responsibility; `VerifiedUser::from_claims` builds one from validated JWT
//! claims.

use crate::error::ExtensionError;
use crate::state::{HasServerInfo, McpState, OAuthState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Extension, Json};
use futures::stream::Stream;
use mcpkit_core::auth::VerifiedUser;
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol::Message;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::capability::tasks::{TaskManager, route_task_store};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::streams::StoredEvent;
use mcpkit_server::{
    AugmentedTaskOutcome, PromptHandler, ResourceHandler, ServerHandler, ToolHandler,
    begin_augmented_task, route_completion, route_logging, route_prompts, route_resources,
    route_tools,
};
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Delivers peer messages onto the session's SSE stream registry.
struct SessionStreamSink {
    registry: Arc<mcpkit_server::streams::StreamRegistry>,
}

impl mcpkit_server::adapter_peer::SessionSink for SessionStreamSink {
    fn send_notification(
        &self,
        message: Message,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), mcpkit_server::adapter_peer::SinkError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let json = serde_json::to_string(&message).map_err(|e| {
                mcpkit_server::adapter_peer::SinkError::Serialization(e.to_string())
            })?;
            // Best-effort: with no live stream the notification is dropped
            // (runtime parity — a client without a stream misses it).
            let _ = self.registry.send("message", json);
            Ok(())
        })
    }

    fn send_request(
        &self,
        message: Message,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), mcpkit_server::adapter_peer::SinkError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let json = serde_json::to_string(&message).map_err(|e| {
                mcpkit_server::adapter_peer::SinkError::Serialization(e.to_string())
            })?;
            self.registry
                .send("message", json)
                .map(|_| ())
                .ok_or(mcpkit_server::adapter_peer::SinkError::NoClientStream)
        })
    }

    fn has_live_stream(&self) -> bool {
        self.registry.has_live_stream()
    }
}

/// Build the request-capable peer for a session (#153 PR 4).
///
/// Takes the session's parts rather than a `Session` snapshot so callers can
/// drop the snapshot before awaiting: a snapshot holds the session's
/// `Arc<OutboundOwner>`, and a request awaiting a peer response while holding
/// one would keep the owner alive — preventing DELETE/reap from failing that
/// very request.
fn session_peer<H>(
    state: &McpState<H>,
    streams: Arc<mcpkit_server::streams::StreamRegistry>,
    outbound: Arc<mcpkit_server::adapter_peer::SessionOutbound>,
) -> mcpkit_server::adapter_peer::SessionPeer
where
    H: HasServerInfo + Send + Sync + 'static,
{
    mcpkit_server::adapter_peer::SessionPeer::new(
        Arc::new(SessionStreamSink { registry: streams }),
        outbound,
        state.peer_timeouts,
    )
    .with_reconnect_grace(state.reconnect_grace)
}

/// Whether this message is an `initialize` request — the only message allowed
/// to omit `mcp-session-id` (the server assigns the id in its response).
fn is_initialize(msg: &Message) -> bool {
    matches!(msg, Message::Request(r) if r.method.as_ref() == "initialize")
}

/// Handle MCP POST requests.
///
/// This handler processes JSON-RPC messages sent via HTTP POST.
///
/// # Headers
///
/// - `mcp-protocol-version`: Optional. If present, must name a supported
///   protocol version; if absent, `2025-03-26` is assumed for backwards
///   compatibility.
/// - `mcp-session-id`: Optional. Used to track sessions.
/// - `Content-Type`: Should be `application/json`.
///
/// # Response
///
/// Returns a JSON-RPC response for request messages, or 202 Accepted for notifications.
pub async fn handle_mcp_post<H>(
    State(state): State<McpState<H>>,
    headers: HeaderMap,
    user: Option<Extension<VerifiedUser>>,
    body: String,
) -> impl IntoResponse
where
    H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler + Send + Sync + 'static,
{
    // The verified user (if any) is supplied by the application's auth middleware
    // via a request extension; mcpkit binds the session to it.
    let user = user.map(|Extension(u)| u);
    // Reject disallowed Origins (DNS-rebinding protection) before any work.
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected: origin not allowed"
        );
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }

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

    // Parse the message first: whether a missing session id is acceptable
    // depends on whether this is an `initialize` request.
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            return ExtensionError::InvalidMessage(e.to_string()).into_response();
        }
    };

    // Get or create session
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let session_id = match session_id {
        Some(id) => match state.sessions.touch_verified(&id, user.as_ref()) {
            Ok(true) => id,
            // Reject an unknown session id rather than silently proceeding.
            Ok(false) => {
                warn!(session_id = %id, "Rejected: unknown session id");
                return ExtensionError::SessionNotFound(id).into_response();
            }
            Err(e) => {
                warn!(session_id = %id, error = %e, "Rejected: session binding violation");
                return (StatusCode::FORBIDDEN, e.to_string()).into_response();
            }
        },
        // Spec: a server assigning session ids does so at initialization;
        // other requests without `mcp-session-id` are 400 Bad Request.
        None if is_initialize(&msg) => state.sessions.create_for_user(user),
        None => {
            warn!("Rejected: missing mcp-session-id on non-initialize message");
            return (
                StatusCode::BAD_REQUEST,
                "missing mcp-session-id (required for all messages after initialize)",
            )
                .into_response();
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
            // observe the negotiated values and the session is no longer subject
            // to the initialization timeout.
            if request.method.as_ref() == "initialize" {
                let (version, caps) = negotiate_initialize(request.params.as_ref());
                state
                    .sessions
                    .update(&session_id, |s| s.mark_initialized(version, caps.clone()));
            }

            // Resolve the session's negotiated values for the request context,
            // falling back to defaults before initialization completes.
            let session = state.sessions.get(&session_id);
            let protocol_version = session
                .as_ref()
                .and_then(|s| s.protocol_version)
                .unwrap_or(ProtocolVersion::LATEST);
            // This session's task store (per-session isolation for `tasks/*`).
            let task_store = session.as_ref().map(|s| s.tasks.clone());
            let client_caps = session
                .as_ref()
                .and_then(|s| s.client_capabilities.clone())
                .unwrap_or_default();

            // Route with a request-capable peer (#153 PR 4): handlers can
            // call ctx.elicit()/ctx.list_roots()/sampling; the request rides
            // the session's SSE stream and the client answers via a response
            // POST correlated below.
            let peer: Box<dyn mcpkit_server::Peer> = match &session {
                Some(s) => Box::new(session_peer(
                    &state,
                    Arc::clone(&s.streams),
                    Arc::clone(s.outbound_owner.outbound()),
                )),
                None => Box::new(NoOpPeer),
            };
            // Release the snapshot before awaiting: it holds the session's
            // OutboundOwner, and a request blocked in ctx.elicit() must not
            // keep DELETE/reap from failing its own pending request.
            drop(session);
            let response = create_response_for_request(
                &state,
                &request,
                protocol_version,
                &client_caps,
                task_store.as_ref(),
                peer.as_ref(),
            )
            .await;

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
            // Dispatch to the ServerHandler hooks off the request path
            // (#153 PR 4): a hook may call ctx.list_roots(), whose response
            // arrives via a separate POST — the 202 must not wait for that
            // round-trip. Spawned into the session's JoinSet, whose teardown
            // aborts in-flight hooks (deliberate: the session is gone).
            if let Some(session) = state.sessions.get(&session_id) {
                let state2 = state.clone();
                let sid = session_id.clone();
                let method = notification.method.to_string();
                if let Ok(mut hooks) = session.hooks.lock() {
                    hooks.spawn(async move {
                        let Some(session) = state2.sessions.get(&sid) else {
                            return;
                        };
                        let peer = session_peer(
                            &state2,
                            Arc::clone(&session.streams),
                            Arc::clone(session.outbound_owner.outbound()),
                        );
                        let client_caps = session.client_capabilities.clone().unwrap_or_default();
                        let server_caps = state2.effective_capabilities();
                        let version = session.protocol_version.unwrap_or(ProtocolVersion::LATEST);
                        // Release the snapshot: a hook blocked in
                        // ctx.list_roots() must not hold the OutboundOwner.
                        drop(session);
                        let ctx =
                            Context::for_notification(&client_caps, &server_caps, version, &peer);
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
            (
                StatusCode::ACCEPTED,
                [("mcp-session-id", session_id.as_str())],
            )
                .into_response()
        }
        // Spec (Streamable HTTP): a client delivers responses to
        // server-initiated requests by POSTing them; "if the server accepts
        // the input, the server MUST return HTTP status code 202 Accepted
        // with no body". Correlation with a pending server-initiated request
        // arrives with the session peer (#153); until then this is
        // log-and-drop, matching the runtime's `route_response` for ids that
        // match no pending request.
        Message::Response(response) => {
            // Correlate with a pending server-initiated request (#153 PR 4);
            // an unmatched id is logged and dropped (runtime parity), and the
            // spec-mandated 202 is returned either way.
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
            (
                StatusCode::ACCEPTED,
                [("mcp-session-id", session_id.as_str())],
            )
                .into_response()
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
///
/// Routes all MCP methods through the appropriate handler traits.
async fn create_response_for_request<H>(
    state: &McpState<H>,
    request: &mcpkit_core::protocol::Request,
    protocol_version: ProtocolVersion,
    client_caps: &ClientCapabilities,
    task_store: Option<&Arc<TaskManager>>,
    peer: &dyn mcpkit_server::Peer,
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
        peer,
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
                        // Not task-augmented — fall through to the normal path.
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

/// Handle MCP DELETE requests: explicit session termination.
///
/// Per spec (§Session Management item 5). Removing the session drops its
/// `OutboundOwner`, so every pending server-initiated request fails
/// immediately; subsequent requests with the id get 404.
pub async fn handle_mcp_delete<H>(
    State(state): State<McpState<H>>,
    headers: HeaderMap,
    user: Option<Extension<VerifiedUser>>,
) -> impl IntoResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let user = user.map(|Extension(u)| u);
    let Some(id) = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
    else {
        return (StatusCode::BAD_REQUEST, "missing mcp-session-id").into_response();
    };
    match state.sessions.get_verified(&id, user.as_ref()) {
        Ok(Some(_)) => {}
        Ok(None) => return (StatusCode::NOT_FOUND, "unknown session id").into_response(),
        Err(e) => return (StatusCode::FORBIDDEN, e.to_string()).into_response(),
    }
    let _ = state.sessions.remove(&id);
    info!(session_id = %id, "Session terminated by client DELETE");
    StatusCode::NO_CONTENT.into_response()
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
    user: Option<Extension<VerifiedUser>>,
) -> impl IntoResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    // Reject disallowed Origins (DNS-rebinding protection) before streaming.
    let origin = headers.get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected SSE: origin not allowed"
        );
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }

    let user = user.map(|Extension(u)| u);
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // #153 PR 1: streams attach to the POST-created session. A GET without a
    // session id is 400 (sessions are assigned at initialize); an unknown id
    // is 404 per spec (never adopt a client-presented id); a mismatched user
    // is 403 — enforced against the SAME registry that holds the binding, so
    // the check can no longer pass vacuously (#173).
    let Some(id) = session_id else {
        warn!("Rejected SSE: missing mcp-session-id");
        return (
            StatusCode::BAD_REQUEST,
            "missing mcp-session-id (obtain one via initialize)",
        )
            .into_response();
    };
    match state.sessions.get_verified(&id, user.as_ref()) {
        Ok(Some(_)) => {}
        Ok(None) => {
            warn!(session_id = %id, "Rejected SSE: unknown session id");
            return (StatusCode::NOT_FOUND, "unknown session id").into_response();
        }
        Err(e) => {
            warn!(session_id = %id, error = %e, "Rejected SSE: session binding violation");
            return (StatusCode::FORBIDDEN, e.to_string()).into_response();
        }
    }

    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let registry = state.sessions.streams(&id).expect("session verified above");

    // Resume the named stream (same identity, same-stream replay) or open a
    // fresh one primed with a `connected` event (#153 PR 2).
    let (handle, replay_events) = if let Some(last_id) = &last_event_id {
        info!(session_id = %id, last_event_id = %last_id, "Reconnecting with Last-Event-ID");
        if let Some((handle, replay)) = registry.resume(last_id) {
            (handle, replay)
        } else {
            // Unknown or expired stream cursor: open a fresh stream rather
            // than replaying another stream's events (spec MUST NOT).
            let (handle, prime) = registry.open("connected", id);
            (handle, vec![prime])
        }
    } else {
        let (handle, prime) = registry.open("connected", id);
        (handle, vec![prime])
    };

    let stream = create_sse_stream(handle, replay_events);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Create the SSE event stream for one [`mcpkit_server::streams::StreamHandle`]:
/// replayed events first (each with its stored id), then live events as the
/// registry delivers them. The first event carries a `retry` hint (spec: the
/// client MUST respect `retry`), so client reconnect cadence is dictated
/// rather than guessed.
fn create_sse_stream(
    mut handle: mcpkit_server::streams::StreamHandle,
    replay_events: Vec<StoredEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    async_stream::stream! {
        let mut first = true;
        for stored in replay_events {
            debug!(event_id = %stored.id, "Sending stored event");
            let mut event = Event::default()
                .id(&stored.id)
                .event(&stored.event_type)
                .data(&stored.data);
            if first {
                event = event.retry(std::time::Duration::from_secs(2));
                first = false;
            }
            yield Ok(event);
        }

        // Live delivery: the registry stores each event (id allocated once)
        // before queueing it here, so the wire id always equals the stored id.
        while let Some(stored) = handle.recv().await {
            let mut event = Event::default()
                .id(&stored.id)
                .event(&stored.event_type)
                .data(&stored.data);
            if first {
                event = event.retry(std::time::Duration::from_secs(2));
                first = false;
            }
            yield Ok(event);
        }
        debug!("SSE stream closed (stream killed or session dropped)");
    }
}

/// Handle `.well-known/oauth-protected-resource` requests.
///
/// Per RFC 9728, MCP servers MUST implement this endpoint to indicate
/// the locations of authorization servers that can issue tokens for this resource.
///
/// # Response
///
/// Returns a JSON object containing:
/// - `resource`: The protected resource identifier (server URL)
/// - `authorization_servers`: List of authorization server URLs
/// - `scopes_supported`: Optional list of supported scopes
/// - `bearer_methods_supported`: Token presentation methods (typically `["header"]`)
///
/// # Example Response
///
/// ```json
/// {
///   "resource": "https://mcp.example.com",
///   "authorization_servers": ["https://auth.example.com"],
///   "scopes_supported": ["files:read", "files:write"],
///   "bearer_methods_supported": ["header"]
/// }
/// ```
///
/// # References
///
/// - [RFC 9728: OAuth 2.0 Protected Resource Metadata](https://datatracker.ietf.org/doc/html/rfc9728)
/// - [MCP Authorization Specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
pub async fn handle_oauth_protected_resource(State(state): State<OAuthState>) -> impl IntoResponse {
    debug!("Serving OAuth protected resource metadata");
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        Json(state.metadata),
    )
}

#[cfg(test)]
mod tests {
    use super::negotiate_initialize;
    use mcpkit_core::protocol_version::ProtocolVersion;

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
}
