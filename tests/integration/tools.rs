//! Integration tests for tool handling.

use mcpkit_core::types::{Tool, ToolOutput};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::capability::tools::{ToolBuilder, ToolService};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::ToolHandler;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;

fn make_test_context() -> (RequestId, ClientCapabilities, ServerCapabilities, ProtocolVersion, NoOpPeer) {
    (
        RequestId::Number(1),
        ClientCapabilities::default(),
        ServerCapabilities::default(),
        ProtocolVersion::LATEST,
        NoOpPeer,
    )
}

#[tokio::test]
async fn test_tool_service_basic() -> Result<(), Box<dyn std::error::Error>> {
    // Create a tool service
    let mut service = ToolService::new();

    // Register a simple tool
    let tool = ToolBuilder::new("add")
        .description("Add two numbers")
        .build();

    service.register(tool, |args, _ctx| async move {
        let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok(ToolOutput::text((a + b).to_string()))
    });

    // Verify the tool is listed
    assert!(service.contains("add"));
    assert_eq!(service.len(), 1);

    let tools = service.list();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "add");
    Ok(())
}

#[tokio::test]
async fn test_tool_call() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ToolService::new();

    let tool = ToolBuilder::new("multiply")
        .description("Multiply two numbers")
        .build();

    service.register(tool, |args, _ctx| async move {
        let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok(ToolOutput::text((a * b).to_string()))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // Call the tool
    let result = service.call("multiply", serde_json::json!({"a": 3.0, "b": 4.0}), &ctx).await;
    assert!(result.is_ok());

    let output = result?;
    let call_result = output.into_call_result();
    assert!(!call_result.is_error.unwrap_or(false));
    assert!(!call_result.content.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_tool_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let service = ToolService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.call("nonexistent", serde_json::json!({}), &ctx).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn test_tool_handler_trait() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ToolService::new();

    let tool = ToolBuilder::new("greet")
        .description("Generate a greeting")
        .build();

    service.register(tool, |args, _ctx| async move {
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
        Ok(ToolOutput::text(format!("Hello, {}!", name)))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // Use the ToolHandler trait
    let tools = service.list_tools(&ctx).await?;
    assert_eq!(tools.len(), 1);

    let result = service.call_tool("greet", serde_json::json!({"name": "Alice"}), &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_multiple_tools() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ToolService::new();

    // Register multiple tools
    for op in ["add", "sub", "mul", "div"] {
        let tool = ToolBuilder::new(op)
            .description(format!("{} operation", op))
            .build();

        service.register(tool, |args, _ctx| async move {
            let _ = args;
            Ok(ToolOutput::text("result".to_string()))
        });
    }

    assert_eq!(service.len(), 4);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let tools = service.list_tools(&ctx).await?;
    assert_eq!(tools.len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_tool_builder_annotations() -> Result<(), Box<dyn std::error::Error>> {
    let tool = ToolBuilder::new("delete_file")
        .description("Delete a file")
        .destructive(true)
        .build();

    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.ok_or("Annotations should be present")?;
    assert_eq!(annotations.destructive_hint, Some(true));
    Ok(())
}

#[tokio::test]
async fn test_tool_output_variants() -> Result<(), Box<dyn std::error::Error>> {
    // Test text output
    let text_output = ToolOutput::text("Hello");
    let result = text_output.into_call_result();
    assert!(!result.is_error.unwrap_or(false));

    // Test error output
    let error_output = ToolOutput::error("Something went wrong");
    let result = error_output.into_call_result();
    assert!(result.is_error.unwrap_or(false));
    Ok(())
}
