// Test: Missing required 'description' attribute on prompt

#[allow(unused_imports)]
use mcpkit_macros::{mcp_server, prompt};

struct MyServer;

#[mcp_server(name = "test", version = "1.0.0")]
impl MyServer {
    #[prompt]
    async fn greeting(&self, name: String) -> mcpkit_core::types::GetPromptResult {
        mcpkit_core::types::GetPromptResult {
            description: None,
            messages: vec![],
        }
    }
}

fn main() {}
