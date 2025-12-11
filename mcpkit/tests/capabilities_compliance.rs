//! MCP capability compliance tests.
//!
//! These tests verify that capability types and negotiation work correctly.

use mcpkit::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit::types::prompt::PromptArgument;
use mcpkit::types::{Prompt, Resource, Tool, ToolAnnotations};
use serde_json::json;

#[test]
fn test_tool_definition() {
    let tool = Tool::new("search")
        .description("Search the database")
        .input_schema(json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        }));

    let json = serde_json::to_value(&tool).unwrap();

    assert_eq!(json["name"], "search");
    assert_eq!(json["description"], "Search the database");
    assert!(json["inputSchema"]["properties"]["query"].is_object());
}

#[test]
fn test_tool_with_annotations() {
    let tool = Tool::new("delete")
        .description("Delete an item")
        .annotations(ToolAnnotations {
            title: Some("Delete Item".to_string()),
            destructive_hint: Some(true),
            idempotent_hint: Some(false),
            read_only_hint: None,
            open_world_hint: None,
        })
        .input_schema(json!({"type": "object"}));

    let json = serde_json::to_value(&tool).unwrap();

    assert!(json["annotations"].is_object());
    assert_eq!(json["annotations"]["title"], "Delete Item");
    assert_eq!(json["annotations"]["destructiveHint"], true);
}

#[test]
fn test_resource_definition() {
    let resource = Resource {
        uri: "file:///config.json".to_string(),
        name: "Configuration".to_string(),
        description: Some("App configuration".to_string()),
        mime_type: Some("application/json".to_string()),
        size: Some(1024),
        annotations: None,
    };

    let json = serde_json::to_value(&resource).unwrap();

    assert_eq!(json["uri"], "file:///config.json");
    assert_eq!(json["name"], "Configuration");
    assert_eq!(json["description"], "App configuration");
    assert_eq!(json["mimeType"], "application/json");
    assert_eq!(json["size"], 1024);
}

#[test]
fn test_prompt_definition() {
    let prompt = Prompt {
        name: "code_review".to_string(),
        description: Some("Review code for issues".to_string()),
        arguments: Some(vec![PromptArgument {
            name: "code".to_string(),
            description: Some("The code to review".to_string()),
            required: Some(true),
        }]),
    };

    let json = serde_json::to_value(&prompt).unwrap();

    assert_eq!(json["name"], "code_review");
    assert_eq!(json["description"], "Review code for issues");
    assert!(json["arguments"].is_array());
    assert_eq!(json["arguments"][0]["name"], "code");
    assert_eq!(json["arguments"][0]["required"], true);
}

#[test]
fn test_client_capabilities_sampling() {
    let caps = ClientCapabilities::default().with_sampling();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["sampling"].is_object());
}

#[test]
fn test_server_capabilities_logging() {
    let caps = ServerCapabilities::new().with_logging();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["logging"].is_object());
}

#[test]
fn test_capability_has_methods() {
    let caps = ServerCapabilities::new().with_tools().with_resources();

    assert!(caps.has_tools());
    assert!(caps.has_resources());
    assert!(!caps.has_prompts());
    assert!(!caps.has_tasks());
}

#[test]
fn test_tool_list_changed_capability() {
    let caps = ServerCapabilities::new().with_tools_and_changes();

    let json = serde_json::to_value(&caps).unwrap();

    // Should have tools with listChanged
    assert!(json["tools"]["listChanged"].as_bool().unwrap_or(false));
}

#[test]
fn test_resources_subscribe_capability() {
    let caps = ServerCapabilities::new().with_resources_and_subscriptions();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["resources"]["subscribe"].as_bool().unwrap_or(false));
    assert!(json["resources"]["listChanged"].as_bool().unwrap_or(false));
}

#[test]
fn test_tool_without_description() {
    let tool = Tool::new("simple").input_schema(json!({"type": "object"}));

    assert!(tool.description.is_none());
}

#[test]
fn test_resource_without_optional_fields() {
    let resource = Resource {
        uri: "test://resource".to_string(),
        name: "Test".to_string(),
        description: None,
        mime_type: None,
        size: None,
        annotations: None,
    };

    let json = serde_json::to_value(&resource).unwrap();

    assert_eq!(json["uri"], "test://resource");
    assert!(json.get("description").is_none() || json["description"].is_null());
}

#[test]
fn test_prompt_without_arguments() {
    let prompt = Prompt {
        name: "simple".to_string(),
        description: Some("A simple prompt".to_string()),
        arguments: None,
    };

    let json = serde_json::to_value(&prompt).unwrap();

    assert_eq!(json["name"], "simple");
    assert!(json.get("arguments").is_none() || json["arguments"].is_null());
}

#[test]
fn test_capabilities_deserialization() {
    let json = json!({
        "tools": {
            "listChanged": true
        },
        "resources": {
            "subscribe": true,
            "listChanged": true
        }
    });

    let caps: ServerCapabilities = serde_json::from_value(json).unwrap();
    assert!(caps.has_tools());
    assert!(caps.has_resources());
}

#[test]
fn test_tool_deserialization() {
    let json = json!({
        "name": "test_tool",
        "description": "A test tool",
        "inputSchema": {
            "type": "object",
            "properties": {
                "param": {"type": "string"}
            }
        }
    });

    let tool: Tool = serde_json::from_value(json).unwrap();
    assert_eq!(tool.name, "test_tool");
    assert_eq!(tool.description.as_deref(), Some("A test tool"));
}

#[test]
fn test_client_capabilities_elicitation() {
    let caps = ClientCapabilities::default().with_elicitation();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["elicitation"].is_object());
}

#[test]
fn test_client_capabilities_roots() {
    let caps = ClientCapabilities::default().with_roots();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["roots"].is_object());
}

#[test]
fn test_client_capabilities_roots_with_changes() {
    let caps = ClientCapabilities::default().with_roots_and_changes();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["roots"]["listChanged"].as_bool().unwrap_or(false));
}

#[test]
fn test_server_capabilities_all() {
    let caps = ServerCapabilities::new()
        .with_tools()
        .with_resources()
        .with_prompts()
        .with_tasks()
        .with_logging()
        .with_completions();

    assert!(caps.has_tools());
    assert!(caps.has_resources());
    assert!(caps.has_prompts());
    assert!(caps.has_tasks());

    let json = serde_json::to_value(&caps).unwrap();
    assert!(json["tools"].is_object());
    assert!(json["resources"].is_object());
    assert!(json["prompts"].is_object());
    assert!(json["tasks"].is_object());
    assert!(json["logging"].is_object());
    assert!(json["completions"].is_object());
}

#[test]
fn test_client_capabilities_has_methods() {
    let caps = ClientCapabilities::default()
        .with_roots()
        .with_sampling();

    assert!(caps.has_roots());
    assert!(caps.has_sampling());
    assert!(!caps.has_elicitation());
}
