// Test: Missing required 'uri_pattern' attribute on resource

#[allow(unused_imports)]
use mcpkit_macros::{mcp_server, resource};

struct MyServer;

#[mcp_server(name = "test", version = "1.0.0")]
impl MyServer {
    #[resource(name = "Config")]
    async fn get_config(&self, _uri: &str) -> mcpkit_core::types::ResourceContents {
        mcpkit_core::types::ResourceContents::text("config://app", "value")
    }
}

fn main() {}
