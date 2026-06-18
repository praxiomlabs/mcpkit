//! Regression test: generated tool input-schema property order is deterministic.
//!
//! The macro previously collected schema `properties` through a `HashMap`, whose
//! iteration order is randomized per instance, so `tools/list` emitted
//! differently-ordered schemas across runs (breaking caching / snapshot tests).

use mcpkit::mcp_server;
use mcpkit::server::{Context, NoOpPeer, ToolHandler};
use mcpkit::types::ToolOutput;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
use mcpkit_core::protocol_version::ProtocolVersion;

struct S;

#[mcp_server(name = "s", version = "1.0.0")]
impl S {
    #[tool(description = "many params in a fixed declaration order")]
    async fn many(
        &self,
        zebra: String,
        alpha: i64,
        mike: bool,
        bravo: f64,
        yankee: String,
        charlie: i64,
    ) -> ToolOutput {
        let _ = (zebra, alpha, mike, bravo, yankee, charlie);
        ToolOutput::text("ok")
    }
}

async fn property_order(handler: &S) -> Vec<String> {
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
    let tools = <S as ToolHandler>::list_tools(handler, &ctx)
        .await
        .expect("list_tools");
    let tool = tools.iter().find(|t| t.name == "many").expect("many tool");
    tool.input_schema["properties"]
        .as_object()
        .expect("properties object")
        .keys()
        .cloned()
        .collect()
}

#[tokio::test]
async fn tool_schema_property_order_is_deterministic() {
    let handler = S;
    let first = property_order(&handler).await;
    let second = property_order(&handler).await;
    assert_eq!(
        first, second,
        "tool input-schema property order must be deterministic across calls"
    );
    assert_eq!(first.len(), 6, "all six parameters should be present");
}
