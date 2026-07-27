//! #153 PR 5 (actix port): SSE binding rules + server-initiated requests
//! end-to-end — the same matrix as the axum `sse_binding`/`peer_e2e` suites.

// Integration-test scaffolding: framework-shaped handler signatures, boxed
// future types and guards held across assertions. None of this ships.
#![allow(clippy::future_not_send)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::type_complexity)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::needless_pass_by_value)]

use actix_web::HttpMessage;
use actix_web::test::TestRequest;
use actix_web::web;
use mcpkit_actix::{McpRouter, McpState};
use mcpkit_core::auth::VerifiedUser;
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
) -> (actix_web::http::StatusCode, serde_json::Value) {
    let mut req = TestRequest::default();
    if let Some(s) = session {
        req = req.insert_header(("mcp-session-id", s));
    }
    let req = req.to_http_request();
    let resp =
        match mcpkit_actix::handle_mcp_post(req, web::Data::new(state.clone()), body.to_string())
            .await
        {
            Ok(resp) => resp,
            Err(e) => actix_web::ResponseError::error_response(&e),
        };
    let status = resp.status();
    let bytes = actix_web::body::to_bytes(resp.into_body())
        .await
        .expect("body");
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

async fn init(state: &McpState<H>) -> String {
    let req = TestRequest::default().to_http_request();
    let resp = mcpkit_actix::handle_mcp_post(
        req,
        web::Data::new(state.clone()),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": { "roots": {}, "elicitation": {} }
            }
        })
        .to_string(),
    )
    .await
    .expect("handler result");
    resp.headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .expect("session id")
}

async fn sse_status(state: &McpState<H>, session: Option<&str>, user: Option<VerifiedUser>) -> u16 {
    let mut req = TestRequest::default();
    if let Some(s) = session {
        req = req.insert_header(("mcp-session-id", s));
    }
    let req = req.to_http_request();
    if let Some(u) = user {
        req.extensions_mut().insert(u);
    }
    mcpkit_actix::handle_sse(req, web::Data::new(state.clone()))
        .await
        .status()
        .as_u16()
}

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

// ---- SSE binding rules (#172/#173 for actix) -------------------------------

#[actix_rt::test]
async fn sse_without_session_id_is_400() {
    let (state, _) = test_state();
    assert_eq!(sse_status(&state, None, None).await, 400);
}

#[actix_rt::test]
async fn sse_with_unknown_session_id_is_404_and_never_adopts_it() {
    let (state, _) = test_state();
    assert_eq!(
        sse_status(&state, Some("attacker-chosen-id"), None).await,
        404
    );
    assert!(state.sessions.get("attacker-chosen-id").is_none());
}

#[actix_rt::test]
async fn sse_with_other_users_session_is_403() {
    let (state, _) = test_state();
    let alice = VerifiedUser::new("alice").issuer("https://idp");
    let bob = VerifiedUser::new("bob").issuer("https://idp");
    let sid = state.sessions.create_for_user(Some(alice));

    assert_eq!(sse_status(&state, Some(&sid), Some(bob)).await, 403);
    assert_eq!(sse_status(&state, Some(&sid), None).await, 403);
}

#[actix_rt::test]
async fn sse_with_own_session_streams() {
    let (state, _) = test_state();
    let sid = state.sessions.create();
    assert_eq!(sse_status(&state, Some(&sid), None).await, 200);
}

#[actix_rt::test]
async fn get_on_mcp_endpoint_serves_sse() {
    use actix_web::{App, test};
    let router = McpRouter::new(H(Observed::default()));
    let state = router.state();
    let app = test::init_service(App::new().configure(router.configure_app())).await;

    let sid = init(&state).await;
    let req = test::TestRequest::get()
        .uri("/mcp")
        .insert_header(("mcp-session-id", sid.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("text/event-stream")),
        "GET on the MCP endpoint must open an SSE stream"
    );
}

// ---- server-initiated requests (the #153 feature) --------------------------

#[actix_rt::test]
async fn tool_elicitation_round_trips_over_the_stream() {
    let (state, _) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let call = {
        let state = state.clone();
        let sid = sid.clone();
        tokio::task::spawn_local(async move {
            post(
                &state,
                Some(&sid),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": { "name": "ask", "arguments": {} }
                }),
            )
            .await
            .1
        })
    };

    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "elicitation/create");
    let request_id = request["id"].clone();

    let (status, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": request_id,
            "result": { "action": "accept", "content": { "answer": "blue" } }
        }),
    )
    .await;
    assert_eq!(status, actix_web::http::StatusCode::ACCEPTED);

    let result = call.await.expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(text, "elicited: blue", "tool result: {result}");
}

#[actix_rt::test]
async fn roots_hook_round_trips_over_the_stream() {
    let (state, observed) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let _ = post(
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

#[actix_rt::test]
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

#[actix_rt::test]
async fn elicit_without_stream_fails_fast() {
    let (state, _) = test_state(); // 100ms grace
    let sid = init(&state).await;

    let started = std::time::Instant::now();
    let (_, resp) = post(
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
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[actix_rt::test]
async fn delete_fails_pending_and_forgets_the_session() {
    let (state, _) = test_state();
    let sid = init(&state).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let call = {
        let state = state.clone();
        let sid = sid.clone();
        tokio::task::spawn_local(async move {
            post(
                &state,
                Some(&sid),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": { "name": "ask", "arguments": {} }
                }),
            )
            .await
            .1
        })
    };
    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "elicitation/create");

    let req = TestRequest::default()
        .insert_header(("mcp-session-id", sid.as_str()))
        .to_http_request();
    let resp = mcpkit_actix::handle_mcp_delete(req, web::Data::new(state.clone())).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);

    let result = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("pending request must fail on DELETE, not run out its timeout")
        .expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.starts_with("elicit failed:"), "got: {result}");

    let (status, _) = post(
        &state,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" }),
    )
    .await;
    assert_eq!(status, actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_rt::test]
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
    .1;
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
        .1;
        if let Some(t) = result["result"]["content"][0]["text"].as_str() {
            text = t.to_string();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(text, "elicited: blue");
}

/// Spec: a notification carries no id and MUST NOT draw a response — including
/// one the server does not handle, and one whose params are missing.
///
/// Asserted on this dispatch path specifically. Each adapter decides this in
/// its own `handle_mcp_post`, not in the shared `route_*` helpers, so passing
/// on one adapter says nothing about the other three.
#[tokio::test]
async fn notifications_never_draw_a_response() {
    let (state, _) = test_state();
    let sid = init(&state).await;

    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "notifications/roots/list_changed",
        "notifications/progress",
        "notifications/not_a_real_method",
    ] {
        let (status, body) = post(
            &state,
            Some(&sid),
            serde_json::json!({ "jsonrpc": "2.0", "method": method }),
        )
        .await;
        assert_eq!(
            body,
            serde_json::Value::Null,
            "{method} drew a response body"
        );
        assert_eq!(status.as_u16(), 202, "{method} should be accepted with 202");
    }
}

/// Spec: a server honours exactly the capabilities it advertised. This handler
/// declares no `completion`, so `completion/complete` must be method-not-found
/// (-32601) — the literal is spelled out rather than referenced through the
/// constant that produces it.
#[tokio::test]
async fn capabilities_are_honoured_exactly_as_advertised() {
    let (state, _) = test_state();
    let (_, init_result) = post(
        &state,
        None,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-11-25", "capabilities": {} }
        }),
    )
    .await;
    assert!(
        init_result["result"]["capabilities"]
            .get("completion")
            .is_none(),
        "handler advertised completion it does not implement: {init_result}"
    );
    let sid = init(&state).await;

    let (_, body) = post(
        &state,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "completion/complete",
            "params": {
                "ref": { "type": "ref/prompt", "name": "p" },
                "argument": { "name": "a", "value": "" }
            }
        }),
    )
    .await;
    assert_eq!(
        body["error"]["code"], -32601,
        "unadvertised completion/complete must be method-not-found: {body}"
    );
}
