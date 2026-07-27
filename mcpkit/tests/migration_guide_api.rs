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
#![allow(clippy::needless_continue)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::path_buf_push_overwrite)]
#![allow(clippy::unnecessary_debug_formatting)]

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
