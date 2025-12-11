//! Test: Server with multiple tools expands correctly

use mcpkit_core as _;  // Re-export for generated code
use mcpkit_macros::mcp_server;
use mcpkit_server as _;  // Re-export for generated code
use serde_json as _;  // Re-export for generated code

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> mcpkit_core::types::ToolOutput {
        mcpkit_core::types::ToolOutput::text((a + b).to_string())
    }

    /// Multiply two numbers
    #[tool(description = "Multiply two numbers", idempotent = true)]
    async fn multiply(&self, a: f64, b: f64) -> mcpkit_core::types::ToolOutput {
        mcpkit_core::types::ToolOutput::text((a * b).to_string())
    }

    /// Divide two numbers
    #[tool(description = "Divide first number by second", destructive = false, read_only = true)]
    async fn divide(&self, a: f64, b: f64) -> Result<mcpkit_core::types::ToolOutput, mcpkit_core::error::McpError> {
        if b == 0.0 {
            return Err(mcpkit_core::error::McpError::invalid_params("divide", "Cannot divide by zero"));
        }
        Ok(mcpkit_core::types::ToolOutput::text((a / b).to_string()))
    }
}

fn main() {}
