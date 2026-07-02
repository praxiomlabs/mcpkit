//! Test: Client with sampling handler expands correctly

use mcpkit::mcp_client;
use mcpkit::types::{CreateMessageRequest, CreateMessageResult, OneOrMany, Role, SamplingContent, StopReason};
use mcpkit::error::McpError;

struct SamplingHandler;

#[mcp_client]
impl SamplingHandler {
    #[sampling]
    async fn handle_sampling(&self, _request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
        Ok(CreateMessageResult {
            model: "test-model".to_string(),
            role: Role::Assistant,
            content: OneOrMany::One(SamplingContent::text("Hello!")),
            stop_reason: Some(StopReason::EndTurn),
            meta: None,
        })
    }
}

fn main() {
    let handler = SamplingHandler;
    // Should have capabilities with sampling enabled
    let caps = handler.capabilities();
    assert!(caps.sampling.is_some());
}
