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
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::{
    PromptHandler, ResourceHandler, ServerHandler, ToolHandler, route_logging, route_prompts,
    route_resources, route_tools,
};
use std::time::Duration;
use tracing::{debug, info, warn};

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
        None => state.sessions.create_for_user(user),
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
            let client_caps = session
                .and_then(|s| s.client_capabilities)
                .unwrap_or_default();

            // Create a basic response using the handler's capabilities
            let response =
                create_response_for_request(&state, &request, protocol_version, &client_caps).await;

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
    let server_caps = state.handler.capabilities();
    let peer = NoOpPeer;
    let ctx = Context::new(
        &req_id,
        None,
        client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    match method {
        "ping" => Response::success(request.id.clone(), serde_json::json!({})),
        "initialize" => {
            let init_result = serde_json::json!({
                "protocolVersion": protocol_version.as_str(),
                "serverInfo": state.server_info,
                "capabilities": state.handler.capabilities(),
            });
            Response::success(request.id.clone(), init_result)
        }
        _ => {
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
            if let Some(result) = route_logging(
                state.handler.as_ref(),
                &state.handler.capabilities(),
                method,
                params,
                &ctx,
            )
            .await
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

    // Enforce the session's user binding before replaying buffered events.
    if let Some(id) = &session_id {
        if let Err(e) = state.sessions.get_verified(id, user.as_ref()) {
            warn!(session_id = %id, error = %e, "Rejected SSE: session binding violation");
            return HttpResponse::Forbidden().body(e.to_string());
        }
    }

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
