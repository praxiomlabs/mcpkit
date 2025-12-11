//! Test: Server with only tools expands correctly

use mcpkit_core as _;  // Re-export for generated code
use mcpkit_macros::mcp_server;
use mcpkit_server as _;  // Re-export for generated code
use serde_json as _;  // Re-export for generated code

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together
    #[tool(description = "Add two numbers")]
    async fn add(&self, a: f64, b: f64) -> mcpkit_core::types::ToolOutput {
        mcpkit_core::types::ToolOutput::text((a + b).to_string())
    }
}

fn main() {}
