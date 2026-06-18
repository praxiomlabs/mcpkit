//! Regression test for #14: `#[mcp(default = ..., min = ..., max = ...)]` on a
//! tool parameter must be emitted into the generated JSON Schema (and the
//! helper attribute must be stripped so the impl still compiles).

use mcpkit::mcp_server;
use mcpkit::server::{Context, NoOpPeer, ToolHandler};
use mcpkit::types::ToolOutput;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
use mcpkit_core::protocol_version::ProtocolVersion;

struct Search;

#[mcp_server(name = "search", version = "1.0.0")]
impl Search {
    /// Search for things.
    #[tool(description = "Search")]
    async fn search(
        &self,
        /// The query string
        query: String,
        /// Maximum results to return
        #[mcp(default = 10, min = 1, max = 100)]
        limit: i64,
    ) -> ToolOutput {
        let _ = (query, limit);
        ToolOutput::text("ok")
    }
}

#[tokio::test]
async fn mcp_param_attrs_are_emitted_into_input_schema() {
    let handler = Search;
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

    let tools = <Search as ToolHandler>::list_tools(&handler, &ctx)
        .await
        .expect("list_tools");
    let tool = tools
        .iter()
        .find(|t| t.name == "search")
        .expect("search tool present");

    let limit = &tool.input_schema["properties"]["limit"];
    assert_eq!(limit["default"], serde_json::json!(10), "schema: {limit}");
    assert_eq!(limit["minimum"], serde_json::json!(1), "schema: {limit}");
    assert_eq!(limit["maximum"], serde_json::json!(100), "schema: {limit}");

    // The plain parameter has no default/min/max, but its doc comment becomes
    // the schema description (and the doc attribute is stripped so it compiles).
    let query = &tool.input_schema["properties"]["query"];
    assert!(query.get("default").is_none());
    assert!(query.get("minimum").is_none());
    assert_eq!(query["description"], serde_json::json!("The query string"));
}
