//! Test: Client with elicitation handler expands correctly

use mcpkit::mcp_client;
use mcpkit::types::{ElicitRequest, ElicitResult};
use mcpkit::error::McpError;

struct ElicitationHandler;

#[mcp_client]
impl ElicitationHandler {
    #[elicitation]
    async fn handle_elicitation(&self, _request: ElicitRequest) -> Result<ElicitResult, McpError> {
        Ok(ElicitResult::declined())
    }
}

fn main() {
    let handler = ElicitationHandler;
    // Should have capabilities with elicitation enabled
    let caps = handler.capabilities();
    assert!(caps.elicitation.is_some());
}
