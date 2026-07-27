//! #153 PR 4 end-to-end: server-initiated requests over the axum adapter.
//!
//! The flow these tests pin IS the feature: a handler calls
//! `ctx.elicit()`/`ctx.list_roots()`, the JSON-RPC request rides the
//! session's SSE stream, the client answers with a response POST, and the
//! waiting handler resumes.

use axum::Extension;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use mcpkit_axum::{McpRouter, McpState};
use mcpkit_core::capability::ServerInfo;
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    ElicitRequest, ElicitationSchema, GetPromptResult, Prompt, Resource, ResourceContents, Root,
    TaskSupport, Tool, ToolOutput,
};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct Observed {
    initialized: Arc<AtomicBool>,
    roots: Arc<Mutex<Option<Result<Vec<Root>, String>>>>,
}

#[derive(Clone)]
struct H(Observed);

impl ServerHandler for H {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("t", "1.0.0")
    }
    async fn on_initialized(&self, _ctx: &Context<'_>) {
        self.0.initialized.store(true, Ordering::SeqCst);
    }
    async fn on_roots_list_changed(&self, ctx: &Context<'_>) {
        let result = ctx.list_roots().await.map_err(|e| e.to_string());
        *self.0.roots.lock().unwrap() = Some(result);
    }
}
impl ToolHandler for H {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![Tool::new("ask").task_support(TaskSupport::Optional)])
    }
    async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Map<String, serde_json::Value>,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "ask" => {
                let schema = serde_json::from_value::<ElicitationSchema>(
                    serde_json::json!({ "type": "object", "properties": {} }),
                )
                .expect("schema");
                match ctx.elicit(ElicitRequest::new("pick a color", schema)).await {
                    Ok(result) => Ok(ToolOutput::text(format!(
                        "elicited: {}",
                        serde_json::to_value(&result)
                            .unwrap_or_default()
                            .get("content")
                            .and_then(|c| c.get("answer"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("<none>")
                    ))),
                    Err(e) => Ok(ToolOutput::text(format!("elicit failed: {e}"))),
                }
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
        name: &str,
        _args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::method_not_found(name))
    }
}

fn test_state() -> (McpState<H>, Observed) {
    let observed = Observed::default();
    let state = McpRouter::new(H(observed.clone()))
        .with_reconnect_grace(Duration::from_millis(100))
        .state();
    (state, observed)
}

async fn post(
    state: &McpState<H>,
    session: Option<&str>,
    body: serde_json::Value,
) -> (serde_json::Value, Option<String>) {
    let mut headers = HeaderMap::new();
    if let Some(s) = session {
        headers.insert("mcp-session-id", HeaderValue::from_str(s).expect("sid"));
    }
    let response = mcpkit_axum::handle_mcp_post(
        State(state.clone()),
        headers,
        None::<Extension<mcpkit_core::auth::VerifiedUser>>,
        body.to_string(),
    )
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

async fn init(state: &McpState<H>) -> String {
    let (_, sid) = post(
        state,
        None,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": { "roots": {}, "elicitation": {} }
            }
        }),
    )
    .await;
    sid.expect("session id")
}

/// Pull the next JSON-RPC message off the session's SSE stream.
async fn next_stream_request(
    handle: &mut mcpkit_server::streams::StreamHandle,
) -> serde_json::Value {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), handle.recv())
            .await
            .expect("stream event within 5s")
            .expect("stream open");
        if event.event_type == "message" {
            return serde_json::from_str(&event.data).expect("json-rpc message");
        }
    }
}

/// The §4.7 headline: a plain tool elicits over HTTP and completes when the
/// client POSTs the response.
#[tokio::test]
async fn tool_elicitation_round_trips_over_the_stream() {
    let (state, _) = test_state();
    let sid = init(&state).await;

    // "Connect" the client's SSE stream.
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    // Call the tool concurrently; it blocks in ctx.elicit().
    let call = {
        let state = state.clone();
        let sid = sid.clone();
        tokio::spawn(async move {
            post(
                &state,
                Some(&sid),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": { "name": "ask", "arguments": {} }
                }),
            )
            .await
            .0
        })
    };

    // The elicitation request appears on the stream.
    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "elicitation/create");
    let request_id = request["id"].clone();

    // The client answers via a response POST -> 202.
    let (resp, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": request_id,
            "result": { "action": "accept", "content": { "answer": "blue" } }
        }),
    )
    .await;
    assert_eq!(resp, serde_json::Value::Null, "202 has no body");

    // The tool resumed with the elicited value.
    let result = call.await.expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(text, "elicited: blue", "tool result: {result}");
}

/// The #141/#153 notification consumer: roots/list_changed -> hook ->
/// ctx.list_roots() -> stream -> response POST -> hook observes the roots.
#[tokio::test]
async fn roots_hook_round_trips_over_the_stream() {
    let (state, observed) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let (_, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/roots/list_changed" }),
    )
    .await;

    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "roots/list");
    let request_id = request["id"].clone();

    let _ = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": request_id,
            "result": { "roots": [{ "uri": "file:///w", "name": "w" }] }
        }),
    )
    .await;

    // The hook observed the client's roots.
    for _ in 0..100 {
        if observed.roots.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let roots = observed
        .roots
        .lock()
        .unwrap()
        .clone()
        .expect("hook ran")
        .expect("list_roots succeeded");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].uri, "file:///w");
}

#[tokio::test]
async fn on_initialized_hook_fires() {
    let (state, observed) = test_state();
    let sid = init(&state).await;
    let _ = post(
        &state,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    for _ in 0..100 {
        if observed.initialized.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("on_initialized hook never fired");
}

/// Without any SSE stream, a peer request fails fast (send-time grace) and
/// the tool degrades instead of hanging for the request timeout.
#[tokio::test]
async fn elicit_without_stream_fails_fast() {
    let (state, _) = test_state(); // 100ms grace
    let sid = init(&state).await;

    let started = std::time::Instant::now();
    let (resp, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "ask", "arguments": {} }
        }),
    )
    .await;
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.starts_with("elicit failed:"), "got: {resp}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "must fail at the grace, not the elicitation timeout"
    );
}

/// DELETE terminates the session: pending server-initiated requests fail
/// immediately and the id is gone (404 afterwards).
#[tokio::test]
async fn delete_fails_pending_and_forgets_the_session() {
    let (state, _) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let call = {
        let state = state.clone();
        let sid = sid.clone();
        tokio::spawn(async move {
            post(
                &state,
                Some(&sid),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": { "name": "ask", "arguments": {} }
                }),
            )
            .await
            .0
        })
    };
    // The elicitation is in flight (visible on the stream), unanswered.
    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "elicitation/create");

    // Client terminates the session.
    let mut headers = HeaderMap::new();
    headers.insert("mcp-session-id", HeaderValue::from_str(&sid).expect("sid"));
    let resp = mcpkit_axum::handle_mcp_delete(
        State(state.clone()),
        headers,
        None::<Extension<mcpkit_core::auth::VerifiedUser>>,
    )
    .await
    .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::NO_CONTENT);

    // The pending elicitation failed immediately; the tool degraded.
    let result = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("pending request must fail on DELETE, not run out its timeout")
        .expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.starts_with("elicit failed:"), "got: {result}");

    // The session id is gone.
    let (resp, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" }),
    )
    .await;
    assert!(
        resp.get("result").is_none(),
        "expected session-not-found, got: {resp}"
    );
}

#[tokio::test]
async fn task_augmented_tool_elicits_over_the_stream() {
    let (state, _) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    // A task-augmented call replies immediately with CreateTaskResult...
    let created = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "ask", "arguments": {}, "task": {} }
        }),
    )
    .await
    .0;
    let task_id = created["result"]["task"]["taskId"]
        .as_str()
        .expect("taskId")
        .to_string();

    // ...while the background tool elicits over the session stream (#153
    // PR 6: the peer survives into the spawned task).
    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "elicitation/create");
    // Spec MUST (2025-11-25 tasks, "Associating Task-Related Messages"): an
    // elicitation a task-augmented tool call depends on carries that call's
    // task id. Key spelled out on purpose — asserting against the constant
    // that produces it would pass even if the constant were wrong.
    assert_eq!(
        request["params"]["_meta"]["io.modelcontextprotocol/related-task"]["taskId"],
        serde_json::json!(task_id),
        "elicitation/create raised by a task-augmented tool must carry related-task _meta"
    );
    let request_id = request["id"].clone();
    let _ = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": request_id,
            "result": { "action": "accept", "content": { "answer": "blue" } }
        }),
    )
    .await;

    // tasks/result yields the tool result once the elicited answer lands.
    let mut text = String::new();
    for _ in 0..100 {
        let result = post(
            &state,
            Some(&sid),
            serde_json::json!({
                "jsonrpc": "2.0", "id": 9, "method": "tasks/result",
                "params": { "taskId": task_id }
            }),
        )
        .await
        .0;
        if let Some(t) = result["result"]["content"][0]["text"].as_str() {
            text = t.to_string();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(text, "elicited: blue");
}
