//! Minimal MCP server example.
//!
//! This example demonstrates the simplest possible MCP server using
//! the unified `#[mcp_server]` macro.
//!
//! ## Running
//!
//! ```bash
//! cargo run -p minimal-server
//! ```
//!
//! ## What This Demonstrates
//!
//! - Single `#[mcp_server]` macro for server definition
//! - Direct parameter extraction from function signatures
//! - Automatic handler registration via `into_server()`
//! - Stateful handlers without requiring `Clone`
//! - The `into_server()` convenience method for easy server creation

use mcpkit::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// A minimal calculator server with tools and internal state.
///
/// Note: No `#[derive(Clone)]` is needed! The `into_server()` method
/// wraps the handler in `Arc` internally.
struct Calculator {
    /// Tracks how many operations have been performed
    operation_count: AtomicU64,
}

impl Calculator {
    fn new() -> Self {
        Self {
            operation_count: AtomicU64::new(0),
        }
    }
}

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together.
    ///
    /// This is the simplest possible tool - takes two numbers and returns their sum.
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        ToolOutput::text(format!("{}", a + b))
    }

    /// Multiply two numbers.
    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        ToolOutput::text(format!("{}", a * b))
    }

    /// Get the number of operations performed.
    #[tool(description = "Get operation count", read_only = true)]
    async fn get_stats(&self) -> ToolOutput {
        let count = self.operation_count.load(Ordering::Relaxed);
        ToolOutput::text(format!("Operations performed: {}", count))
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Create the calculator server with state
    let calculator = Calculator::new();

    println!("Calculator MCP server initialized!");
    println!();

    // Demonstrate that the macro-generated code works
    let info = <Calculator as ServerHandler>::server_info(&calculator);
    println!("Server: {} v{}", info.name, info.version);

    let caps = <Calculator as ServerHandler>::capabilities(&calculator);
    println!("Has tools: {}", caps.has_tools());

    // Set up owned data for creating contexts
    use mcpkit::server::NoOpPeer;
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;

    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;

    // Create context using owned data and references
    let ctx = Context::new(
        &request_id,
        None, // No progress token
        &client_caps,
        &server_caps,
        ProtocolVersion::LATEST,
        &peer,
    );

    let tools = <Calculator as ToolHandler>::list_tools(&calculator, &ctx).await?;
    println!();
    println!("Available tools:");
    for tool in &tools {
        println!(
            "  - {} : {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        );
    }

    // Call tools and observe state changes
    println!();
    println!("Calling add(2, 3)...");
    let args = serde_json::json!({"a": 2.0, "b": 3.0});
    let result = <Calculator as ToolHandler>::call_tool(&calculator, "add", args, &ctx).await?;
    print_result(&result);

    println!();
    println!("Calling multiply(4, 5)...");
    let args = serde_json::json!({"a": 4.0, "b": 5.0});
    let result = <Calculator as ToolHandler>::call_tool(&calculator, "multiply", args, &ctx).await?;
    print_result(&result);

    // Check the operation count (demonstrates stateful behavior)
    println!();
    println!("Calling get_stats()...");
    let args = serde_json::json!({});
    let result = <Calculator as ToolHandler>::call_tool(&calculator, "get_stats", args, &ctx).await?;
    print_result(&result);

    // Demonstrate into_server() - the recommended way to create a server
    println!();
    println!("=== Creating server with into_server() ===");
    println!();
    println!("The recommended way to create a server:");
    println!("  let server = Calculator::new().into_server();");
    println!();
    println!("This automatically:");
    println!("  - Wraps handler in Arc (no Clone required!)");
    println!("  - Registers all #[tool] methods");
    println!("  - Registers all #[resource] methods");
    println!("  - Registers all #[prompt] methods");
    println!();

    // Actually create the server to prove it compiles
    let _server = Calculator::new().into_server();
    println!("Server created successfully!");

    Ok(())
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
            println!("Error: {}", message);
        }
    }
}

// Verify the generated implementations compile
#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit::server::{Context, NoOpPeer};
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;

    // Helper struct to hold owned data for tests
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

    #[tokio::test]
    async fn test_server_info() {
        let calc = Calculator::new();
        let info = <Calculator as ServerHandler>::server_info(&calc);
        assert_eq!(info.name, "calculator");
        assert_eq!(info.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_capabilities() {
        let calc = Calculator::new();
        let caps = <Calculator as ServerHandler>::capabilities(&calc);
        assert!(caps.has_tools());
    }

    #[tokio::test]
    async fn test_list_tools() {
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let tools = <Calculator as ToolHandler>::list_tools(&calc, &ctx)
            .await
            .unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name, "add");
        assert_eq!(tools[1].name, "multiply");
        assert_eq!(tools[2].name, "get_stats");
    }

    #[tokio::test]
    async fn test_call_add() {
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({"a": 2.0, "b": 3.0});
        let result = <Calculator as ToolHandler>::call_tool(&calc, "add", args, &ctx)
            .await
            .unwrap();

        match result {
            ToolOutput::Success(r) => {
                assert!(!r.is_error.unwrap_or(false));
                assert_eq!(r.content.len(), 1);
                if let Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "5");
                } else {
                    panic!("Expected text content");
                }
            }
            ToolOutput::RecoverableError { .. } => panic!("Expected success"),
        }
    }

    #[tokio::test]
    async fn test_call_multiply() {
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({"a": 4.0, "b": 5.0});
        let result = <Calculator as ToolHandler>::call_tool(&calc, "multiply", args, &ctx)
            .await
            .unwrap();

        match result {
            ToolOutput::Success(r) => {
                assert!(!r.is_error.unwrap_or(false));
                if let Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "20");
                } else {
                    panic!("Expected text content");
                }
            }
            ToolOutput::RecoverableError { .. } => panic!("Expected success"),
        }
    }

    #[tokio::test]
    async fn test_stateful_behavior() {
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();

        // Perform some operations
        let args = serde_json::json!({"a": 1.0, "b": 2.0});
        let _ = <Calculator as ToolHandler>::call_tool(&calc, "add", args, &ctx).await;
        let args = serde_json::json!({"a": 3.0, "b": 4.0});
        let _ = <Calculator as ToolHandler>::call_tool(&calc, "multiply", args, &ctx).await;

        // Check operation count
        let args = serde_json::json!({});
        let result = <Calculator as ToolHandler>::call_tool(&calc, "get_stats", args, &ctx)
            .await
            .unwrap();

        match result {
            ToolOutput::Success(r) => {
                if let Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "Operations performed: 2");
                } else {
                    panic!("Expected text content");
                }
            }
            ToolOutput::RecoverableError { .. } => panic!("Expected success"),
        }
    }

    #[tokio::test]
    async fn test_into_server() {
        // Verify that into_server() creates a properly configured server
        // without requiring Clone on the handler
        let _server = Calculator::new().into_server();
        // If this compiles, the test passes!
    }

    #[tokio::test]
    async fn test_tool_annotations() {
        // Verify tool annotations are set correctly via #[tool(read_only = true)]
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let tools = <Calculator as ToolHandler>::list_tools(&calc, &ctx)
            .await
            .unwrap();

        // Find the get_stats tool and verify its annotations
        let get_stats = tools.iter().find(|t| t.name == "get_stats").unwrap();
        let annotations = get_stats.annotations.as_ref().unwrap();

        // get_stats has read_only = true
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(false));

        // add has no special annotations (all false by default)
        let add = tools.iter().find(|t| t.name == "add").unwrap();
        let add_annotations = add.annotations.as_ref().unwrap();
        assert_eq!(add_annotations.read_only_hint, Some(false));
        assert_eq!(add_annotations.destructive_hint, Some(false));
        assert_eq!(add_annotations.idempotent_hint, Some(false));
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        let calc = Calculator::new();
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({});
        let result =
            <Calculator as ToolHandler>::call_tool(&calc, "unknown", args, &ctx).await;
        assert!(result.is_err());
    }
}
