//! HTTP handlers for MCP requests.

use crate::error::ExtensionError;
use crate::state::{HasServerInfo, McpState, OAuthState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use actix_web::http::header::ContentType;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use futures::stream::{self, StreamExt};
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
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Build the request-capable peer for a session (#153).
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
        Arc::new(mcpkit_server::adapter_peer::StreamRegistrySink::new(
            streams,
        )),
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
    req: HttpRequest,
    state: web::Data<McpState<H>>,
    body: String,
) -> Result<HttpResponse, ExtensionError>
where
    H: ServerHandler + ToolHandler + ResourceHandler + PromptHandler + Send + Sync + 'static,
{
    // Reject disallowed Origins (DNS-rebinding protection) before any work.
    let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected: origin not allowed"
        );
        return Ok(HttpResponse::Forbidden().body("origin not allowed"));
    }

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

    // The verified user (if any) is supplied by the application's auth
    // middleware via a request extension; mcpkit binds the session to it.
    let user = req.extensions().get::<VerifiedUser>().cloned();

    // Parse the message first: whether a missing session id is acceptable
    // depends on whether this is an `initialize` request.
    let msg: Message =
        serde_json::from_str(&body).map_err(|e| ExtensionError::InvalidMessage(e.to_string()))?;

    // Get or create session
    let session_id = req
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let session_id = match session_id {
        Some(id) => match state.sessions.touch_verified(&id, user.as_ref()) {
            Ok(true) => id,
            // Reject an unknown session id rather than silently proceeding.
            Ok(false) => {
                warn!(session_id = %id, "Rejected: unknown session id");
                return Err(ExtensionError::SessionNotFound(id));
            }
            Err(e) => {
                warn!(session_id = %id, error = %e, "Rejected: session binding violation");
                return Ok(HttpResponse::Forbidden().body(e.to_string()));
            }
        },
        // Spec: a server assigning session ids does so at initialization;
        // other requests without `mcp-session-id` are 400 Bad Request.
        None if is_initialize(&msg) => state.sessions.create_for_user(user),
        None => {
            warn!("Rejected: missing mcp-session-id on non-initialize message");
            return Ok(HttpResponse::BadRequest()
                .body("missing mcp-session-id (required for all messages after initialize)"));
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

            // Route with a request-capable peer (#153): handlers can call
            // ctx.elicit()/ctx.list_roots()/sampling; the request rides the
            // session's SSE stream and the client answers via a response POST
            // correlated below.
            let peer: std::sync::Arc<dyn mcpkit_server::Peer> = match &session {
                Some(s) => std::sync::Arc::new(session_peer(
                    &state,
                    Arc::clone(&s.streams),
                    Arc::clone(s.outbound_owner.outbound()),
                )),
                None => std::sync::Arc::new(NoOpPeer),
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
                peer,
            )
            .await;

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
            // Dispatch to the ServerHandler hooks off the request path
            // (#153): a hook may call ctx.list_roots(), whose response
            // arrives via a separate POST — the 202 must not wait for that
            // round-trip.
            if let Some(session) = state.sessions.get(&session_id) {
                let state2 = McpState::clone(&state);
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
            Ok(HttpResponse::Accepted()
                .insert_header(("mcp-session-id", session_id))
                .finish())
        }
        // Spec (Streamable HTTP): a client delivers responses to
        // server-initiated requests by POSTing them; the server MUST return
        // 202 Accepted with no body. Correlated against the session's
        // pending server-initiated requests (#153); an unmatched id is
        // logged and dropped (runtime parity).
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
            Ok(HttpResponse::Accepted()
                .insert_header(("mcp-session-id", session_id))
                .finish())
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

/// Handle MCP DELETE requests: explicit session termination.
///
/// Per spec (§Session Management item 5). Removing the session drops its
/// `OutboundOwner`, so every pending server-initiated request fails
/// immediately; subsequent requests with the id get 404.
pub async fn handle_mcp_delete<H>(req: HttpRequest, state: web::Data<McpState<H>>) -> HttpResponse
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        return HttpResponse::Forbidden().body("origin not allowed");
    }
    let user = req.extensions().get::<VerifiedUser>().cloned();
    let Some(id) = req
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
    else {
        return HttpResponse::BadRequest().body("missing mcp-session-id");
    };
    match state.sessions.get_verified(&id, user.as_ref()) {
        Ok(Some(_)) => {}
        Ok(None) => return HttpResponse::NotFound().body("unknown session id"),
        Err(e) => return HttpResponse::Forbidden().body(e.to_string()),
    }
    let _ = state.sessions.remove(&id);
    info!(session_id = %id, "Session terminated by client DELETE");
    HttpResponse::NoContent().finish()
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
    // Reject disallowed Origins (DNS-rebinding protection) before streaming.
    let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected SSE: origin not allowed"
        );
        return HttpResponse::Forbidden().body("origin not allowed");
    }

    let user = req.extensions().get::<VerifiedUser>().cloned();
    let session_id = req
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // #153: streams attach to the POST-created session. A GET without a
    // session id is 400 (sessions are assigned at initialize); an unknown id
    // is 404 per spec (never adopt a client-presented id); a mismatched user
    // is 403 — enforced against the same registry that holds the binding.
    let Some(id) = session_id else {
        warn!("Rejected SSE: missing mcp-session-id");
        return HttpResponse::BadRequest()
            .body("missing mcp-session-id (obtain one via initialize)");
    };
    match state.sessions.get_verified(&id, user.as_ref()) {
        Ok(Some(_)) => {}
        Ok(None) => {
            warn!(session_id = %id, "Rejected SSE: unknown session id");
            return HttpResponse::NotFound().body("unknown session id");
        }
        Err(e) => {
            warn!(session_id = %id, error = %e, "Rejected SSE: session binding violation");
            return HttpResponse::Forbidden().body(e.to_string());
        }
    }

    let last_event_id = req
        .headers()
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let registry = state.sessions.streams(&id).expect("session verified above");
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

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .streaming(stream)
}

fn sse_frame(stored: &mcpkit_server::streams::StoredEvent, first: bool) -> web::Bytes {
    // The first event of every stream carries a `retry` hint (spec: the
    // client MUST respect `retry`), so reconnect cadence is dictated.
    let retry = if first { "retry: 2000\n" } else { "" };
    web::Bytes::from(format!(
        "{retry}id: {}\nevent: {}\ndata: {}\n\n",
        stored.id, stored.event_type, stored.data
    ))
}

fn create_sse_stream(
    handle: mcpkit_server::streams::StreamHandle,
    replay_events: Vec<mcpkit_server::streams::StoredEvent>,
) -> impl futures::Stream<Item = Result<web::Bytes, actix_web::error::Error>> {
    // Replayed/prime events first (each with its stored id — allocated once,
    // so the wire id always equals the buffered id).
    let mut first = true;
    let replay_frames: Vec<Result<web::Bytes, actix_web::error::Error>> = replay_events
        .iter()
        .map(|stored| {
            let frame = sse_frame(stored, first);
            first = false;
            Ok(frame)
        })
        .collect();
    let replay = stream::iter(replay_frames);

    // Live delivery off the session's per-stream channel.
    let messages = stream::unfold((handle, first), |(mut handle, first)| async move {
        let stored = handle.recv().await?;
        let frame = sse_frame(&stored, first);
        Some((Ok::<_, actix_web::error::Error>(frame), (handle, false)))
    });

    // Periodic keep-alive comments.
    let keepalive = stream::unfold((), |()| async {
        tokio::time::sleep(Duration::from_secs(15)).await;
        Some((
            Ok::<_, actix_web::error::Error>(web::Bytes::from_static(b": keepalive\n\n")),
            (),
        ))
    });

    replay.chain(stream::select(messages, keepalive))
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
pub async fn handle_oauth_protected_resource(
    state: web::Data<OAuthState>,
) -> Result<HttpResponse, ExtensionError> {
    debug!("Serving OAuth protected resource metadata");
    let body = serde_json::to_string(&state.metadata).map_err(ExtensionError::Serialization)?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::json())
        .body(body))
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
