//! Example MCP server using the smol async runtime.
//!
//! This example demonstrates how to use mcpkit with the smol runtime
//! instead of Tokio. smol is a lightweight async runtime that's a good
//! choice for smaller applications or when you want minimal dependencies.
//!
//! ## Running
//!
//! ```bash
//! cargo run -p smol-server
//! ```
//!
//! ## Key Differences from Tokio
//!
//! 1. Use `smol::block_on()` instead of `#[tokio::main]`
//! 2. Enable the `smol-runtime` feature on mcpkit-transport
//! 3. Use `futures_lite` for stream operations instead of `tokio_stream`
//!
//! ## When to Use smol
//!
//! - Smaller binary size requirements
//! - Minimal dependency footprint
//! - WASM compatibility (smol has better WASM support)
//! - Single-threaded applications
//! - Learning async Rust (simpler than Tokio)

use mcpkit::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// A simple echo server that demonstrates the smol runtime.
struct EchoServer {
    message_count: AtomicU64,
}

impl EchoServer {
    fn new() -> Self {
        Self {
            message_count: AtomicU64::new(0),
        }
    }
}

#[mcp_server(name = "smol-echo-server", version = "1.0.0")]
impl EchoServer {
    /// Echo back the input message.
    ///
    /// A simple tool that returns the input message,
    /// demonstrating that async/await works with smol.
    #[tool(description = "Echo back the input message")]
    async fn echo(&self, message: String) -> ToolOutput {
        self.message_count.fetch_add(1, Ordering::Relaxed);

        // Simulate some async work
        // Note: We use smol's async primitives instead of Tokio's
        smol::Timer::after(std::time::Duration::from_millis(10)).await;

        ToolOutput::text(format!("Echo: {message}"))
    }

    /// Get server statistics.
    #[tool(description = "Get the number of messages processed", read_only = true)]
    async fn stats(&self) -> ToolOutput {
        let count = self.message_count.load(Ordering::Relaxed);
        ToolOutput::text(format!("Messages processed: {count}"))
    }

    /// Demonstrate concurrent async operations with smol.
    #[tool(description = "Run multiple async operations concurrently")]
    async fn concurrent_demo(&self) -> ToolOutput {
        use futures_lite::future;

        // Run multiple timers concurrently using nested zip
        // future::zip runs two futures concurrently and returns (A, B)
        let ((_, _), _) = future::zip(
            future::zip(
                smol::Timer::after(std::time::Duration::from_millis(50)),
                smol::Timer::after(std::time::Duration::from_millis(50)),
            ),
            smol::Timer::after(std::time::Duration::from_millis(50)),
        )
        .await;

        ToolOutput::text("Completed 3 concurrent operations in ~50ms (not 150ms)")
    }
}

fn main() -> Result<(), McpError> {
    // smol uses block_on instead of an attribute macro
    smol::block_on(async {
        println!("smol Echo Server Example");
        println!("========================");
        println!();
        println!("This example demonstrates using mcpkit with the smol async runtime.");
        println!();

        // Create the echo server
        let server = EchoServer::new();

        // Display server info
        let info = <EchoServer as ServerHandler>::server_info(&server);
        println!("Server: {} v{}", info.name, info.version);

        let caps = <EchoServer as ServerHandler>::capabilities(&server);
        println!("Has tools: {}", caps.has_tools());
        println!();

        // Set up context for testing
        use mcpkit::server::NoOpPeer;
        use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
        use mcpkit_core::protocol::RequestId;
        use mcpkit_core::protocol_version::ProtocolVersion;

        let request_id = RequestId::Number(1);
        let client_caps = ClientCapabilities::default();
        let server_caps = ServerCapabilities::default();
        let peer = NoOpPeer;

        let ctx = Context::new(
            &request_id,
            None,
            &client_caps,
            &server_caps,
            ProtocolVersion::LATEST,
            &peer,
        );

        // List available tools
        let tools = <EchoServer as ToolHandler>::list_tools(&server, &ctx).await?;
        println!("Available tools:");
        for tool in &tools {
            println!(
                "  - {} : {}",
                tool.name,
                tool.description.as_deref().unwrap_or("")
            );
        }
        println!();

        // Call the echo tool
        println!("Calling echo(\"Hello from smol!\")...");
        let args = serde_json::json!({"message": "Hello from smol!"});
        let result = <EchoServer as ToolHandler>::call_tool(&server, "echo", args, &ctx).await?;
        print_result(&result);
        println!();

        // Call echo again to increment counter
        println!("Calling echo(\"Second message\")...");
        let args = serde_json::json!({"message": "Second message"});
        let result = <EchoServer as ToolHandler>::call_tool(&server, "echo", args, &ctx).await?;
        print_result(&result);
        println!();

        // Check stats
        println!("Calling stats()...");
        let args = serde_json::json!({});
        let result = <EchoServer as ToolHandler>::call_tool(&server, "stats", args, &ctx).await?;
        print_result(&result);
        println!();

        // Demonstrate concurrent operations
        println!("Calling concurrent_demo()...");
        let start = std::time::Instant::now();
        let args = serde_json::json!({});
        let result =
            <EchoServer as ToolHandler>::call_tool(&server, "concurrent_demo", args, &ctx).await?;
        println!("Completed in {:?}", start.elapsed());
        print_result(&result);
        println!();

        // Create server instance
        println!("=== Runtime Comparison ===");
        println!();
        println!("Tokio:");
        println!("  #[tokio::main]");
        println!("  async fn main() {{ ... }}");
        println!();
        println!("smol:");
        println!("  fn main() {{");
        println!("      smol::block_on(async {{ ... }})");
        println!("  }}");
        println!();
        println!("Both work with the same mcpkit API!");

        // Create server with into_server() to verify it works
        let _server = EchoServer::new().into_server();
        println!();
        println!("Server created successfully with smol runtime!");

        Ok(())
    })
}

fn print_result(result: &ToolOutput) {
    match result {
        ToolOutput::Success(r) => {
            for content in &r.content {
                if let Content::Text(tc) = content {
                    println!("Result: {}", tc.text);
                }
            }
        }
        ToolOutput::RecoverableError { message, .. } => {
            println!("Error: {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit::server::{Context, NoOpPeer};
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;

    struct TestContext {
        request_id: RequestId,
        client_caps: ClientCapabilities,
        server_caps: ServerCapabilities,
        protocol_version: ProtocolVersion,
        peer: NoOpPeer,
    }

    impl TestContext {
        fn new() -> Self {
            Self {
                request_id: RequestId::Number(1),
                client_caps: ClientCapabilities::default(),
                server_caps: ServerCapabilities::default(),
                protocol_version: ProtocolVersion::LATEST,
                peer: NoOpPeer,
            }
        }

        fn as_context(&self) -> Context<'_> {
            Context::new(
                &self.request_id,
                None,
                &self.client_caps,
                &self.server_caps,
                self.protocol_version,
                &self.peer,
            )
        }
    }

    #[test]
    fn test_echo_with_smol() {
        smol::block_on(async {
            let server = EchoServer::new();
            let test_ctx = TestContext::new();
            let ctx = test_ctx.as_context();

            let args = serde_json::json!({"message": "test"});
            let result = <EchoServer as ToolHandler>::call_tool(&server, "echo", args, &ctx)
                .await
                .unwrap();

            match result {
                ToolOutput::Success(r) => {
                    if let Content::Text(tc) = &r.content[0] {
                        assert!(tc.text.contains("test"));
                    } else {
                        panic!("Expected text content");
                    }
                }
                ToolOutput::RecoverableError { .. } => panic!("Expected success"),
            }
        });
    }

    #[test]
    fn test_concurrent_with_smol() {
        smol::block_on(async {
            let server = EchoServer::new();
            let test_ctx = TestContext::new();
            let ctx = test_ctx.as_context();

            let start = std::time::Instant::now();
            let args = serde_json::json!({});
            let _ = <EchoServer as ToolHandler>::call_tool(&server, "concurrent_demo", args, &ctx)
                .await
                .unwrap();

            // Should complete in about 50ms, not 150ms
            let elapsed = start.elapsed();
            assert!(elapsed < std::time::Duration::from_millis(100));
        });
    }
}
