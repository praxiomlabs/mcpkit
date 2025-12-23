//! Test: Server with only tools expands correctly

use mcpkit::mcp_server;
use serde_json as _;  // Re-export for generated code

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> mcpkit::types::ToolOutput {
        mcpkit::types::ToolOutput::text((a + b).to_string())
    }
}

fn main() {}
