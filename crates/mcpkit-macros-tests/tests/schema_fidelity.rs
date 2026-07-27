//! Regression tests for #19: macro schema fidelity.
//!
//! - Qualified type paths (`std::string::String`) resolve to the right schema
//!   (previously only bare `String` matched).
//! - `Option<T>` resolves to the inner type's schema and is omitted from
//!   `required`.

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
use mcpkit::types::ToolOutput;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
use mcpkit_core::protocol_version::ProtocolVersion;

struct Q;

#[mcp_server(name = "q", version = "1.0.0")]
impl Q {
    /// Echo back.
    #[tool(description = "echo")]
    async fn echo(
        &self,
        /// A qualified-path string parameter.
        text: std::string::String,
        /// An optional integer parameter.
        limit: Option<i64>,
    ) -> ToolOutput {
        let _ = (text, limit);
        ToolOutput::text("ok")
    }
}

#[tokio::test]
async fn qualified_paths_and_option_resolve_in_schema() {
    let handler = Q;
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

    let tools = <Q as ToolHandler>::list_tools(&handler, &ctx)
        .await
        .expect("list_tools");
    let tool = tools.iter().find(|t| t.name == "echo").expect("echo tool");

    // `std::string::String` resolves to a string schema (a confusing compile
    // error before this fix).
    assert_eq!(tool.input_schema["properties"]["text"]["type"], "string");
    // `Option<i64>` resolves to the inner integer schema...
    assert_eq!(tool.input_schema["properties"]["limit"]["type"], "integer");

    // ...and the optional parameter is not in `required`, while the plain one is.
    let required = tool.input_schema["required"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        required.iter().any(|v| v == "text"),
        "text must be required"
    );
    assert!(
        !required.iter().any(|v| v == "limit"),
        "Option<i64> must not be required"
    );
}
