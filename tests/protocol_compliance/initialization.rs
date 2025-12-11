//! MCP initialization protocol compliance tests.
//!
//! These tests verify that the SDK correctly implements the MCP
//! initialization handshake.

use mcpkit_core::capability::{
    ClientCapabilities, ClientInfo, InitializeRequest, InitializeResult,
    ServerCapabilities, ServerInfo, PROTOCOL_VERSION,
};
use serde_json::json;

#[test]
fn test_protocol_version() {
    assert_eq!(PROTOCOL_VERSION, "2025-11-25");
}

#[test]
fn test_client_info() {
    let info = ClientInfo {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };

    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["name"], "test-client");
    assert_eq!(json["version"], "1.0.0");
}

#[test]
fn test_server_info() {
    let info = ServerInfo::new("test-server", "2.0.0");

    assert_eq!(info.name, "test-server");
    assert_eq!(info.version, "2.0.0");

    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["name"], "test-server");
    assert_eq!(json["version"], "2.0.0");
}

#[test]
fn test_initialize_request() {
    let client_info = ClientInfo {
        name: "my-client".to_string(),
        version: "1.0.0".to_string(),
    };

    let request = InitializeRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: ClientCapabilities::default(),
        client_info,
    };

    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json["capabilities"].is_object());
    assert_eq!(json["clientInfo"]["name"], "my-client");
}

#[test]
fn test_initialize_result() {
    let server_info = ServerInfo::new("my-server", "1.0.0");
    let capabilities = ServerCapabilities::new().with_tools();

    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities,
        server_info,
        instructions: Some("Usage instructions".to_string()),
    };

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json["capabilities"]["tools"].is_object());
    assert_eq!(json["serverInfo"]["name"], "my-server");
    assert_eq!(json["instructions"], "Usage instructions");
}

#[test]
fn test_client_capabilities_default() {
    let caps = ClientCapabilities::default();
    let json = serde_json::to_value(&caps).unwrap();

    // Default capabilities should be an empty or minimal object
    assert!(json.is_object());
}

#[test]
fn test_server_capabilities_tools() {
    let caps = ServerCapabilities::new().with_tools();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["tools"].is_object());
}

#[test]
fn test_server_capabilities_resources() {
    let caps = ServerCapabilities::new().with_resources();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["resources"].is_object());
}

#[test]
fn test_server_capabilities_prompts() {
    let caps = ServerCapabilities::new().with_prompts();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["prompts"].is_object());
}

#[test]
fn test_server_capabilities_tasks() {
    let caps = ServerCapabilities::new().with_tasks();
    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["tasks"].is_object());
}

#[test]
fn test_server_capabilities_all() {
    let caps = ServerCapabilities::new()
        .with_tools()
        .with_resources()
        .with_prompts()
        .with_tasks();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json["tools"].is_object());
    assert!(json["resources"].is_object());
    assert!(json["prompts"].is_object());
    assert!(json["tasks"].is_object());
}

#[test]
fn test_initialize_request_deserialization() {
    let json = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": {
            "name": "test",
            "version": "1.0"
        }
    });

    let request: InitializeRequest = serde_json::from_value(json).unwrap();
    assert_eq!(request.protocol_version, "2025-11-25");
    assert_eq!(request.client_info.name, "test");
}

#[test]
fn test_initialize_result_deserialization() {
    let json = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "test-server",
            "version": "1.0"
        }
    });

    let result: InitializeResult = serde_json::from_value(json).unwrap();
    assert_eq!(result.protocol_version, "2025-11-25");
    assert_eq!(result.server_info.name, "test-server");
}

#[test]
fn test_initialize_result_optional_instructions() {
    // Without instructions
    let json = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "serverInfo": {
            "name": "test",
            "version": "1.0"
        }
    });

    let result: InitializeResult = serde_json::from_value(json).unwrap();
    assert!(result.instructions.is_none());
}
