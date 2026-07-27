//! Spec-anchored error-code checks.
//!
//! A schema diff cannot cover these: `schema.json` carries exactly one error
//! code (`-32042`), so the JSON-RPC codes live in prose only. Every assertion
//! here checks the *correct* code, not merely that an error occurred — a test
//! that accepts any error passes just as happily when the code is wrong.

use mcpkit_core::error::codes;
use mcpkit_core::protocol::{Message, Request, RequestId, Response};
use mcpkit_server::{ServerBuilder, ServerRuntime};
use mcpkit_transport::{MemoryTransport, Transport};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

struct H;

impl mcpkit_server::ServerHandler for H {
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo::new("error-codes", "1.0.0")
    }
}

impl mcpkit_server::ToolHandler for H {
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

fn req(id: u64, method: &'static str, params: serde_json::Value) -> Message {
    Message::Request(Request::with_params(method, RequestId::Number(id), params))
}

/// Next response, skipping notifications the server may publish meanwhile.
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

fn error_code(response: &Response) -> i32 {
    response
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("expected an error response, got {response:?}"))
        .code
}

/// Spin an initialized server and return the client end of the transport.
fn serve() -> (MemoryTransport, tokio::task::JoinHandle<()>) {
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(H).with_tools(H).build();
    let runtime = ServerRuntime::new(built, server);
    runtime.state().set_initialized();
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });
    (client, handle)
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let (client, handle) = serve();

    client
        .send(req(1, "no/such/method", json!({})))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        error_code(&resp),
        codes::METHOD_NOT_FOUND,
        "unknown method must be -32601"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn malformed_params_are_invalid_params() {
    let (client, handle) = serve();

    // `tools/call` requires a `name`; omitting it is a params defect, not a
    // missing method and not an internal error.
    client
        .send(req(1, "tools/call", json!({ "arguments": {} })))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        error_code(&resp),
        codes::INVALID_PARAMS,
        "malformed params must be -32602"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn unknown_task_id_is_invalid_params() {
    let (client, handle) = serve();

    client
        .send(req(1, "tasks/get", json!({ "taskId": "does-not-exist" })))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        error_code(&resp),
        codes::INVALID_PARAMS,
        "unknown taskId must be -32602 per spec"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn cancelling_a_terminal_task_is_invalid_params() {
    let (client, handle) = serve();

    // Create a task-augmented call and let it reach a terminal status.
    client
        .send(req(
            1,
            "tools/call",
            json!({ "name": "echo", "arguments": {}, "task": { "ttl": 60000 } }),
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
        .unwrap_or_else(|| panic!("no taskId in CreateTaskResult: {created:?}"))
        .to_string();

    // Poll until terminal so the cancel below is genuinely a terminal cancel.
    let mut id = 2;
    loop {
        client
            .send(req(id, "tasks/get", json!({ "taskId": task_id })))
            .await
            .expect("send");
        let resp = next_response(&client).await;
        let status = resp
            .result
            .as_ref()
            .and_then(|r| r.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        id += 1;
        if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
            break;
        }
        assert!(id < 50, "task never reached a terminal status");
        tokio::task::yield_now().await;
    }

    client
        .send(req(id, "tasks/cancel", json!({ "taskId": task_id })))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        error_code(&resp),
        codes::INVALID_PARAMS,
        "cancelling a terminal task must be -32602 per spec"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}

#[tokio::test]
async fn initialize_after_initialize_is_invalid_request() {
    let (client, handle) = serve();

    // `serve()` already marked the session initialized, so this is a duplicate.
    client
        .send(req(
            1,
            "initialize",
            json!({
                "protocolVersion": mcpkit_core::capability::PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "c", "version": "1.0" }
            }),
        ))
        .await
        .expect("send");
    let resp = next_response(&client).await;
    assert_eq!(
        error_code(&resp),
        codes::INVALID_REQUEST,
        "a second initialize must be -32600"
    );

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
}
