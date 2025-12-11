// Test: Missing required 'description' attribute on tool

use mcpkit_macros::{mcp_server, tool};

struct MyServer;

#[mcp_server(name = "test", version = "1.0.0")]
impl MyServer {
    #[tool]
    async fn my_tool(&self) -> mcpkit_core::types::ToolOutput {
        mcpkit_core::types::ToolOutput::text("done")
    }
}

fn main() {}
