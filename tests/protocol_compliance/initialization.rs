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
fn test_client_info() -> Result<(), Box<dyn std::error::Error>> {
    let info = ClientInfo {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };

    let json = serde_json::to_value(&info)?;
    assert_eq!(json["name"], "test-client");
    assert_eq!(json["version"], "1.0.0");
    Ok(())
}

#[test]
fn test_server_info() -> Result<(), Box<dyn std::error::Error>> {
    let info = ServerInfo::new("test-server", "2.0.0");

    assert_eq!(info.name, "test-server");
    assert_eq!(info.version, "2.0.0");

    let json = serde_json::to_value(&info)?;
    assert_eq!(json["name"], "test-server");
    assert_eq!(json["version"], "2.0.0");
    Ok(())
}

#[test]
fn test_initialize_request() -> Result<(), Box<dyn std::error::Error>> {
    let client_info = ClientInfo {
        name: "my-client".to_string(),
        version: "1.0.0".to_string(),
    };

    let request = InitializeRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: ClientCapabilities::default(),
        client_info,
    };

    let json = serde_json::to_value(&request)?;
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json["capabilities"].is_object());
    assert_eq!(json["clientInfo"]["name"], "my-client");
    Ok(())
}

#[test]
fn test_initialize_result() -> Result<(), Box<dyn std::error::Error>> {
    let server_info = ServerInfo::new("my-server", "1.0.0");
    let capabilities = ServerCapabilities::new().with_tools();

    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities,
        server_info,
        instructions: Some("Usage instructions".to_string()),
    };

    let json = serde_json::to_value(&result)?;
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json["capabilities"]["tools"].is_object());
    assert_eq!(json["serverInfo"]["name"], "my-server");
    assert_eq!(json["instructions"], "Usage instructions");
    Ok(())
}

#[test]
fn test_client_capabilities_default() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ClientCapabilities::default();
    let json = serde_json::to_value(&caps)?;

    // Default capabilities should be an empty or minimal object
    assert!(json.is_object());
    Ok(())
}

#[test]
fn test_server_capabilities_tools() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ServerCapabilities::new().with_tools();
    let json = serde_json::to_value(&caps)?;

    assert!(json["tools"].is_object());
    Ok(())
}

#[test]
fn test_server_capabilities_resources() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ServerCapabilities::new().with_resources();
    let json = serde_json::to_value(&caps)?;

    assert!(json["resources"].is_object());
    Ok(())
}

#[test]
fn test_server_capabilities_prompts() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ServerCapabilities::new().with_prompts();
    let json = serde_json::to_value(&caps)?;

    assert!(json["prompts"].is_object());
    Ok(())
}

#[test]
fn test_server_capabilities_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ServerCapabilities::new().with_tasks();
    let json = serde_json::to_value(&caps)?;

    assert!(json["tasks"].is_object());
    Ok(())
}

#[test]
fn test_server_capabilities_all() -> Result<(), Box<dyn std::error::Error>> {
    let caps = ServerCapabilities::new()
        .with_tools()
        .with_resources()
        .with_prompts()
        .with_tasks();

    let json = serde_json::to_value(&caps)?;

    assert!(json["tools"].is_object());
    assert!(json["resources"].is_object());
    assert!(json["prompts"].is_object());
    assert!(json["tasks"].is_object());
    Ok(())
}

#[test]
fn test_initialize_request_deserialization() -> Result<(), Box<dyn std::error::Error>> {
    let json = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": {
            "name": "test",
            "version": "1.0"
        }
    });

    let request: InitializeRequest = serde_json::from_value(json)?;
    assert_eq!(request.protocol_version, "2025-11-25");
    assert_eq!(request.client_info.name, "test");
    Ok(())
}

#[test]
fn test_initialize_result_deserialization() -> Result<(), Box<dyn std::error::Error>> {
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

    let result: InitializeResult = serde_json::from_value(json)?;
    assert_eq!(result.protocol_version, "2025-11-25");
    assert_eq!(result.server_info.name, "test-server");
    Ok(())
}

#[test]
fn test_initialize_result_optional_instructions() -> Result<(), Box<dyn std::error::Error>> {
    // Without instructions
    let json = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "serverInfo": {
            "name": "test",
            "version": "1.0"
        }
    });

    let result: InitializeResult = serde_json::from_value(json)?;
    assert!(result.instructions.is_none());
    Ok(())
}

// Protocol version edge case tests
mod protocol_version_tests {
    use mcpkit_core::protocol_version::{ProtocolVersion, VersionParseError};

    #[test]
    fn test_version_parse_error_message() -> Result<(), Box<dyn std::error::Error>> {
        let err = "invalid-version".parse::<ProtocolVersion>().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid-version"));
        assert!(msg.contains("2024-11-05"));
        assert!(msg.contains("2025-11-25"));
        Ok(())
    }

    #[test]
    fn test_version_parse_edge_cases() {
        // Empty string
        assert!("".parse::<ProtocolVersion>().is_err());

        // Version with extra characters
        assert!("2024-11-05-beta".parse::<ProtocolVersion>().is_err());
        assert!(" 2024-11-05".parse::<ProtocolVersion>().is_err());
        assert!("2024-11-05 ".parse::<ProtocolVersion>().is_err());

        // Wrong delimiter
        assert!("2024/11/05".parse::<ProtocolVersion>().is_err());
        assert!("2024.11.05".parse::<ProtocolVersion>().is_err());

        // Valid versions work
        assert!("2024-11-05".parse::<ProtocolVersion>().is_ok());
        assert!("2025-11-25".parse::<ProtocolVersion>().is_ok());
    }

    #[test]
    fn test_version_try_from() {
        // From String
        let result = ProtocolVersion::try_from("2024-11-05".to_string());
        assert!(result.is_ok());

        // From &str
        let result = ProtocolVersion::try_from("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_version_default() {
        let default = ProtocolVersion::default();
        assert_eq!(default, ProtocolVersion::LATEST);
    }

    #[test]
    fn test_negotiate_edge_cases() {
        // Negotiate with subset of versions
        let subset = &[ProtocolVersion::V2025_03_26, ProtocolVersion::V2025_06_18];

        // Client requests oldest - not in subset, return oldest in subset
        let result = ProtocolVersion::negotiate("2024-11-05", subset);
        assert_eq!(result, None);

        // Client requests version in subset
        let result = ProtocolVersion::negotiate("2025-06-18", subset);
        assert_eq!(result, Some(ProtocolVersion::V2025_06_18));

        // Client requests latest - not in subset, return highest in subset
        let result = ProtocolVersion::negotiate("2025-11-25", subset);
        assert_eq!(result, Some(ProtocolVersion::V2025_06_18));

        // Malformed version string - returns latest in subset
        let result = ProtocolVersion::negotiate("not-a-version", subset);
        assert_eq!(result, Some(ProtocolVersion::V2025_06_18));
    }

    #[test]
    fn test_version_all_constant() {
        // ALL should be in chronological order
        let all = ProtocolVersion::ALL;
        assert!(all.len() >= 4);

        for i in 1..all.len() {
            assert!(all[i - 1] < all[i], "Versions should be in ascending order");
        }
    }
}
