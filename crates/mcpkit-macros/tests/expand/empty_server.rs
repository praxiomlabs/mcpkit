//! Test: Server with no tools/resources/prompts expands correctly

use mcpkit_core as _;  // Re-export for generated code
use mcpkit_macros::mcp_server;
use mcpkit_server as _;  // Re-export for generated code
use serde_json as _;  // Re-export for generated code

struct EmptyServer;

#[mcp_server(name = "empty", version = "0.1.0")]
impl EmptyServer {}

fn main() {}
