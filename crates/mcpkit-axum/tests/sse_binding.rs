//! #153 PR 1 (#172, #173): SSE streams attach to the POST-created session —
//! the MCP endpoint serves GET, unknown session ids are 404 (never adopted),
//! mismatched users are 403, and a missing session id is 400.

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use mcpkit_axum::McpState;
use mcpkit_core::auth::VerifiedUser;
use mcpkit_core::capability::ServerInfo;
use mcpkit_core::error::McpError;
use mcpkit_core::types::{GetPromptResult, Prompt, Resource, ResourceContents, Tool, ToolOutput};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};

struct H;

impl ServerHandler for H {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("t", "1.0.0")
    }
}
impl ToolHandler for H {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![])
    }
    async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Map<String, serde_json::Value>,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        Err(McpError::method_not_found(name))
    }
}
impl ResourceHandler for H {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(vec![])
    }
    async fn read_resource(
        &self,
        _uri: &str,
        _ctx: &Context<'_>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        Ok(vec![])
    }
}
impl PromptHandler for H {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(vec![])
    }
    async fn get_prompt(
        &self,
        name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::method_not_found(name))
    }
}

fn sse_headers(session: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(s) = session {
        headers.insert("mcp-session-id", HeaderValue::from_str(s).expect("sid"));
    }
    headers
}

async fn sse_status(
    state: &McpState<H>,
    session: Option<&str>,
    user: Option<VerifiedUser>,
) -> StatusCode {
    mcpkit_axum::handle_sse(
        State(state.clone()),
        sse_headers(session),
        user.map(Extension),
    )
    .await
    .into_response()
    .status()
}

#[tokio::test]
async fn sse_without_session_id_is_400() {
    let state = McpState::new(H);
    assert_eq!(
        sse_status(&state, None, None).await,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn sse_with_unknown_session_id_is_404_and_never_adopts_it() {
    let state = McpState::new(H);
    assert_eq!(
        sse_status(&state, Some("attacker-chosen-id"), None).await,
        StatusCode::NOT_FOUND
    );
    // The presented id must not have been registered as a session.
    assert!(state.sessions.get("attacker-chosen-id").is_none());
}

#[tokio::test]
async fn sse_with_other_users_session_is_403() {
    let state = McpState::new(H);
    let alice = VerifiedUser::new("alice").issuer("https://idp");
    let bob = VerifiedUser::new("bob").issuer("https://idp");
    let sid = state.sessions.create_for_user(Some(alice));

    assert_eq!(
        sse_status(&state, Some(&sid), Some(bob)).await,
        StatusCode::FORBIDDEN
    );
    // Anonymous access to a bound session is also rejected.
    assert_eq!(
        sse_status(&state, Some(&sid), None).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn sse_with_own_session_streams() {
    let state = McpState::new(H);
    let sid = state.sessions.create();
    assert_eq!(sse_status(&state, Some(&sid), None).await, StatusCode::OK);
}

#[tokio::test]
async fn get_on_mcp_endpoint_serves_sse() {
    use tower::util::ServiceExt;
    // Spec (#172): the MCP endpoint must serve GET; a compliant client GETs
    // the same path it POSTs to.
    let router = mcpkit_axum::McpRouter::new(H).into_router();

    // initialize via POST to obtain a session id
    let resp = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("content-type", "application/json")
                .header("mcp-protocol-version", "2025-11-25")
                .body(axum::body::Body::from(
                    r#"{"jsonrpc":"2.0","method":"initialize","params":{},"id":0}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    let sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .expect("session id");

    let resp = router
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/mcp")
                .header("mcp-session-id", sid)
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("text/event-stream")),
        "GET on the MCP endpoint must open an SSE stream"
    );
}
