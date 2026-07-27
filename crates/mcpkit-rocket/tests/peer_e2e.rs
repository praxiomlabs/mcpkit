//! #153 PR 5 (rocket port): SSE binding rules + server-initiated requests
//! end-to-end — the same matrix as the axum/actix/warp `peer_e2e` suites.

// Integration-test scaffolding: framework-shaped handler signatures, boxed
// future types and guards held across assertions. None of this ships.
#![allow(clippy::future_not_send)]
#![allow(clippy::new_ret_no_self)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::type_complexity)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::needless_pass_by_value)]

use mcpkit_core::auth::VerifiedUser;
use mcpkit_core::capability::ServerInfo;
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    ElicitRequest, ElicitationSchema, GetPromptResult, Prompt, Resource, ResourceContents, Root,
    TaskSupport, Tool, ToolOutput,
};
use mcpkit_rocket::{McpRouter, McpState};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use rocket::http::{ContentType, Header};
use rocket::local::asynchronous::Client;
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

mcpkit_rocket::create_mcp_routes!(H);

async fn setup() -> (Arc<Client>, McpState<H>, Observed) {
    let observed = Observed::default();
    let state = McpRouter::new(H(observed.clone()))
        .with_reconnect_grace(Duration::from_millis(100))
        .into_state();
    let rocket = rocket::build()
        .manage(state.clone())
        .mount("/", rocket::routes![mcp_post, mcp_get, mcp_delete, mcp_sse]);
    let client = Client::tracked(rocket).await.expect("client");
    (Arc::new(client), state, observed)
}

async fn post(
    client: &Client,
    session: Option<&str>,
    body: serde_json::Value,
) -> (u16, serde_json::Value) {
    let mut req = client
        .post("/mcp")
        .header(ContentType::JSON)
        .body(body.to_string());
    if let Some(s) = session {
        req = req.header(Header::new("mcp-session-id", s.to_string()));
    }
    let resp = req.dispatch().await;
    let status = resp.status().code;
    let text = resp.into_string().await.unwrap_or_default();
    let json = if text.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

async fn init(client: &Client) -> String {
    let resp = client
        .post("/mcp")
        .header(ContentType::JSON)
        .body(
            serde_json::json!({
                "jsonrpc": "2.0", "id": 0, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "roots": {}, "elicitation": {} }
                }
            })
            .to_string(),
        )
        .dispatch()
        .await;
    resp.headers()
        .get_one("mcp-session-id")
        .map(String::from)
        .expect("session id")
}

fn sse_status(state: &McpState<H>, session: Option<&str>, user: Option<VerifiedUser>) -> u16 {
    match mcpkit_rocket::handle_sse(state, session.map(String::from), None, user, None) {
        Ok(_) => 200,
        Err(status) => status.code,
    }
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

// ---- SSE binding rules (#172/#173 for rocket) ------------------------------

#[tokio::test]
async fn sse_without_session_id_is_400() {
    let (_, state, _) = setup().await;
    assert_eq!(sse_status(&state, None, None), 400);
}

#[tokio::test]
async fn sse_with_unknown_session_id_is_404_and_never_adopts_it() {
    let (_, state, _) = setup().await;
    assert_eq!(sse_status(&state, Some("attacker-chosen-id"), None), 404);
    assert!(!state.sessions.exists("attacker-chosen-id"));
}

#[tokio::test]
async fn sse_with_other_users_session_is_403() {
    let (_, state, _) = setup().await;
    let alice = VerifiedUser::new("alice").issuer("https://idp");
    let bob = VerifiedUser::new("bob").issuer("https://idp");
    let sid = state.sessions.create_for_user(Some(alice));

    assert_eq!(sse_status(&state, Some(&sid), Some(bob)), 403);
    assert_eq!(sse_status(&state, Some(&sid), None), 403);
}

#[tokio::test]
async fn sse_with_own_session_streams() {
    let (_, state, _) = setup().await;
    let sid = state.sessions.create();
    assert_eq!(sse_status(&state, Some(&sid), None), 200);
}

#[tokio::test]
async fn get_on_mcp_endpoint_serves_sse() {
    let (client, _, _) = setup().await;
    let sid = init(&client).await;
    // Check only status and headers — an SSE body never ends.
    let resp = client
        .get("/mcp")
        .header(Header::new("mcp-session-id", sid))
        .dispatch()
        .await;
    assert_eq!(resp.status().code, 200);
    assert!(
        resp.content_type()
            .is_some_and(|ct| ct.to_string().starts_with("text/event-stream")),
        "GET on the MCP endpoint must open an SSE stream"
    );
}

// ---- server-initiated requests (the #153 feature) --------------------------

#[tokio::test]
async fn tool_elicitation_round_trips_over_the_stream() {
    let (client, state, _) = setup().await;
    let sid = init(&client).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let call = {
        let client = Arc::clone(&client);
        let sid = sid.clone();
        tokio::spawn(async move {
            post(
                &client,
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
        &client,
        Some(&sid),
        serde_json::json!({
            "jsonrpc": "2.0", "id": request_id,
            "result": { "action": "accept", "content": { "answer": "blue" } }
        }),
    )
    .await;
    assert_eq!(status, 202);

    let result = call.await.expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(text, "elicited: blue", "tool result: {result}");
}

#[tokio::test]
async fn roots_hook_round_trips_over_the_stream() {
    let (client, state, observed) = setup().await;
    let sid = init(&client).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let _ = post(
        &client,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/roots/list_changed" }),
    )
    .await;

    let request = next_stream_request(&mut stream).await;
    assert_eq!(request["method"], "roots/list");
    let request_id = request["id"].clone();

    let _ = post(
        &client,
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

#[tokio::test]
async fn on_initialized_hook_fires() {
    let (client, _, observed) = setup().await;
    let sid = init(&client).await;
    let _ = post(
        &client,
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

#[tokio::test]
async fn elicit_without_stream_fails_fast() {
    let (client, _, _) = setup().await; // 100ms grace
    let sid = init(&client).await;

    let started = std::time::Instant::now();
    let (_, resp) = post(
        &client,
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

#[tokio::test]
async fn delete_fails_pending_and_forgets_the_session() {
    let (client, state, _) = setup().await;
    let sid = init(&client).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    let call = {
        let client = Arc::clone(&client);
        let sid = sid.clone();
        tokio::spawn(async move {
            post(
                &client,
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

    let resp = client
        .delete("/mcp")
        .header(Header::new("mcp-session-id", sid.clone()))
        .dispatch()
        .await;
    assert_eq!(resp.status().code, 204);

    let result = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("pending request must fail on DELETE, not run out its timeout")
        .expect("join");
    let text = result["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default();
    assert!(text.starts_with("elicit failed:"), "got: {result}");

    let (status, _) = post(
        &client,
        Some(&sid),
        serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": "ping" }),
    )
    .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn task_augmented_tool_elicits_over_the_stream() {
    let (client, state, _) = setup().await;
    let sid = init(&client).await;
    let registry = state.sessions.streams(&sid).expect("session");
    let (mut stream, _prime) = registry.open("connected", sid.clone());

    // A task-augmented call replies immediately with CreateTaskResult...
    let created = post(
        &client,
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
        &client,
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
            &client,
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
    let (client, _state, _) = setup().await;
    let sid = init(&client).await;

    for method in [
        "notifications/initialized",
        "notifications/cancelled",
        "notifications/roots/list_changed",
        "notifications/progress",
        "notifications/not_a_real_method",
    ] {
        let (status, body) = post(
            &client,
            Some(&sid),
            serde_json::json!({ "jsonrpc": "2.0", "method": method }),
        )
        .await;
        assert_eq!(
            body,
            serde_json::Value::Null,
            "{method} drew a response body"
        );
        assert_eq!(status, 202, "{method} should be accepted with 202");
    }
}

/// Spec: a server honours exactly the capabilities it advertised. This handler
/// declares no `completion`, so `completion/complete` must be method-not-found
/// (-32601) — the literal is spelled out rather than referenced through the
/// constant that produces it.
#[tokio::test]
async fn capabilities_are_honoured_exactly_as_advertised() {
    let (client, _state, _) = setup().await;
    let (_, init_result) = post(
        &client,
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
    let sid = init(&client).await;

    let (_, body) = post(
        &client,
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
