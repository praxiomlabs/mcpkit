//! #19: `#[mcp_server]` must reject a generic impl block with a clear error
//! instead of generating malformed trait impls referencing undefined `T`.

use mcpkit::mcp_server;

struct Handler<T>(T);

#[mcp_server(name = "h", version = "1.0.0")]
impl<T> Handler<T> {
    #[tool(description = "noop")]
    async fn noop(&self) -> mcpkit::types::ToolOutput {
        mcpkit::types::ToolOutput::text("ok")
    }
}

fn main() {}
