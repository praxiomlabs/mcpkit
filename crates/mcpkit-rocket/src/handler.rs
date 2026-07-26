//! HTTP handlers for MCP requests using Rocket.

use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
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
use rocket::http::{ContentType, Header, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::stream::{Event, EventStream};
use rocket::response::{self, Responder, Response};
use std::io::Cursor;
use std::sync::Arc;
use tracing::{debug, info, warn};

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
    state: &McpState<H>,
    session_id: Option<String>,
    origin: Option<&str>,
    user: Option<VerifiedUser>,
) -> Status
where
    H: HasServerInfo + Send + Sync + 'static,
{
    if !state.origin_validator.is_allowed(origin) {
        return Status::Forbidden;
    }
    let Some(id) = session_id else {
        return Status::BadRequest;
    };
    match state.sessions.touch_verified(&id, user.as_ref()) {
        Ok(true) => {}
        Ok(false) => return Status::NotFound,
        Err(_) => return Status::Forbidden,
    }
    let _ = state.sessions.remove(&id);
    info!(session_id = %id, "Session terminated by client DELETE");
    Status::NoContent
}

/// Whether this message is an `initialize` request — the only message allowed
/// to omit `mcp-session-id` (the server assigns the id in its response).
fn is_initialize(msg: &Message) -> bool {
    matches!(msg, Message::Request(r) if r.method.as_ref() == "initialize")
}

/// MCP protocol version header.
pub struct ProtocolVersionHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ProtocolVersionHeader {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let version = request
            .headers()
            .get_one("mcp-protocol-version")
            .map(String::from);
        Outcome::Success(ProtocolVersionHeader(version))
    }
}

/// MCP session ID header.
pub struct SessionIdHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for SessionIdHeader {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let session_id = request
            .headers()
            .get_one("mcp-session-id")
            .map(String::from);
        Outcome::Success(SessionIdHeader(session_id))
    }
}

/// The verified user for this request.
///
/// An application's authentication fairing validates the bearer token and caches
/// the resulting identity with `request.local_cache(|| Some(user))` (or `None`);
/// this guard reads it back so mcpkit can bind the session to it.
pub struct VerifiedUserGuard(pub Option<VerifiedUser>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for VerifiedUserGuard {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let user = request.local_cache(|| None::<VerifiedUser>).clone();
        Outcome::Success(Self(user))
    }
}

/// `Origin` header, for DNS-rebinding protection.
pub struct OriginHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for OriginHeader {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let origin = request.headers().get_one("origin").map(String::from);
        Outcome::Success(OriginHeader(origin))
    }
}

/// Last-Event-ID header for SSE reconnection.
pub struct LastEventIdHeader(pub Option<String>);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for LastEventIdHeader {
    type Error = ();

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let last_event_id = request.headers().get_one("last-event-id").map(String::from);
        Outcome::Success(LastEventIdHeader(last_event_id))
    }
}

/// Response wrapper for MCP POST requests.
pub struct McpResponse {
    status: Status,
    content_type: ContentType,
    session_id: Option<String>,
    body: String,
}

impl McpResponse {
    /// Create a success response.
    #[must_use]
    pub fn success(body: String, session_id: String) -> Self {
        Self {
            status: Status::Ok,
            content_type: ContentType::JSON,
            session_id: Some(session_id),
            body,
        }
    }

    /// Create an accepted response (for notifications).
    #[must_use]
    pub fn accepted(session_id: String) -> Self {
        Self {
            status: Status::Accepted,
            content_type: ContentType::JSON,
            session_id: Some(session_id),
            body: String::new(),
        }
    }

    /// Create an error response.
    #[must_use]
    pub fn error(status: Status, message: String) -> Self {
        Self {
            status,
            content_type: ContentType::JSON,
            session_id: None,
            body: serde_json::json!({
                "error": {
                    "code": -32600,
                    "message": message
                }
            })
            .to_string(),
        }
    }
}

impl<'r> Responder<'r, 'static> for McpResponse {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        let mut builder = Response::build();
        builder.status(self.status);
        builder.header(self.content_type);

        if let Some(session_id) = self.session_id {
            builder.header(Header::new("mcp-session-id", session_id));
        }

        if !self.body.is_empty() {
            builder.sized_body(self.body.len(), Cursor::new(self.body));
        }

        builder.ok()
    }
}

/// Handler context wrapping the generic handler type.
pub struct HandlerContext<H> {
    inner: Arc<H>,
}

impl<H> Clone for HandlerContext<H> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<H> HandlerContext<H> {
    /// Create a new handler context.
    pub fn new(handler: H) -> Self {
        Self {
            inner: Arc::new(handler),
        }
    }

    /// Get a reference to the inner handler.
    #[must_use]
    pub fn handler(&self) -> &H {
        &self.inner
    }
}

/// Handle MCP POST requests.
///
/// This is the core handler function that processes JSON-RPC messages.
pub async fn handle_mcp_post<H>(
    state: &McpState<H>,
    version: Option<&str>,
    session_id: Option<String>,
    origin: Option<&str>,
    user: Option<VerifiedUser>,
    body: &str,
) -> McpResponse
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
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected: origin not allowed"
        );
        return McpResponse::error(Status::Forbidden, "origin not allowed".to_string());
    }

    // Validate protocol version
    if !is_supported_version(version) {
        let provided = version.unwrap_or("none");
        warn!(version = provided, "Unsupported protocol version");
        return McpResponse::error(
            Status::BadRequest,
            format!(
                "Unsupported protocol version: {} (supported: {})",
                provided,
                SUPPORTED_VERSIONS.join(", ")
            ),
        );
    }

    // Parse the message first: whether a missing session id is acceptable
    // depends on whether this is an `initialize` request.
    let msg: Message = match serde_json::from_str(body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            return McpResponse::error(Status::BadRequest, format!("Invalid message: {e}"));
        }
    };

    // Get or create session (binding it to the verified user, if any).
    let session_id = match session_id {
        Some(id) => match state.sessions.touch_verified(&id, user.as_ref()) {
            Ok(true) => id,
            // Reject an unknown session id rather than silently proceeding.
            Ok(false) => {
                warn!(session_id = %id, "Rejected: unknown session id");
                return McpResponse::error(Status::NotFound, "unknown session id".to_string());
            }
            Err(e) => {
                warn!(session_id = %id, error = %e, "Rejected: session binding violation");
                return McpResponse::error(Status::Forbidden, e.to_string());
            }
        },
        // Spec: a server assigning session ids does so at initialization;
        // other requests without `mcp-session-id` are 400 Bad Request.
        None if is_initialize(&msg) => state.sessions.create_for_user(user),
        None => {
            warn!("Rejected: missing mcp-session-id on non-initialize message");
            return McpResponse::error(
                Status::BadRequest,
                "missing mcp-session-id (required for all messages after initialize)".to_string(),
            );
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
                    std::sync::Arc::new(session_peer(state, streams, outbound))
                }
                _ => std::sync::Arc::new(NoOpPeer),
            };
            let response = create_response_for_request(
                state,
                &request,
                protocol_version,
                &client_caps,
                task_store.as_ref(),
                peer,
            )
            .await;

            match serde_json::to_string(&Message::Response(response)) {
                Ok(body) => McpResponse::success(body, session_id),
                Err(e) => McpResponse::error(
                    Status::InternalServerError,
                    format!("Serialization error: {e}"),
                ),
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
                let state2 = state.clone();
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
            McpResponse::accepted(session_id)
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
            McpResponse::accepted(session_id)
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
                } else if let Some(result) = route_task_store(store, method, params)
                    .await
                    .or_unknown_task()
                {
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

/// Handle SSE connections for server-to-client streaming (GET on the MCP
/// endpoint, #153).
///
/// Streams attach to the POST-created session: 400 without a session id;
/// 404 for unknown ids (never adopt a client-presented id); 403 for a
/// disallowed origin or mismatched user — enforced against the same
/// registry that holds the binding. `Last-Event-ID` resumes the stream with
/// same-stream replay; an unknown or expired cursor opens a fresh stream.
pub fn handle_sse<H>(
    state: &McpState<H>,
    session_id: Option<String>,
    origin: Option<&str>,
    user: Option<VerifiedUser>,
    last_event_id: Option<String>,
) -> Result<EventStream![], Status>
where
    H: HasServerInfo + Send + Sync + 'static,
{
    // Reject disallowed Origins (DNS-rebinding protection) before streaming.
    if !state.origin_validator.is_allowed(origin) {
        warn!(
            origin = origin.unwrap_or("none"),
            "Rejected SSE: origin not allowed"
        );
        return Err(Status::Forbidden);
    }
    let Some(id) = session_id else {
        warn!("Rejected SSE: missing mcp-session-id");
        return Err(Status::BadRequest);
    };
    match state.sessions.touch_verified(&id, user.as_ref()) {
        Ok(true) => {}
        Ok(false) => {
            warn!(session_id = %id, "Rejected SSE: unknown session id");
            return Err(Status::NotFound);
        }
        Err(e) => {
            warn!(session_id = %id, error = %e, "Rejected SSE: session binding violation");
            return Err(Status::Forbidden);
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
    Ok(EventStream! {
        let mut handle = handle;
        let mut first = true;
        for stored in replay_events {
            let mut event = Event::data(stored.data).event(stored.event_type).id(stored.id);
            if first {
                event = event.with_retry(std::time::Duration::from_secs(2));
                first = false;
            }
            yield event;
        }
        while let Some(stored) = handle.recv().await {
            let mut event = Event::data(stored.data).event(stored.event_type).id(stored.id);
            if first {
                event = event.with_retry(std::time::Duration::from_secs(2));
                first = false;
            }
            yield event;
        }
    })
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

    // Test HandlerContext
    struct TestHandler {
        name: String,
    }

    #[test]
    fn test_handler_context_creation() {
        let handler = TestHandler {
            name: "test".to_string(),
        };
        let ctx = HandlerContext::new(handler);
        assert_eq!(ctx.handler().name, "test");
    }

    #[test]
    fn test_handler_context_clone() {
        let handler = TestHandler {
            name: "test".to_string(),
        };
        let ctx = HandlerContext::new(handler);
        let cloned = ctx.clone();

        // Both should reference the same Arc
        assert_eq!(ctx.handler().name, cloned.handler().name);
    }

    // Test McpResponse
    #[test]
    fn test_mcp_response_success() {
        let response =
            McpResponse::success(r#"{"result":"ok"}"#.to_string(), "session-123".to_string());
        assert_eq!(response.status, Status::Ok);
        assert_eq!(response.content_type, ContentType::JSON);
        assert_eq!(response.session_id, Some("session-123".to_string()));
        assert_eq!(response.body, r#"{"result":"ok"}"#);
    }

    #[test]
    fn test_mcp_response_accepted() {
        let response = McpResponse::accepted("session-456".to_string());
        assert_eq!(response.status, Status::Accepted);
        assert_eq!(response.content_type, ContentType::JSON);
        assert_eq!(response.session_id, Some("session-456".to_string()));
        assert!(response.body.is_empty());
    }

    #[test]
    fn test_mcp_response_error() {
        let response = McpResponse::error(Status::BadRequest, "Invalid request".to_string());
        assert_eq!(response.status, Status::BadRequest);
        assert_eq!(response.content_type, ContentType::JSON);
        assert!(response.session_id.is_none());
        assert!(response.body.contains("Invalid request"));
        assert!(response.body.contains("-32600"));
    }

    // Test header types
    #[test]
    fn test_protocol_version_header_with_value() {
        let header = ProtocolVersionHeader(Some("2025-11-25".to_string()));
        assert_eq!(header.0, Some("2025-11-25".to_string()));
    }

    #[test]
    fn test_protocol_version_header_without_value() {
        let header = ProtocolVersionHeader(None);
        assert!(header.0.is_none());
    }

    #[test]
    fn test_session_id_header_with_value() {
        let header = SessionIdHeader(Some("abc-123".to_string()));
        assert_eq!(header.0, Some("abc-123".to_string()));
    }

    #[test]
    fn test_session_id_header_without_value() {
        let header = SessionIdHeader(None);
        assert!(header.0.is_none());
    }

    #[test]
    fn test_last_event_id_header_with_value() {
        let header = LastEventIdHeader(Some("evt-999".to_string()));
        assert_eq!(header.0, Some("evt-999".to_string()));
    }

    #[test]
    fn test_last_event_id_header_without_value() {
        let header = LastEventIdHeader(None);
        assert!(header.0.is_none());
    }
}
