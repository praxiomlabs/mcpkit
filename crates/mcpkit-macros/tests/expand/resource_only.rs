//! Test: Server with only resources expands correctly

use mcpkit::mcp_server;
use serde_json as _;  // Re-export for generated code

struct FileServer;

#[mcp_server(name = "file-server", version = "1.0.0")]
impl FileServer {
    /// Get file contents
    #[resource(
        uri_pattern = "file:///{path}",
        name = "File Contents",
        description = "Read file contents by path",
        mime_type = "text/plain"
    )]
    async fn get_file(&self, uri: &str) -> mcpkit::types::ResourceContents {
        mcpkit::types::ResourceContents::text(uri, "file contents")
    }
}

fn main() {}
