//! E2E for #122: task-augmented `tools/call` and `tasks/*` served through the
//! axum adapter, including per-session isolation and cancellation propagation.

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
    let (resp, sid) = post(&state, None, call_task(1, "echo")).await;
    let sid = sid.expect("session id");
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

    let (resp, _) = post(&state, None, call_task(1, "echo")).await;
    assert_eq!(
        resp["result"]["task"]["ttl"], 1234,
        "configured task ttl not materialized: {resp}"
    );
}

#[tokio::test]
async fn task_augmented_call_on_forbidden_tool_is_rejected() {
    let (state, _) = state();
    let (resp, _) = post(&state, None, call_task(1, "plain")).await;
    // A tool without taskSupport must be rejected, not run as a task.
    assert!(!resp["error"].is_null(), "expected rejection, got: {resp}");
    assert!(resp["result"]["task"].is_null());
}

#[tokio::test]
async fn tasks_cancel_trips_ctx_cancelled() {
    let (state, observed) = state();

    // Start a task whose tool parks on ctx.cancelled().
    let (resp, sid) = post(&state, None, call_task(1, "waiter")).await;
    let sid = sid.expect("session id");
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
    let (resp, sid_a) = post(&state, None, call_task(1, "echo")).await;
    let sid_a = sid_a.expect("session A id");
    let task_id = resp["result"]["task"]["taskId"]
        .as_str()
        .expect("taskId")
        .to_string();

    // Session B is a distinct session (no session header -> new session).
    let (_probe, sid_b) = post(&state, None, call_task(2, "echo")).await;
    let sid_b = sid_b.expect("session B id");
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
