// Test: Typo in tool attribute should suggest correct spelling

#[allow(unused_imports)]
use mcpkit_macros::{mcp_server, tool};

struct MyServer;

#[mcp_server(name = "test", version = "1.0.0")]
impl MyServer {
    #[tool(descripion = "test")]  // Typo: "descripion" instead of "description"
    async fn my_tool(&self) -> mcpkit_core::types::ToolOutput {
        mcpkit_core::types::ToolOutput::text("done")
    }
}

fn main() {}
