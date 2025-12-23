//! Test: Server with only prompts expands correctly

use mcpkit::mcp_server;
use serde_json as _;  // Re-export for generated code

struct PromptServer;

#[mcp_server(name = "prompt-server", version = "1.0.0")]
impl PromptServer {
    /// Generate a greeting prompt
    #[prompt(description = "Generate a personalized greeting")]
    async fn greeting(&self, name: String) -> mcpkit::types::GetPromptResult {
        mcpkit::types::GetPromptResult {
            description: Some("A greeting prompt".to_string()),
            messages: vec![
                mcpkit::types::PromptMessage::user(format!("Say hello to {}", name)),
            ],
        }
    }
}

fn main() {}
