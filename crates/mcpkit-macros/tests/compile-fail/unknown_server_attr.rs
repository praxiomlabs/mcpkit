// Test: Unknown attribute on mcp_server should produce helpful error

use mcpkit_macros::mcp_server;

struct MyServer;

#[mcp_server(name = "test", version = "1.0.0", unknown_attr = "value")]
impl MyServer {}

fn main() {}
