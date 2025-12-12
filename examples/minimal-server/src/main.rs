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
//! - Automatic router wiring

use mcpkit::prelude::*;

/// A minimal calculator server with just two tools.
struct Calculator;

#[mcp_server(name = "calculator", version = "1.0.0")]
impl Calculator {
    /// Add two numbers together.
    ///
    /// This is the simplest possible tool - takes two numbers and returns their sum.
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a + b))
    }

    /// Multiply two numbers.
    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a * b))
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Initialize the calculator server
    let calculator = Calculator;

    println!("Calculator MCP server initialized!");
    println!();

    // Demonstrate that the macro-generated code works
    let info = <Calculator as ServerHandler>::server_info(&calculator);
    println!("Server: {} v{}", info.name, info.version);

    let caps = <Calculator as ServerHandler>::capabilities(&calculator);
    println!("Has tools: {}", caps.has_tools());

    // Set up owned data for creating contexts
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit::server::NoOpPeer;

    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;

    // Create context using owned data and references
    let ctx = Context::new(
        &request_id,
        None,  // No progress token
        &client_caps,
        &server_caps,
        &peer,
    );

    let tools = <Calculator as ToolHandler>::list_tools(&calculator, &ctx).await?;
    println!();
    println!("Available tools:");
    for tool in &tools {
        println!("  - {} : {}", tool.name, tool.description.as_deref().unwrap_or(""));
    }

    // Call a tool
    println!();
    println!("Calling add(2, 3)...");
    let args = serde_json::json!({"a": 2.0, "b": 3.0});
    let result = <Calculator as ToolHandler>::call_tool(&calculator, "add", args, &ctx).await?;
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

    println!();
    println!("Calling multiply(4, 5)...");
    let args = serde_json::json!({"a": 4.0, "b": 5.0});
    let result = <Calculator as ToolHandler>::call_tool(&calculator, "multiply", args, &ctx).await?;
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

    Ok(())
}

// Verify the generated implementations compile
#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit::server::{NoOpPeer, Context};

    // Helper struct to hold owned data for tests
    struct TestContext {
        request_id: RequestId,
        client_caps: ClientCapabilities,
        server_caps: ServerCapabilities,
        peer: NoOpPeer,
    }

    impl TestContext {
        fn new() -> Self {
            Self {
                request_id: RequestId::Number(1),
                client_caps: ClientCapabilities::default(),
                server_caps: ServerCapabilities::default(),
                peer: NoOpPeer,
            }
        }

        fn as_context(&self) -> Context<'_> {
            Context::new(
                &self.request_id,
                None,
                &self.client_caps,
                &self.server_caps,
                &self.peer,
            )
        }
    }

    #[tokio::test]
    async fn test_server_info() {
        let info = <Calculator as ServerHandler>::server_info(&Calculator);
        assert_eq!(info.name, "calculator");
        assert_eq!(info.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_capabilities() {
        let caps = <Calculator as ServerHandler>::capabilities(&Calculator);
        assert!(caps.has_tools());
    }

    #[tokio::test]
    async fn test_list_tools() {
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let tools = <Calculator as ToolHandler>::list_tools(&Calculator, &ctx).await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "add");
        assert_eq!(tools[1].name, "multiply");
    }

    #[tokio::test]
    async fn test_call_add() {
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({"a": 2.0, "b": 3.0});
        let result = <Calculator as ToolHandler>::call_tool(&Calculator, "add", args, &ctx)
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
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({"a": 4.0, "b": 5.0});
        let result = <Calculator as ToolHandler>::call_tool(&Calculator, "multiply", args, &ctx)
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
    async fn test_unknown_tool() {
        let test_ctx = TestContext::new();
        let ctx = test_ctx.as_context();
        let args = serde_json::json!({});
        let result = <Calculator as ToolHandler>::call_tool(&Calculator, "unknown", args, &ctx).await;
        assert!(result.is_err());
    }
}
