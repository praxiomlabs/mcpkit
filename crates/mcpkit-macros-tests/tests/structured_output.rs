//! A `#[tool]` returning `Json<T>` populates the result's `structuredContent`.

// Test / example code: assertion shapes, fixture naming and framework-shaped
// signatures are written for readability at the call site, not to satisfy
// pedantic/nursery lints. None of this ships in the library.
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_else)]
#![allow(clippy::wildcard_enum_match_arm)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::unused_async)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::future_not_send)]
#![allow(clippy::type_complexity)]

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
        serde_json::from_value(serde_json::json!({"a": 2, "b": 3})).expect("object"),
        &ctx,
    )
    .await
    .expect("call_tool");

    let result: CallToolResult = output.into();
    assert_eq!(
        result.structured_content,
        Some(serde_json::from_value(serde_json::json!({"total": 5})).expect("object")),
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
