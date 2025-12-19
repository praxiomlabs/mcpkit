//! Tool schema compatibility tests.
//!
//! These tests verify that the SDK's tool schemas are compatible with
//! the MCP specification and rmcp (the reference Rust implementation).
//!
//! The tests ensure:
//! 1. Tool definitions serialize to the correct JSON format
//! 2. Field names use correct camelCase (inputSchema, isError, etc.)
//! 3. `CallToolResult` format matches the specification
//! 4. `ListToolsResult` format is compatible
//! 5. Tool annotations serialize correctly

use mcpkit::types::Content;
use mcpkit::types::tool::{
    CallToolRequest, CallToolResult, ListToolsRequest, ListToolsResult, Tool, ToolAnnotations,
    ToolOutput,
};
use serde_json::json;

// =============================================================================
// Tool Definition Schema Tests
// =============================================================================

#[test]
fn test_tool_basic_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("get_weather")
        .description("Get current weather for a location")
        .input_schema(json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City name or zip code"
                }
            },
            "required": ["location"]
        }));

    let json = serde_json::to_value(&tool)?;

    // Verify required fields exist
    assert!(json.get("name").is_some(), "Tool must have 'name' field");
    assert!(
        json.get("inputSchema").is_some(),
        "Tool must have 'inputSchema' field (camelCase)"
    );

    // Verify field values
    assert_eq!(json["name"], "get_weather");
    assert_eq!(json["description"], "Get current weather for a location");

    // Verify inputSchema is properly nested
    assert_eq!(json["inputSchema"]["type"], "object");
    assert!(json["inputSchema"]["properties"]["location"].is_object());
    assert!(json["inputSchema"]["required"].is_array());
    Ok(())
}

#[test]
fn test_tool_field_names_are_camel_case() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("test")
        .description("Test tool")
        .input_schema(json!({"type": "object"}))
        .annotations(ToolAnnotations {
            title: Some("Test Title".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        });

    let json = serde_json::to_value(&tool)?;
    let json_str = serde_json::to_string(&tool)?;

    // Verify camelCase field names
    assert!(
        json_str.contains("\"inputSchema\""),
        "inputSchema must be camelCase"
    );
    assert!(
        json_str.contains("\"readOnlyHint\""),
        "readOnlyHint must be camelCase"
    );
    assert!(
        json_str.contains("\"destructiveHint\""),
        "destructiveHint must be camelCase"
    );
    assert!(
        json_str.contains("\"idempotentHint\""),
        "idempotentHint must be camelCase"
    );
    assert!(
        json_str.contains("\"openWorldHint\""),
        "openWorldHint must be camelCase"
    );

    // Verify annotations object structure
    let annotations = json.get("annotations").ok_or("annotations must exist")?;
    assert_eq!(annotations["title"], "Test Title");
    assert_eq!(annotations["readOnlyHint"], true);
    assert_eq!(annotations["destructiveHint"], false);
    assert_eq!(annotations["idempotentHint"], true);
    assert_eq!(annotations["openWorldHint"], false);
    Ok(())
}

#[test]
fn test_tool_optional_fields_skip_when_none() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("minimal").input_schema(json!({"type": "object"}));

    let json = serde_json::to_value(&tool)?;
    let json_str = serde_json::to_string(&tool)?;

    // Required fields present
    assert!(json.get("name").is_some());
    assert!(json.get("inputSchema").is_some());

    // Optional fields should be absent when None
    assert!(
        !json_str.contains("\"description\""),
        "description should be skipped when None"
    );
    assert!(
        !json_str.contains("\"annotations\""),
        "annotations should be skipped when None"
    );
    Ok(())
}

#[test]
fn test_tool_with_complex_input_schema() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("create_user")
        .description("Create a new user account")
        .input_schema(json!({
            "type": "object",
            "properties": {
                "username": {
                    "type": "string",
                    "minLength": 3,
                    "maxLength": 50,
                    "pattern": "^[a-zA-Z0-9_]+$"
                },
                "email": {
                    "type": "string",
                    "format": "email"
                },
                "age": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 150
                },
                "roles": {
                    "type": "array",
                    "items": { "type": "string" },
                    "uniqueItems": true
                },
                "metadata": {
                    "type": "object",
                    "additionalProperties": true
                }
            },
            "required": ["username", "email"],
            "additionalProperties": false
        }));

    let json = serde_json::to_value(&tool)?;

    // Verify complex schema is preserved exactly
    let schema = &json["inputSchema"];
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["username"]["minLength"], 3);
    assert_eq!(schema["properties"]["email"]["format"], "email");
    assert_eq!(schema["properties"]["age"]["minimum"], 0);
    assert_eq!(schema["properties"]["roles"]["uniqueItems"], true);
    assert_eq!(schema["additionalProperties"], false);
    Ok(())
}

// =============================================================================
// CallToolResult Schema Tests
// =============================================================================

#[test]
fn test_call_tool_result_text_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let result = CallToolResult::text("Weather in New York: 72°F, Partly cloudy");

    let json = serde_json::to_value(&result)?;
    let json_str = serde_json::to_string(&result)?;

    // Verify structure
    assert!(json.get("content").is_some(), "Must have 'content' field");
    assert!(json["content"].is_array(), "content must be an array");

    // Verify content item
    let content = &json["content"][0];
    assert_eq!(content["type"], "text");
    assert!(content.get("text").is_some());

    // isError should be absent for successful results (skip_serializing_if)
    assert!(
        !json_str.contains("\"isError\""),
        "isError should be skipped when None"
    );
    Ok(())
}

#[test]
fn test_call_tool_result_error_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let result = CallToolResult::error("Failed to fetch weather data");

    let json = serde_json::to_value(&result)?;
    let json_str = serde_json::to_string(&result)?;

    // Verify isError is present and true
    assert!(
        json_str.contains("\"isError\""),
        "isError must be camelCase"
    );
    assert_eq!(json["isError"], true);

    // Verify content contains error message
    assert!(
        !json["content"]
            .as_array()
            .ok_or("Expected array")?
            .is_empty()
    );
    Ok(())
}

#[test]
fn test_call_tool_result_is_error_field_casing() -> Result<(), Box<dyn std::error::Error>> {
    let error_result = CallToolResult {
        content: vec![Content::text("Error message")],
        is_error: Some(true),
    };

    let json_str = serde_json::to_string(&error_result)?;

    // Must be camelCase for MCP compatibility
    assert!(json_str.contains("\"isError\":true"));
    assert!(
        !json_str.contains("is_error"),
        "is_error (snake_case) must not appear"
    );
    Ok(())
}

#[test]
fn test_call_tool_result_multiple_content() -> Result<(), Box<dyn std::error::Error>> {
    let result = CallToolResult::content(vec![
        Content::text("First result"),
        Content::text("Second result"),
    ]);

    let json = serde_json::to_value(&result)?;

    let content = json["content"].as_array().ok_or("Expected array")?;
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "text");
    Ok(())
}

// =============================================================================
// ToolOutput Conversion Tests
// =============================================================================

#[test]
fn test_tool_output_to_call_tool_result() -> Result<(), Box<dyn std::error::Error>> {
    // Verify ToolOutput converts correctly to CallToolResult
    let output = ToolOutput::text("Success");
    let result: CallToolResult = output.into();

    let json = serde_json::to_value(&result)?;
    assert!(json["content"].is_array());
    assert!(!result.is_error());
    Ok(())
}

#[test]
fn test_tool_output_error_to_call_tool_result() -> Result<(), Box<dyn std::error::Error>> {
    let output = ToolOutput::error("Something went wrong");
    let result: CallToolResult = output.into();

    assert!(result.is_error());

    let json = serde_json::to_value(&result)?;
    assert_eq!(json["isError"], true);
    Ok(())
}

#[test]
fn test_tool_output_error_with_suggestion() -> Result<(), Box<dyn std::error::Error>> {
    let output = ToolOutput::error_with_suggestion(
        "Invalid parameter",
        "Use a valid location name like 'New York'",
    );
    let result: CallToolResult = output.into();

    let json = serde_json::to_value(&result)?;
    assert_eq!(json["isError"], true);

    // Suggestion should be included in the content
    let text = json["content"][0]["text"]
        .as_str()
        .ok_or("Expected string")?;
    assert!(text.contains("Invalid parameter"));
    assert!(text.contains("Suggestion"));
    Ok(())
}

// =============================================================================
// ListToolsResult Schema Tests
// =============================================================================

#[test]
fn test_list_tools_result_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let result = ListToolsResult {
        tools: vec![
            Tool::new("tool_a")
                .description("Tool A")
                .input_schema(json!({"type": "object"})),
            Tool::new("tool_b")
                .description("Tool B")
                .input_schema(json!({"type": "object"})),
        ],
        next_cursor: Some("next-page-token".to_string()),
    };

    let json = serde_json::to_value(&result)?;
    let json_str = serde_json::to_string(&result)?;

    // Verify structure
    assert!(json.get("tools").is_some());
    assert!(json["tools"].is_array());
    assert_eq!(json["tools"].as_array().ok_or("Expected array")?.len(), 2);

    // Verify nextCursor is camelCase
    assert!(
        json_str.contains("\"nextCursor\""),
        "nextCursor must be camelCase"
    );
    assert_eq!(json["nextCursor"], "next-page-token");
    Ok(())
}

#[test]
fn test_list_tools_result_without_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let result = ListToolsResult {
        tools: vec![Tool::new("tool").input_schema(json!({"type": "object"}))],
        next_cursor: None,
    };

    let json_str = serde_json::to_string(&result)?;

    // nextCursor should be absent when None
    assert!(
        !json_str.contains("nextCursor"),
        "nextCursor should be skipped when None"
    );
    Ok(())
}

// =============================================================================
// ListToolsRequest Schema Tests
// =============================================================================

#[test]
fn test_list_tools_request_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let request = ListToolsRequest {
        cursor: Some("page-token".to_string()),
    };

    let json = serde_json::to_value(&request)?;

    assert_eq!(json["cursor"], "page-token");
    Ok(())
}

#[test]
fn test_list_tools_request_without_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let request = ListToolsRequest::default();

    let json_str = serde_json::to_string(&request)?;

    // Should be an empty object or have no cursor
    assert!(
        !json_str.contains("cursor") || json_str == "{}",
        "cursor should be skipped when None"
    );
    Ok(())
}

// =============================================================================
// CallToolRequest Schema Tests
// =============================================================================

#[test]
fn test_call_tool_request_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let request = CallToolRequest {
        name: "get_weather".to_string(),
        arguments: Some(json!({"location": "New York"})),
    };

    let json = serde_json::to_value(&request)?;

    assert_eq!(json["name"], "get_weather");
    assert_eq!(json["arguments"]["location"], "New York");
    Ok(())
}

#[test]
fn test_call_tool_request_without_arguments() -> Result<(), Box<dyn std::error::Error>> {
    let request = CallToolRequest {
        name: "ping".to_string(),
        arguments: None,
    };

    let json_str = serde_json::to_string(&request)?;

    // arguments should be skipped when None
    assert!(
        !json_str.contains("arguments") || json_str.contains("\"arguments\":null"),
        "arguments should be skipped or null when None"
    );
    Ok(())
}

// =============================================================================
// Deserialization Compatibility Tests (rmcp format)
// =============================================================================

#[test]
fn test_tool_deserialization_from_rmcp_format() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate a tool definition as it would come from rmcp
    let rmcp_tool_json = json!({
        "name": "calculate",
        "description": "Perform a calculation",
        "inputSchema": {
            "type": "object",
            "properties": {
                "operation": { "type": "string" },
                "a": { "type": "number" },
                "b": { "type": "number" }
            },
            "required": ["operation", "a", "b"]
        }
    });

    let tool: Tool = serde_json::from_value(rmcp_tool_json)?;

    assert_eq!(tool.name, "calculate");
    assert_eq!(tool.description.as_deref(), Some("Perform a calculation"));
    assert_eq!(tool.input_schema["type"], "object");
    Ok(())
}

#[test]
fn test_tool_deserialization_with_annotations() -> Result<(), Box<dyn std::error::Error>> {
    let json_with_annotations = json!({
        "name": "delete_file",
        "description": "Delete a file from the filesystem",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        },
        "annotations": {
            "title": "File Deletion Tool",
            "readOnlyHint": false,
            "destructiveHint": true,
            "idempotentHint": true,
            "openWorldHint": false
        }
    });

    let tool: Tool = serde_json::from_value(json_with_annotations)?;

    assert_eq!(tool.name, "delete_file");
    let annotations = tool.annotations.ok_or("Expected annotations")?;
    assert_eq!(annotations.title, Some("File Deletion Tool".to_string()));
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.destructive_hint, Some(true));
    assert_eq!(annotations.idempotent_hint, Some(true));
    assert_eq!(annotations.open_world_hint, Some(false));
    Ok(())
}

#[test]
fn test_call_tool_result_deserialization_from_rmcp_format() -> Result<(), Box<dyn std::error::Error>>
{
    // Simulate a CallToolResult as it would come from rmcp
    let rmcp_result_json = json!({
        "content": [
            {
                "type": "text",
                "text": "Result: 42"
            }
        ],
        "isError": false
    });

    let result: CallToolResult = serde_json::from_value(rmcp_result_json)?;

    assert!(!result.is_error());
    assert_eq!(result.content.len(), 1);
    Ok(())
}

#[test]
fn test_call_tool_result_error_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let error_json = json!({
        "content": [
            {
                "type": "text",
                "text": "Error: Division by zero"
            }
        ],
        "isError": true
    });

    let result: CallToolResult = serde_json::from_value(error_json)?;

    assert!(result.is_error());
    Ok(())
}

#[test]
fn test_list_tools_result_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let json = json!({
        "tools": [
            {
                "name": "tool1",
                "inputSchema": { "type": "object" }
            },
            {
                "name": "tool2",
                "description": "Second tool",
                "inputSchema": { "type": "object" }
            }
        ],
        "nextCursor": "page2"
    });

    let result: ListToolsResult = serde_json::from_value(json)?;

    assert_eq!(result.tools.len(), 2);
    assert_eq!(result.tools[0].name, "tool1");
    assert_eq!(result.tools[1].name, "tool2");
    assert_eq!(result.next_cursor, Some("page2".to_string()));
    Ok(())
}

// =============================================================================
// Full Request/Response Cycle Tests
// =============================================================================

#[test]
fn test_tools_list_response_matches_mcp_spec() -> Result<(), Box<dyn std::error::Error>> {
    // Create a full tools/list response as per MCP spec
    let tools = vec![
        Tool::new("get_weather")
            .description("Get current weather information for a location")
            .input_schema(json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "City name or zip code"
                    }
                },
                "required": ["location"]
            })),
        Tool::new("search_database")
            .description("Search the database")
            .input_schema(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }))
            .annotations(ToolAnnotations::read_only()),
    ];

    let response = ListToolsResult {
        tools,
        next_cursor: None,
    };

    let json = serde_json::to_value(&response)?;

    // Verify MCP-compliant structure
    assert!(json["tools"].is_array());
    assert_eq!(json["tools"][0]["name"], "get_weather");
    assert!(json["tools"][0]["inputSchema"]["properties"]["location"].is_object());
    assert_eq!(json["tools"][1]["annotations"]["readOnlyHint"], true);
    Ok(())
}

#[test]
fn test_tools_call_response_matches_mcp_spec() -> Result<(), Box<dyn std::error::Error>> {
    // Create a tools/call response as per MCP spec
    let result = CallToolResult::text(
        "Current weather in New York:\nTemperature: 72°F\nConditions: Partly cloudy",
    );

    let json = serde_json::to_value(&result)?;

    // Verify MCP-compliant structure
    assert!(json["content"].is_array());
    assert_eq!(json["content"][0]["type"], "text");
    assert!(
        json["content"][0]["text"]
            .as_str()
            .ok_or("Expected string")?
            .contains("72°F")
    );
    Ok(())
}

// =============================================================================
// Edge Cases and Boundary Tests
// =============================================================================

#[test]
fn test_empty_input_schema() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("no_params").input_schema(json!({
        "type": "object",
        "properties": {}
    }));

    let json = serde_json::to_value(&tool)?;

    assert_eq!(json["inputSchema"]["type"], "object");
    assert!(
        json["inputSchema"]["properties"]
            .as_object()
            .ok_or("Expected object")?
            .is_empty()
    );
    Ok(())
}

#[test]
fn test_tool_with_special_characters_in_name() -> Result<(), Box<dyn std::error::Error>> {
    // Some tools might have underscores or hyphens
    let tool = Tool::new("my_special-tool.v2")
        .description("Tool with special chars")
        .input_schema(json!({"type": "object"}));

    let json = serde_json::to_value(&tool)?;

    assert_eq!(json["name"], "my_special-tool.v2");
    Ok(())
}

#[test]
fn test_tool_with_unicode_description() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("unicode_tool")
        .description("工具描述 - Tool description with émojis 🔧")
        .input_schema(json!({"type": "object"}));

    let json = serde_json::to_value(&tool)?;

    assert!(
        json["description"]
            .as_str()
            .ok_or("Expected string")?
            .contains("工具描述")
    );
    assert!(
        json["description"]
            .as_str()
            .ok_or("Expected string")?
            .contains("🔧")
    );
    Ok(())
}

#[test]
fn test_round_trip_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let original_tool = Tool::new("round_trip")
        .description("Test round-trip serialization")
        .input_schema(json!({
            "type": "object",
            "properties": {
                "param": { "type": "string" }
            }
        }))
        .annotations(ToolAnnotations::read_only());

    // Serialize
    let json_str = serde_json::to_string(&original_tool)?;

    // Deserialize
    let deserialized: Tool = serde_json::from_str(&json_str)?;

    // Verify equality
    assert_eq!(original_tool.name, deserialized.name);
    assert_eq!(original_tool.description, deserialized.description);
    assert_eq!(original_tool.input_schema, deserialized.input_schema);
    assert_eq!(
        original_tool
            .annotations
            .as_ref()
            .ok_or("Expected annotations")?
            .read_only_hint,
        deserialized
            .annotations
            .as_ref()
            .ok_or("Expected annotations")?
            .read_only_hint
    );
    Ok(())
}

// =============================================================================
// Builder API Tests (ensuring builders produce compliant schemas)
// =============================================================================

#[test]
fn test_with_string_param_produces_valid_schema() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("test")
        .with_string_param("query", "Search query", true)
        .with_string_param("filter", "Optional filter", false);

    let json = serde_json::to_value(&tool)?;
    let schema = &json["inputSchema"];

    assert_eq!(schema["properties"]["query"]["type"], "string");
    assert_eq!(schema["properties"]["filter"]["type"], "string");
    assert!(
        schema["required"]
            .as_array()
            .ok_or("Expected array")?
            .contains(&json!("query"))
    );
    assert!(
        !schema["required"]
            .as_array()
            .ok_or("Expected array")?
            .contains(&json!("filter"))
    );
    Ok(())
}

#[test]
fn test_with_number_param_produces_valid_schema() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("calc")
        .with_number_param("x", "First number", true)
        .with_number_param("y", "Second number", true);

    let json = serde_json::to_value(&tool)?;
    let schema = &json["inputSchema"];

    assert_eq!(schema["properties"]["x"]["type"], "number");
    assert_eq!(schema["properties"]["y"]["type"], "number");
    Ok(())
}

#[test]
fn test_with_boolean_param_produces_valid_schema() -> Result<(), Box<dyn std::error::Error>> {
    let tool = Tool::new("config")
        .with_boolean_param("enabled", "Enable feature", false)
        .with_boolean_param("verbose", "Verbose output", false);

    let json = serde_json::to_value(&tool)?;
    let schema = &json["inputSchema"];

    assert_eq!(schema["properties"]["enabled"]["type"], "boolean");
    assert_eq!(schema["properties"]["verbose"]["type"], "boolean");
    Ok(())
}
