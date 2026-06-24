//! `#[tool(title = .., task_support = ..)]` populates `Tool.title` and
//! `Tool.execution.taskSupport`.

use mcpkit::mcp_server;
use mcpkit::server::{Context, NoOpPeer, ToolHandler};
use mcpkit::types::TaskSupport;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
use mcpkit_core::protocol_version::ProtocolVersion;

struct Srv;

#[mcp_server(name = "srv", version = "1.0.0")]
impl Srv {
    /// A tool that advertises display + task metadata.
    #[tool(
        description = "long job",
        title = "Long Job",
        task_support = "optional"
    )]
    async fn run(&self) -> String {
        "ok".to_string()
    }
}

#[tokio::test]
async fn tool_advertises_title_and_task_support() {
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

    let tools = <Srv as ToolHandler>::list_tools(&Srv, &ctx)
        .await
        .expect("list_tools");
    let tool = tools.iter().find(|t| t.name == "run").expect("run tool");

    assert_eq!(tool.title.as_deref(), Some("Long Job"));
    assert_eq!(
        tool.execution.as_ref().and_then(|e| e.task_support),
        Some(TaskSupport::Optional),
    );
}
