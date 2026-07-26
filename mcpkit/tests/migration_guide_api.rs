//! Compiles the API paths `docs/migration-from-rmcp.md` claims, on both sides.
//!
//! The guide documents rmcp's API as well as mcpkit's, and nothing checked it —
//! so it kept describing rmcp 1.7 long after 2.x moved `Server::builder()` and
//! `StdioTransport` out from under it. Prose cannot be compiled; these `use`
//! statements can, so an rmcp release that moves a path fails here instead of
//! leaving readers with samples that do not build.
//!
//! This asserts the paths exist and resolve. It does not assert the guide's
//! surrounding narrative is right.

/// The rmcp "before" imports, exactly as the guide shows them.
#[allow(unused_imports)]
mod rmcp_before {
    use rmcp::{
        ErrorData as McpError, ServerHandler, ServiceExt,
        model::{
            CallToolResult, ContentBlock, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
        },
        transport::stdio,
    };

    // Named so an unused-import lint cannot quietly hollow this out.
    #[allow(dead_code)]
    fn paths_resolve(_: Option<McpError>, _: Option<CallToolResult>, _: Option<ContentBlock>) {}
}

/// The mcpkit "after" imports, exactly as the guide shows them.
#[allow(unused_imports)]
mod mcpkit_after {
    use mcpkit::prelude::*;
    use mcpkit::types::Object;
    use mcpkit_server::handler::ToolHandler;
    use mcpkit_transport::SyncStdioTransport;

    #[allow(dead_code)]
    fn paths_resolve(_: Option<Object>) {}
}

/// `stdio()` yields the stdin/stdout pair rmcp 2.x serves over — not a
/// transport struct, which is the change the guide now calls out.
#[test]
fn rmcp_stdio_is_a_pair_not_a_transport_struct() {
    fn _assert_shape() -> (tokio::io::Stdin, tokio::io::Stdout) {
        rmcp::transport::stdio()
    }
}

/// mcpkit's stdio transport is still a struct with `new()`, which is what makes
/// the two columns of the guide's transport table different.
#[test]
fn mcpkit_sync_stdio_transport_is_constructible() {
    let _ = mcpkit_transport::SyncStdioTransport::new();
}
