//! Integration tests for tool handling.

use mcpkit::ToolInput;
use mcpkit::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit::protocol::RequestId;
use mcpkit::protocol_version::ProtocolVersion;
use mcpkit::types::ToolOutput;
use mcpkit::types::tool::CallToolResult;
use mcpkit_server::capability::tools::{ToolBuilder, ToolService};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::ToolHandler;
use serde::{Deserialize, Serialize};

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

#[tokio::test]
async fn test_tool_builder_annotations() {
    // Test destructive tool
    let tool = ToolBuilder::new("delete_file")
        .description("Delete a file from the filesystem")
        .destructive(true)
        .build();

    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.destructive_hint, Some(true));
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.idempotent_hint, Some(false));

    // Test read-only tool
    let tool = ToolBuilder::new("read_file")
        .description("Read a file")
        .read_only(true)
        .build();

    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.read_only_hint, Some(true));
    assert_eq!(annotations.destructive_hint, Some(false));

    // Test idempotent tool
    let tool = ToolBuilder::new("set_value")
        .description("Set a configuration value")
        .idempotent(true)
        .build();

    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.idempotent_hint, Some(true));

    // Test all annotations combined
    let tool = ToolBuilder::new("complex_tool")
        .description("A complex tool with multiple hints")
        .destructive(false)
        .read_only(true)
        .idempotent(true)
        .build();

    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.destructive_hint, Some(false));
    assert_eq!(annotations.read_only_hint, Some(true));
    assert_eq!(annotations.idempotent_hint, Some(true));
}

#[tokio::test]
async fn test_tool_service_preserves_annotations() {
    let mut service = ToolService::new();

    // Register a tool with annotations
    let tool = ToolBuilder::new("dangerous_operation")
        .description("A potentially dangerous operation")
        .destructive(true)
        .idempotent(false)
        .build();

    service.register(tool, |_args, _ctx| async move {
        Ok(ToolOutput::text("done".to_string()))
    });

    // Verify annotations are preserved in list
    let tools = service.list();
    assert_eq!(tools.len(), 1);
    let tool = &tools[0];
    assert!(tool.annotations.is_some());
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.destructive_hint, Some(true));
}

/// A nested struct for testing schema generation.
#[derive(Debug, Clone, Serialize, Deserialize, ToolInput)]
struct Address {
    /// The street address
    street: String,
    /// The city name
    city: String,
    /// Optional zip code
    zip: Option<String>,
}

/// A parent struct containing a nested struct.
#[derive(Debug, Clone, Serialize, Deserialize, ToolInput)]
struct Person {
    /// The person's name
    name: String,
    /// The person's age
    age: u32,
    /// The person's address
    address: Address,
}

#[test]
fn test_nested_struct_schema_generation() {
    // Generate schema for the nested Address struct
    let address_schema = Address::tool_input_schema();

    // Verify Address schema has proper structure
    assert_eq!(address_schema["type"], "object");
    assert_eq!(address_schema["title"], "Address");

    let addr_props = &address_schema["properties"];
    assert!(addr_props["street"].is_object());
    assert_eq!(addr_props["street"]["type"], "string");
    assert!(addr_props["city"].is_object());
    assert_eq!(addr_props["city"]["type"], "string");
    assert!(addr_props["zip"].is_object());

    // Check required fields (street and city are required, zip is optional)
    let addr_required = address_schema["required"].as_array().unwrap();
    assert!(addr_required.contains(&serde_json::json!("street")));
    assert!(addr_required.contains(&serde_json::json!("city")));
    assert!(!addr_required.contains(&serde_json::json!("zip")));

    // Generate schema for the parent Person struct
    let person_schema = Person::tool_input_schema();

    // Verify Person schema has proper structure
    assert_eq!(person_schema["type"], "object");
    assert_eq!(person_schema["title"], "Person");

    let person_props = &person_schema["properties"];
    assert!(person_props["name"].is_object());
    assert_eq!(person_props["name"]["type"], "string");
    assert!(person_props["age"].is_object());
    assert_eq!(person_props["age"]["type"], "integer");

    // The nested Address field should have full schema, not just "object"
    let nested_addr = &person_props["address"];
    assert!(nested_addr.is_object());
    assert_eq!(nested_addr["type"], "object");
    assert_eq!(nested_addr["title"], "Address");

    // Verify nested schema includes properties from Address
    let nested_props = &nested_addr["properties"];
    assert!(nested_props["street"].is_object());
    assert!(nested_props["city"].is_object());
    assert!(nested_props["zip"].is_object());

    // Check Person required fields
    let person_required = person_schema["required"].as_array().unwrap();
    assert!(person_required.contains(&serde_json::json!("name")));
    assert!(person_required.contains(&serde_json::json!("age")));
    assert!(person_required.contains(&serde_json::json!("address")));
}
