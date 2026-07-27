//! E2E for #122: task-augmented `tools/call` and `tasks/*` served through the
//! axum adapter, including per-session isolation and cancellation propagation.

// Integration-test scaffolding: framework-shaped handler signatures, boxed
// future types and guards held across assertions. None of this ships.
#![allow(clippy::future_not_send)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::type_complexity)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::needless_pass_by_value)]

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use mcpkit_axum::McpState;
use mcpkit_core::capability::ServerInfo;
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    GetPromptResult, Prompt, Resource, ResourceContents, TaskSupport, Tool, ToolOutput,
};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

struct H {
    observed_cancel: Arc<AtomicBool>,
}

impl ServerHandler for H {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("tasks-test", "0.0.0")
    }
}
impl ToolHandler for H {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool::new("echo").task_support(TaskSupport::Optional),
            Tool::new("waiter").task_support(TaskSupport::Optional),
            // No execution.taskSupport -> Forbidden by default.
            Tool::new("plain"),
        ])
    }
    async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Map<String, serde_json::Value>,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "echo" => Ok(ToolOutput::text("echoed")),
            "waiter" => {
                // Park until cancelled, then record that cancellation propagated.
                ctx.cancelled().await;
                self.observed_cancel.store(true, Ordering::SeqCst);
                Ok(ToolOutput::text("cancelled"))
            }
            other => Err(McpError::method_not_found(other)),
        }
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
        _name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::method_not_found("get_prompt"))
    }
}

fn state() -> (McpState<H>, Arc<AtomicBool>) {
    let observed = Arc::new(AtomicBool::new(false));
    let state = McpState::new(H {
        observed_cancel: observed.clone(),
    });
    (state, observed)
}

/// POST one JSON-RPC message; returns (parsed response JSON, session id header).
async fn post(
    state: &McpState<H>,
    session: Option<&str>,
    body: serde_json::Value,
) -> (serde_json::Value, Option<String>) {
    let mut headers = HeaderMap::new();
    if let Some(s) = session {
        headers.insert(
            "mcp-session-id",
            HeaderValue::from_str(s).expect("session header"),
        );
    }
    let response =
        mcpkit_axum::handle_mcp_post(State(state.clone()), headers, None, body.to_string())
            .await
            .into_response();
    let sid = response
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (json, sid)
}

/// POST `initialize` and return the session id the server assigned
/// (required since #153 PR 0b: only `initialize` may omit `mcp-session-id`).
async fn init_session(state: &McpState<H>) -> String {
    let (_, sid) = post(
        state,
        None,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-11-25", "capabilities": {} }
        }),
    )
    .await;
    sid.expect("initialize must assign a session id")
}

fn call_task(id: u64, name: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": name, "arguments": {}, "task": {} }
    })
}

fn task_method(id: u64, method: &str, task_id: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": id, "method": method,
        "params": { "taskId": task_id }
    })
}

#[tokio::test]
async fn task_augmented_call_creates_gets_and_results() {
    let (state, _) = state();

    // Augmented tools/call returns CreateTaskResult (status "working") immediately.
    let sid = init_session(&state).await;
    let (resp, _) = post(&state, Some(&sid), call_task(1, "echo")).await;
    assert!(resp["error"].is_null(), "augmented call errored: {resp}");
    assert_eq!(resp["result"]["task"]["status"], "working");
    let task_id = resp["result"]["task"]["taskId"]
        .as_str()
        .expect("taskId")
        .to_string();

    // The tool runs in the background (tokio::spawn); tasks/result yields once done.
    let mut payload = serde_json::Value::Null;
    for _ in 0..100 {
        let (r, _) = post(&state, Some(&sid), task_method(2, "tasks/result", &task_id)).await;
        if r["error"].is_null() && !r["result"].is_null() {
            payload = r["result"].clone();
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        payload["content"][0]["text"], "echoed",
        "payload: {payload}"
    );

    // tasks/get reports the terminal status.
    let (g, _) = post(&state, Some(&sid), task_method(3, "tasks/get", &task_id)).await;
    assert_eq!(g["result"]["status"], "completed", "get: {g}");
}

#[tokio::test]
async fn with_task_ttl_configures_session_store_retention() {
    // McpRouter/McpState::with_task_ttl sets each session store's default, and an
    // omitted `ttl` is materialized to it on the CreateTaskResult.
    let observed = Arc::new(AtomicBool::new(false));
    let state = McpState::new(H {
        observed_cancel: observed,
    })
    .with_task_ttl(Some(1234));

    let sid = init_session(&state).await;
    let (resp, _) = post(&state, Some(&sid), call_task(1, "echo")).await;
    assert_eq!(
        resp["result"]["task"]["ttl"], 1234,
        "configured task ttl not materialized: {resp}"
    );
}

#[tokio::test]
async fn task_augmented_call_on_forbidden_tool_is_rejected() {
    let (state, _) = state();
    let sid = init_session(&state).await;
    let (resp, _) = post(&state, Some(&sid), call_task(1, "plain")).await;
    // A tool without taskSupport must be rejected, not run as a task.
    // Spec: -32601 (Method not found), not -32602.
    assert_eq!(
        resp["error"]["code"], -32601,
        "expected -32601, got: {resp}"
    );
    assert!(resp["result"]["task"].is_null());
}

#[tokio::test]
async fn tasks_cancel_trips_ctx_cancelled() {
    let (state, observed) = state();

    // Start a task whose tool parks on ctx.cancelled().
    let sid = init_session(&state).await;
    let (resp, _) = post(&state, Some(&sid), call_task(1, "waiter")).await;
    let task_id = resp["result"]["task"]["taskId"]
        .as_str()
        .expect("taskId")
        .to_string();
    assert!(
        !observed.load(Ordering::SeqCst),
        "tool cancelled before request"
    );

    // tasks/cancel must trip the token wired into the background context.
    let (c, _) = post(&state, Some(&sid), task_method(2, "tasks/cancel", &task_id)).await;
    assert!(c["error"].is_null(), "cancel errored: {c}");

    // The tool observes cancellation and records it.
    let mut seen = false;
    for _ in 0..100 {
        if observed.load(Ordering::SeqCst) {
            seen = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        seen,
        "tasks/cancel did not trip ctx.cancelled() in the tool"
    );
}

#[tokio::test]
async fn tasks_are_isolated_per_session() {
    let (state, _) = state();

    // Session A creates a task.
    let sid_a = init_session(&state).await;
    let (resp, _) = post(&state, Some(&sid_a), call_task(1, "echo")).await;
    let task_id = resp["result"]["task"]["taskId"]
        .as_str()
        .expect("taskId")
        .to_string();

    // Session B is a distinct session (no session header -> new session).
    let sid_b = init_session(&state).await;
    let (_probe, _) = post(&state, Some(&sid_b), call_task(2, "echo")).await;
    assert_ne!(sid_a, sid_b, "expected two distinct sessions");

    // Session B must not be able to read session A's task.
    let (g, _) = post(&state, Some(&sid_b), task_method(3, "tasks/get", &task_id)).await;
    assert!(
        !g["error"].is_null(),
        "session B could read session A's task: {g}"
    );
    // And session A still can.
    let (ga, _) = post(&state, Some(&sid_a), task_method(4, "tasks/get", &task_id)).await;
    assert!(ga["error"].is_null(), "session A lost its own task: {ga}");
}

/// Spec (Streamable HTTP): a `POSTed` JSON-RPC *response* is accepted with
/// 202, not rejected — clients deliver responses to server-initiated
/// requests this way (#153 PR 0a; correlation lands with the session peer).
#[tokio::test]
async fn response_post_is_accepted_with_202() {
    use axum::response::IntoResponse;
    let (state, _) = state();
    let sid = init_session(&state).await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "mcp-session-id",
        HeaderValue::from_str(&sid).expect("session header"),
    );
    let response = mcpkit_axum::handle_mcp_post(
        State(state),
        headers,
        None,
        r#"{"jsonrpc":"2.0","id":42,"result":{"roots":[]}}"#.to_string(),
    )
    .await
    .into_response();
    assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);
}

/// #153 PR 0b: a non-initialize message without `mcp-session-id` is 400.
#[tokio::test]
async fn missing_session_id_on_non_initialize_is_400() {
    use axum::response::IntoResponse;
    let (state, _) = state();
    let response = mcpkit_axum::handle_mcp_post(
        State(state),
        HeaderMap::new(),
        None,
        call_task(1, "echo").to_string(),
    )
    .await
    .into_response();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

/// An id the session's task store does not own is *invalid params* (-32602),
/// not *method not found* (-32601).
///
/// The adapters have no custom task handler, so a store that declines an id has
/// declined it for good. Answering -32601 would tell the peer this server has
/// no `tasks/get` at all, moments after it served `tasks/*` for a real id.
#[tokio::test]
async fn unknown_task_id_is_invalid_params_not_method_not_found() {
    let (state, _) = state();
    let sid = init_session(&state).await;

    for method in ["tasks/get", "tasks/result", "tasks/cancel"] {
        let (resp, _) = post(
            &state,
            Some(&sid),
            task_method(1, method, "no-such-task-id"),
        )
        .await;
        assert_eq!(
            resp["error"]["code"], -32602,
            "{method} with an unknown id must be -32602: {resp}"
        );
    }
}

/// The guard above must not swallow a genuinely unknown method.
#[tokio::test]
async fn unknown_method_is_still_method_not_found() {
    let (state, _) = state();
    let sid = init_session(&state).await;

    let (resp, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tasks/nonexistent",
            "params": { "taskId": "x" }
        }),
    )
    .await;
    assert_eq!(
        resp["error"]["code"], -32601,
        "a non-spec tasks/* method must stay -32601: {resp}"
    );
}

/// A task transition must reach the client over the session's SSE stream.
///
/// The adapters have no run loop, so they cannot use the runtime's ambient
/// pump; their task store publishes onto the session's stream registry
/// instead. Without that wiring `notifications/tasks/status` was implemented
/// but never emitted on any HTTP transport.
#[tokio::test]
async fn task_transition_publishes_status_notification_on_the_session_stream() {
    let (state, _) = state();
    let sid = init_session(&state).await;

    // Open the stream *before* the transition: delivery is store-and-forward
    // for a live stream only.
    let session = state.sessions.get(&sid).expect("session exists");
    let (mut stream, _prime) = session.streams.open("message", "{}".to_string());

    let (resp, _) = post(&state, Some(&sid), call_task(1, "echo")).await;
    assert!(resp["error"].is_null(), "augmented call errored: {resp}");

    let mut other_events = Vec::new();
    let notification = loop {
        let event = tokio::time::timeout(Duration::from_secs(5), stream.recv())
            .await
            .expect("timed out waiting for a task status notification")
            .expect("stream closed before the notification arrived");
        match serde_json::from_str::<serde_json::Value>(&event.data) {
            Ok(json) if json["method"] == "notifications/tasks/status" => break json,
            Ok(json) => other_events.push(json),
            Err(_) => {}
        }
        assert!(
            other_events.len() < 16,
            "no task status notification among {other_events:?}"
        );
    };

    assert_eq!(notification["params"]["status"], "completed");
    assert!(
        notification["params"]["taskId"].is_string(),
        "status notification must carry the taskId: {notification}"
    );
    // Per spec the status notification must not be tagged with
    // `io.modelcontextprotocol/related-task`; the taskId is already in params.
    assert!(
        notification["params"]["_meta"].is_null(),
        "status notification must not carry _meta: {notification}"
    );
}
