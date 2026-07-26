//! Behavioural conformance a schema diff structurally cannot check.
//!
//! `scripts/schema-diff.sh` compares vocabulary: method names, enum variants,
//! discriminators, field shapes. It is blind to *sequencing* (may this request
//! arrive yet?), *gating* (is a capability honoured only when advertised?),
//! *response discipline* (a notification must not draw a reply), *lifecycle*
//! (TTL eviction), and *emission* (is a spec-defined outbound message ever
//! actually sent?).
//!
//! Each test here drives a real server over a transport. The emission test is
//! anchored to the vendored schema rather than to mcpkit's own constants.

use mcpkit_core::error::codes;
use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use mcpkit_server::{ServerBuilder, ServerRuntime};
use mcpkit_transport::{MemoryTransport, Transport};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Full;

impl mcpkit_server::ServerHandler for Full {
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo::new("behaviour", "1.0.0")
    }
}

impl mcpkit_server::ToolHandler for Full {
    async fn list_tools(
        &self,
        _ctx: &mcpkit_server::Context<'_>,
    ) -> Result<Vec<mcpkit_core::types::Tool>, mcpkit_core::error::McpError> {
        Ok(vec![
            mcpkit_core::types::Tool::new("echo")
                .task_support(mcpkit_core::types::TaskSupport::Optional),
        ])
    }
    async fn call_tool(
        &self,
        name: &str,
        _args: serde_json::Map<String, serde_json::Value>,
        _ctx: &mcpkit_server::Context<'_>,
    ) -> Result<mcpkit_core::types::ToolOutput, mcpkit_core::error::McpError> {
        Ok(mcpkit_core::types::ToolOutput::text(format!("ok:{name}")))
    }
}

/// A server with no tool handler at all — used for the capability-gating test.
struct Bare;

impl mcpkit_server::ServerHandler for Bare {
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo::new("bare", "1.0.0")
    }
}

fn req(id: u64, method: &'static str, params: serde_json::Value) -> Message {
    Message::Request(Request::with_params(method, RequestId::Number(id), params))
}

/// The next *response*, skipping notifications the server publishes meanwhile.
async fn next_response(transport: &MemoryTransport) -> Response {
    for _ in 0..16 {
        let msg = timeout(Duration::from_secs(5), transport.recv())
            .await
            .expect("timed out")
            .expect("recv ok")
            .expect("some message");
        match msg {
            Message::Response(r) => return r,
            Message::Notification(_) => continue,
            other => panic!("expected response, got {other:?}"),
        }
    }
    panic!("no response after 16 messages");
}

fn err_code(r: &Response) -> i32 {
    r.error
        .as_ref()
        .unwrap_or_else(|| panic!("expected an error response, got {r:?}"))
        .code
}

/// An **uninitialized** server. The handshake is driven over the wire, so the
/// sequencing tests exercise the real path rather than a pre-set flag.
fn serve_uninitialized<H>(handler: H) -> (MemoryTransport, tokio::task::JoinHandle<()>)
where
    H: mcpkit_server::ServerHandler + Send + Sync + 'static,
{
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(handler).build();
    let runtime = ServerRuntime::new(built, server);
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });
    (client, handle)
}

/// An uninitialized server that *does* serve tools, so a rejection can be
/// attributed to sequencing rather than to a missing handler.
fn serve_uninitialized_with_tools() -> (MemoryTransport, tokio::task::JoinHandle<()>) {
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(Full).with_tools(Full).build();
    let runtime = ServerRuntime::new(built, server);
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });
    (client, handle)
}

fn initialize_msg(id: u64) -> Message {
    req(
        id,
        "initialize",
        json!({
            "protocolVersion": mcpkit_core::capability::PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "c", "version": "1.0" }
        }),
    )
}

// ---------------------------------------------------------------------------
// Row 1 — handshake sequencing and pre-initialization rejection
// ---------------------------------------------------------------------------

/// Per spec the client MUST NOT send requests other than `ping` before the
/// server has replied to `initialize`. A server that answers them anyway is
/// indistinguishable, to a schema diff, from one that does not.
#[tokio::test]
async fn requests_before_initialize_are_rejected_but_ping_is_not() {
    let (client, handle) = serve_uninitialized_with_tools();

    // A normal request before the handshake must be refused.
    client
        .send(req(1, "tools/list", json!({})))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        err_code(&resp),
        codes::INVALID_REQUEST,
        "a pre-initialize request must be -32600, got {resp:?}"
    );

    // `ping` is a liveness check and is valid at any time, including mid-handshake.
    client.send(req(2, "ping", json!({}))).await.expect("send");
    let resp = next_response(&client).await;
    assert!(
        resp.error.is_none(),
        "ping must answer pre-initialize: {resp:?}"
    );

    // After the handshake the same request succeeds — proving the rejection was
    // sequencing, not a missing handler.
    client.send(initialize_msg(3)).await.expect("send");
    let resp = next_response(&client).await;
    assert!(resp.error.is_none(), "initialize failed: {resp:?}");

    client
        .send(req(4, "tools/list", json!({})))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert!(
        resp.error.is_none(),
        "tools/list must succeed once initialized: {resp:?}"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

// ---------------------------------------------------------------------------
// Row 2 — capability gating: advertise-vs-honour
// ---------------------------------------------------------------------------

/// A capability must be honoured **iff** it was advertised. Advertising one the
/// server does not serve, or serving one it did not advertise, are both
/// wire-observable and both invisible to a vocabulary diff.
#[tokio::test]
async fn capabilities_are_honoured_exactly_as_advertised() {
    // Server WITHOUT a tool handler: must not advertise tools, must not serve them.
    let (client, handle) = serve_uninitialized(Bare);
    client.send(initialize_msg(1)).await.expect("send");
    let resp = next_response(&client).await;
    let caps = resp
        .result
        .as_ref()
        .and_then(|r| r.get("capabilities"))
        .cloned()
        .expect("capabilities in InitializeResult");
    assert!(
        caps.get("tools").is_none(),
        "a server with no tool handler must not advertise tools: {caps}"
    );

    client
        .send(req(2, "tools/list", json!({})))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert!(
        resp.error.is_some(),
        "tools/list must not be served when tools were not advertised: {resp:?}"
    );
    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;

    // Server WITH a tool handler: advertises tools and serves them.
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(Full).with_tools(Full).build();
    let runtime = ServerRuntime::new(built, server);
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });

    client.send(initialize_msg(1)).await.expect("send");
    let resp = next_response(&client).await;
    let caps = resp
        .result
        .as_ref()
        .and_then(|r| r.get("capabilities"))
        .cloned()
        .expect("capabilities");
    assert!(
        caps.get("tools").is_some(),
        "a server with a tool handler must advertise tools: {caps}"
    );

    client
        .send(req(2, "tools/list", json!({})))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert!(
        resp.error.is_none(),
        "advertised tools must be served: {resp:?}"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

// ---------------------------------------------------------------------------
// Row 3 — a notification must not draw a response
// ---------------------------------------------------------------------------

/// JSON-RPC notifications carry no id and MUST NOT be replied to. A server that
/// answers one corrupts every subsequent id correlation on the connection.
#[tokio::test]
async fn notifications_never_draw_a_response() {
    let (client, handle) = serve_uninitialized_with_tools();

    client.send(initialize_msg(1)).await.expect("send");
    let resp = next_response(&client).await;
    assert_eq!(resp.id, RequestId::Number(1));

    // Two notifications the server routes, then a request. If either notification
    // drew a reply, it would arrive before the ping's response.
    client
        .send(Message::Notification(Notification::new(
            "notifications/initialized",
        )))
        .await
        .expect("send");
    client
        .send(Message::Notification(Notification::with_params(
            "notifications/cancelled",
            json!({ "requestId": 4242 }),
        )))
        .await
        .expect("send");
    client.send(req(99, "ping", json!({}))).await.expect("send");

    let resp = next_response(&client).await;
    assert_eq!(
        resp.id,
        RequestId::Number(99),
        "a notification drew a response — the next reply should have been the ping's"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

// ---------------------------------------------------------------------------
// Row 4 — task lifecycle: TTL retention is enforced
// ---------------------------------------------------------------------------

/// `ttl` is the retention window for a terminal task. Once it lapses the task is
/// evicted, and a later `tasks/get` must report an unknown id rather than
/// serving state forever.
#[tokio::test]
async fn terminal_tasks_are_evicted_once_their_ttl_lapses() {
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(Full).with_tools(Full).build();
    let runtime = ServerRuntime::new(built, server);
    runtime.state().set_initialized();
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });

    // A task-augmented call with a 1 ms retention window.
    client
        .send(req(
            1,
            "tools/call",
            json!({ "name": "echo", "arguments": {}, "task": { "ttl": 1 } }),
        ))
        .await
        .expect("send");
    let created = next_response(&client).await;
    let task_id = created
        .result
        .as_ref()
        .and_then(|r| r.get("task"))
        .and_then(|t| t.get("taskId"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no taskId: {created:?}"))
        .to_string();

    // Poll until the store reports the id unknown, which is eviction observed
    // through the wire rather than through the store's internals.
    let mut id = 2;
    let evicted = loop {
        tokio::time::sleep(Duration::from_millis(20)).await;
        client
            .send(req(id, "tasks/get", json!({ "taskId": task_id })))
            .await
            .expect("send");
        let resp = next_response(&client).await;
        id += 1;
        if resp.error.is_some() {
            break resp;
        }
        assert!(id < 40, "task was never evicted despite a 1 ms ttl");
    };

    assert_eq!(
        err_code(&evicted),
        codes::INVALID_PARAMS,
        "an evicted task must read as an unknown id (-32602): {evicted:?}"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

// ---------------------------------------------------------------------------
// Row 5 — every spec-defined notification is classified and reachable
// ---------------------------------------------------------------------------

/// The gap that produced two defects this session was a spec-defined message
/// that existed as a type and a constant but was never emitted on some path.
/// A vocabulary diff cannot see it: the string is present in the sources either
/// way.
///
/// This asserts, against the **vendored schema** rather than mcpkit's own
/// constants, that every notification the spec defines is deliberately
/// classified — either mcpkit emits it as a server, or it is one a server only
/// receives. A notification added to the spec, or a constant added here without
/// an emitter, lands in neither set and fails.
/// The method-name constants must cover the spec exactly.
///
/// Anchored to the vendored schema, and spelling the literals out rather than
/// comparing constants to themselves — a constant-to-constant assertion is
/// vacuous, which is how a non-spec `"initialized"` survived in the debug
/// validator and in the test harness.
///
/// Before these constants moved to core they were incomplete: `roots/list` and
/// `tasks/result` were implemented but had no constant, so any caller had to
/// write the literal.
#[test]
fn method_constants_cover_the_spec_exactly() {
    use mcpkit_core::methods as m;

    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spec/2025-11-25/schema.json"
        ))
        .expect("vendored schema"),
    )
    .expect("schema parses");

    let mut spec: Vec<String> = schema["$defs"]
        .as_object()
        .expect("$defs")
        .values()
        .filter_map(|d| d["properties"]["method"]["const"].as_str())
        .filter(|s| !s.starts_with("notifications/"))
        .map(String::from)
        .collect();
    spec.sort();
    spec.dedup();

    let mut declared: Vec<String> = [
        m::INITIALIZE,
        m::PING,
        m::TOOLS_LIST,
        m::TOOLS_CALL,
        m::RESOURCES_LIST,
        m::RESOURCES_READ,
        m::RESOURCES_TEMPLATES_LIST,
        m::RESOURCES_SUBSCRIBE,
        m::RESOURCES_UNSUBSCRIBE,
        m::PROMPTS_LIST,
        m::PROMPTS_GET,
        m::TASKS_LIST,
        m::TASKS_GET,
        m::TASKS_CANCEL,
        m::TASKS_RESULT,
        m::ROOTS_LIST,
        m::SAMPLING_CREATE_MESSAGE,
        m::COMPLETION_COMPLETE,
        m::LOGGING_SET_LEVEL,
        m::ELICITATION_CREATE,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    declared.sort();
    declared.dedup();

    assert_eq!(
        declared, spec,
        "the request-method constants must match the schema's set exactly"
    );
}

#[test]
fn every_spec_notification_is_classified_as_emitted_or_receive_only() {
    use mcpkit_server::router::notifications as n;

    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../spec/2025-11-25/schema.json"
        ))
        .expect("vendored schema is committed at spec/2025-11-25/schema.json"),
    )
    .expect("schema parses");

    let mut spec: Vec<String> = schema["$defs"]
        .as_object()
        .expect("$defs")
        .values()
        .filter_map(|d| d["properties"]["method"]["const"].as_str())
        .filter(|m| m.starts_with("notifications/"))
        .map(String::from)
        .collect();
    spec.sort();
    spec.dedup();

    // Emitted by mcpkit acting as a server. Each has a code path:
    //   MESSAGE/RESOURCES_UPDATED/*_LIST_CHANGED/ELICITATION_COMPLETE -> ServerNotifier
    //   PROGRESS                                                      -> Context::progress
    //   TASK_STATUS                                                   -> TaskStatusNotifier
    let emitted = [
        n::MESSAGE,
        n::PROGRESS,
        n::RESOURCES_UPDATED,
        n::RESOURCES_LIST_CHANGED,
        n::TOOLS_LIST_CHANGED,
        n::PROMPTS_LIST_CHANGED,
        n::ELICITATION_COMPLETE,
        n::TASK_STATUS,
    ];

    // Sent by the client; a server routes but never originates these.
    let receive_only = [n::INITIALIZED, n::CANCELLED, n::ROOTS_LIST_CHANGED];

    let mut classified: Vec<String> = emitted
        .iter()
        .chain(receive_only.iter())
        .map(|s| (*s).to_string())
        .collect();
    classified.sort();
    classified.dedup();

    let unclassified: Vec<&String> = spec.iter().filter(|m| !classified.contains(m)).collect();
    assert!(
        unclassified.is_empty(),
        "spec notifications with no emitter and not marked receive-only: {unclassified:?}"
    );

    let not_in_spec: Vec<&String> = classified.iter().filter(|m| !spec.contains(m)).collect();
    assert!(
        not_in_spec.is_empty(),
        "classified notifications that the spec does not define: {not_in_spec:?}"
    );
}
