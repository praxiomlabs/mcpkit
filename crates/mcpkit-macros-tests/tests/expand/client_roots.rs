//! Test: Client with roots handler expands correctly

use mcpkit::mcp_client;
use mcpkit::client::handler::Root;
use mcpkit::error::McpError;

struct RootsHandler {
    roots: Vec<Root>,
}

#[mcp_client]
impl RootsHandler {
    #[roots]
    async fn list_roots(&self) -> Result<Vec<Root>, McpError> {
        Ok(self.roots.clone())
    }
}

fn main() {
    let handler = RootsHandler {
        roots: vec![Root {
            uri: "file:///workspace".to_string(),
            name: Some("Workspace".to_string()),
        }],
    };
    // Should have capabilities with roots enabled
    let caps = handler.capabilities();
    assert!(caps.roots.is_some());
}
