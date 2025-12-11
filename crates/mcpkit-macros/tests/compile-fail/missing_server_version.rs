// Test: Missing required 'version' attribute on mcp_server

use mcpkit_macros::mcp_server;

struct MyServer;

#[mcp_server(name = "test-server")]
impl MyServer {}

fn main() {}
