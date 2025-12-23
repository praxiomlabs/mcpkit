//! Test: Server with multiple tools expands correctly

use mcpkit::mcp_server;
use serde_json as _;  // Re-export for generated code

struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> mcpkit::types::ToolOutput {
        mcpkit::types::ToolOutput::text((a + b).to_string())
    }

    /// Multiply two numbers
    #[tool(description = "Multiply two numbers", idempotent = true)]
    async fn multiply(&self, a: f64, b: f64) -> mcpkit::types::ToolOutput {
        mcpkit::types::ToolOutput::text((a * b).to_string())
    }

    /// Divide two numbers
    #[tool(description = "Divide first number by second", destructive = false, read_only = true)]
    async fn divide(&self, a: f64, b: f64) -> Result<mcpkit::types::ToolOutput, mcpkit::error::McpError> {
        if b == 0.0 {
            return Err(mcpkit::error::McpError::invalid_params("divide", "Cannot divide by zero"));
        }
        Ok(mcpkit::types::ToolOutput::text((a / b).to_string()))
    }
}

fn main() {}
