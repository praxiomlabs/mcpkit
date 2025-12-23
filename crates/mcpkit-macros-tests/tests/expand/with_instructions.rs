//! Test: Server with instructions attribute expands correctly

use mcpkit::mcp_server;
use serde_json as _;  // Re-export for generated code

struct InstructionServer;

#[mcp_server(
    name = "instruction-server",
    version = "1.0.0",
    instructions = "This server provides mathematical operations. Use the add tool to add numbers."
)]
impl InstructionServer {
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> mcpkit::types::ToolOutput {
        mcpkit::types::ToolOutput::text((a + b).to_string())
    }
}

fn main() {}
