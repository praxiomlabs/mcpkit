// Test: Missing required 'name' attribute on mcp_server

use mcpkit_macros::mcp_server;

struct MyServer;

#[mcp_server(version = "1.0.0")]
impl MyServer {}

fn main() {}
