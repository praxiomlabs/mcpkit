//! A `#[tool]` returning `Json<T>` populates the result's `structuredContent`.

use mcpkit::ToolInput;
use mcpkit::mcp_server;
use mcpkit::server::{Context, NoOpPeer, ToolHandler};
use mcpkit::types::{CallToolResult, Json};
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
use mcpkit_core::protocol_version::ProtocolVersion;
use serde::Serialize;

#[derive(Serialize, ToolInput)]
struct Sum {
    /// The sum of the two operands.
    total: i64,
}

struct Calc;

#[mcp_server(name = "calc", version = "1.0.0")]
impl Calc {
    /// Add two numbers and return structured output.
    #[tool(description = "add")]
    async fn add(&self, a: i64, b: i64) -> Json<Sum> {
        Json(Sum { total: a + b })
    }
}

#[tokio::test]
async fn json_return_populates_structured_content() {
    let handler = Calc;
    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;
    let ctx = Context::new(
        &request_id,
        None,
        &client_caps,
        &server_caps,
        ProtocolVersion::LATEST,
        &peer,
    );

    let output = <Calc as ToolHandler>::call_tool(
        &handler,
        "add",
        serde_json::json!({"a": 2, "b": 3}),
        &ctx,
    )
    .await
    .expect("call_tool");

    let result: CallToolResult = output.into();
    assert_eq!(
        result.structured_content,
        Some(serde_json::json!({"total": 5})),
        "Json<T> return should populate structuredContent"
    );
    // A human-readable JSON fallback is still present in content.
    assert!(
        !result.content.is_empty(),
        "expected a text content fallback"
    );
}

#[tokio::test]
async fn json_return_advertises_output_schema() {
    let handler = Calc;
    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;
    let ctx = Context::new(
        &request_id,
        None,
        &client_caps,
        &server_caps,
        ProtocolVersion::LATEST,
        &peer,
    );

    let tools = <Calc as ToolHandler>::list_tools(&handler, &ctx)
        .await
        .expect("list_tools");
    let add = tools.iter().find(|t| t.name == "add").expect("add tool");
    let schema = add
        .output_schema
        .as_ref()
        .expect("output_schema should be derived from the Json<Sum> return");
    assert_eq!(schema["properties"]["total"]["type"], "integer");
}
