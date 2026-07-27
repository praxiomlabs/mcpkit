//! `#[tool(title = .., task_support = ..)]` populates `Tool.title` and
//! `Tool.execution.taskSupport`.

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

#[test]
fn task_support_tool_advertises_tasks_capability() {
    use mcpkit::server::ServerHandler;
    // A `#[tool(task_support = "optional")]` makes the server task-augmentable,
    // so the macro must advertise the `tasks` capability (#81).
    assert!(Srv.capabilities().has_tasks());
}
