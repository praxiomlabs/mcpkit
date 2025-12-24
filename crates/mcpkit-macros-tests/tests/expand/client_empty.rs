//! Test: Empty client handler expands correctly

use mcpkit::mcp_client;

struct EmptyHandler;

#[mcp_client]
impl EmptyHandler {}

fn main() {
    let handler = EmptyHandler;
    // Should have capabilities() method that returns default capabilities
    let _caps = handler.capabilities();
}
