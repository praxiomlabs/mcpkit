//! Integration tests for tool handling.

use mcpkit::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit::protocol::RequestId;
use mcpkit::protocol_version::ProtocolVersion;
use mcpkit::types::tool::CallToolResult;
use mcpkit::types::ToolOutput;
use mcpkit_server::capability::tools::{ToolBuilder, ToolService};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::ToolHandler;

fn make_test_context() -> (
    RequestId,
    ClientCapabilities,
    ServerCapabilities,
    ProtocolVersion,
    NoOpPeer,
) {
    (
        RequestId::Number(1),
        ClientCapabilities::default(),
        ServerCapabilities::default(),
        ProtocolVersion::LATEST,
        NoOpPeer,
    )
}

#[tokio::test]
async fn test_tool_service_basic() {
    // Create a tool service
    let mut service = ToolService::new();

    // Register a simple tool
    let tool = ToolBuilder::new("add")
        .description("Add two numbers")
        .build();

    service.register(tool, |args, _ctx| async move {
        let a = args
            .get("a")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let b = args
            .get("b")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        Ok(ToolOutput::text((a + b).to_string()))
    });

    // Verify the tool is listed
    assert!(service.contains("add"));
    assert_eq!(service.len(), 1);

    let tools = service.list();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "add");
}

#[tokio::test]
async fn test_tool_call() {
    let mut service = ToolService::new();

    let tool = ToolBuilder::new("multiply")
        .description("Multiply two numbers")
        .build();

    service.register(tool, |args, _ctx| async move {
        let a = args
            .get("a")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let b = args
            .get("b")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        Ok(ToolOutput::text((a * b).to_string()))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Call the tool
    let result = service
        .call("multiply", serde_json::json!({"a": 3.0, "b": 4.0}), &ctx)
        .await;
    assert!(result.is_ok());

    let output = result.unwrap();
    // Convert ToolOutput to CallToolResult via From trait
    let call_result: CallToolResult = output.into();
    assert!(!call_result.is_error.unwrap_or(false));
    assert!(!call_result.content.is_empty());
}

#[tokio::test]
async fn test_tool_not_found() {
    let service = ToolService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let result = service
        .call("nonexistent", serde_json::json!({}), &ctx)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_handler_trait() {
    let mut service = ToolService::new();

    let tool = ToolBuilder::new("greet")
        .description("Generate a greeting")
        .build();

    service.register(tool, |args, _ctx| async move {
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
        Ok(ToolOutput::text(format!("Hello, {name}!")))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Use the ToolHandler trait
    let tools = service.list_tools(&ctx).await.unwrap();
    assert_eq!(tools.len(), 1);

    let result = service
        .call_tool("greet", serde_json::json!({"name": "Alice"}), &ctx)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_multiple_tools() {
    let mut service = ToolService::new();

    // Register multiple tools
    for op in ["add", "sub", "mul", "div"] {
        let tool = ToolBuilder::new(op)
            .description(format!("{op} operation"))
            .build();

        service.register(tool, |args, _ctx| async move {
            let _ = args;
            Ok(ToolOutput::text("result".to_string()))
        });
    }

    assert_eq!(service.len(), 4);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let tools = service.list_tools(&ctx).await.unwrap();
    assert_eq!(tools.len(), 4);
}

#[tokio::test]
async fn test_tool_builder_with_schema() {
    let tool = ToolBuilder::new("search")
        .description("Search the database")
        .input_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        }))
        .build();

    assert_eq!(tool.name, "search");
    assert_eq!(tool.description.as_deref(), Some("Search the database"));
    assert!(tool.input_schema["properties"]["query"].is_object());
}

#[tokio::test]
async fn test_tool_output_variants() {
    // Test text output
    let text_output = ToolOutput::text("Hello");
    let result: CallToolResult = text_output.into();
    assert!(!result.is_error.unwrap_or(false));

    // Test error output
    let error_output = ToolOutput::error("Something went wrong");
    let result: CallToolResult = error_output.into();
    assert!(result.is_error.unwrap_or(false));
}

#[tokio::test]
async fn test_tool_output_error_with_suggestion() {
    let output = ToolOutput::error_with_suggestion("Invalid input", "Try using a valid value");
    let result: CallToolResult = output.into();
    assert!(result.is_error.unwrap_or(false));
    // The suggestion should be appended to the content
    assert!(!result.content.is_empty());
}
