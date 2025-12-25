//! HTTP handlers for MCP requests using Rocket.

use crate::state::{HasServerInfo, McpState};
use crate::{SUPPORTED_VERSIONS, is_supported_version};
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol::Message;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::{
    PromptHandler, ResourceHandler, ServerHandler, ToolHandler, route_prompts, route_resources,
    route_tools,
};
use rocket::http::{ContentType, Header, Status};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::response::stream::{Event, EventStream};
use rocket::response::{self, Responder, Response};
use std::io::Cursor;
use std::sync::Arc;
use tracing::{debug, info, warn};

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
    let msg: Message = match serde_json::from_str(body) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON-RPC message");
            return McpResponse::error(Status::BadRequest, format!("Invalid message: {e}"));
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

            let response = create_response_for_request(state, &request).await;

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
            McpResponse::accepted(session_id)
        }
        _ => {
            warn!("Unexpected message type received");
            McpResponse::error(
                Status::BadRequest,
                "Expected request or notification".to_string(),
            )
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
/// This returns an `EventStream` for pushing notifications to clients.
pub fn handle_sse<H>(state: &McpState<H>, session_id: Option<String>) -> EventStream![]
where
    H: HasServerInfo + Send + Sync + 'static,
{
    let (session_id, mut rx) = if let Some(id) = session_id {
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

    EventStream! {
        // Send connected event with session ID
        yield Event::data(session_id.clone()).event("connected").id("evt-connected");

        // Stream new messages
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    let event_id = format!("evt-{}", uuid::Uuid::new_v4());
                    yield Event::data(msg).event("message").id(event_id);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "SSE client lagged, skipped messages");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    debug!("SSE channel closed");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
