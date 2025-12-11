//! MCP initialization protocol compliance tests.
//!
//! These tests verify that the SDK correctly implements the MCP
//! initialization handshake, including protocol version negotiation.

use mcpkit::capability::{
    is_version_supported, negotiate_version, negotiate_version_detailed, ClientCapabilities,
    ClientInfo, InitializeRequest, InitializeResult, ServerCapabilities, ServerInfo,
    VersionNegotiationResult, PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS,
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

#[test]
fn test_client_capabilities_with_sampling() {
    let caps = ClientCapabilities::default().with_sampling();

    assert!(caps.has_sampling());

    let json = serde_json::to_value(&caps).unwrap();
    assert!(json["sampling"].is_object());
}

#[test]
fn test_client_capabilities_with_elicitation() {
    let caps = ClientCapabilities::default().with_elicitation();

    assert!(caps.has_elicitation());

    let json = serde_json::to_value(&caps).unwrap();
    assert!(json["elicitation"].is_object());
}

#[test]
fn test_client_capabilities_with_roots() {
    let caps = ClientCapabilities::default().with_roots();

    assert!(caps.has_roots());

    let json = serde_json::to_value(&caps).unwrap();
    assert!(json["roots"].is_object());
}

// =============================================================================
// Protocol Version Negotiation Tests
// =============================================================================

#[test]
fn test_supported_protocol_versions() {
    // Verify supported versions list is populated
    assert!(!SUPPORTED_PROTOCOL_VERSIONS.is_empty());

    // Verify 2025-11-25 (latest) is supported
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-11-25"));

    // Verify 2024-11-05 (original MCP spec) is supported
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2024-11-05"));
}

#[test]
fn test_is_version_supported_latest() {
    assert!(is_version_supported("2025-11-25"));
}

#[test]
fn test_is_version_supported_original() {
    assert!(is_version_supported("2024-11-05"));
}

#[test]
fn test_is_version_supported_unknown() {
    assert!(!is_version_supported("1.0.0"));
    assert!(!is_version_supported("2024-01-01"));
    assert!(!is_version_supported("unknown"));
    assert!(!is_version_supported(""));
}

#[test]
fn test_negotiate_version_supported() {
    // Requesting a supported version returns that version
    assert_eq!(negotiate_version("2025-11-25"), "2025-11-25");
    assert_eq!(negotiate_version("2024-11-05"), "2024-11-05");
}

#[test]
fn test_negotiate_version_unsupported() {
    // Requesting an unsupported version returns the server's preferred version
    assert_eq!(negotiate_version("1.0.0"), PROTOCOL_VERSION);
    assert_eq!(negotiate_version("unknown"), PROTOCOL_VERSION);
    assert_eq!(negotiate_version(""), PROTOCOL_VERSION);
}

#[test]
fn test_negotiate_version_detailed_accepted() {
    let result = negotiate_version_detailed("2025-11-25");
    assert!(matches!(result, VersionNegotiationResult::Accepted(_)));
    assert!(result.is_exact_match());
    assert_eq!(result.version(), "2025-11-25");

    let result = negotiate_version_detailed("2024-11-05");
    assert!(matches!(result, VersionNegotiationResult::Accepted(_)));
    assert!(result.is_exact_match());
    assert_eq!(result.version(), "2024-11-05");
}

#[test]
fn test_negotiate_version_detailed_counter_offer() {
    let result = negotiate_version_detailed("1.0.0");
    assert!(matches!(result, VersionNegotiationResult::CounterOffer { .. }));
    assert!(!result.is_exact_match());
    assert_eq!(result.version(), PROTOCOL_VERSION);

    if let VersionNegotiationResult::CounterOffer { requested, offered } = result {
        assert_eq!(requested, "1.0.0");
        assert_eq!(offered, PROTOCOL_VERSION);
    }
}

#[test]
fn test_version_negotiation_result_version() {
    let accepted = VersionNegotiationResult::Accepted("2024-11-05".to_string());
    assert_eq!(accepted.version(), "2024-11-05");

    let counter_offer = VersionNegotiationResult::CounterOffer {
        requested: "old".to_string(),
        offered: "new".to_string(),
    };
    assert_eq!(counter_offer.version(), "new");
}

#[test]
fn test_version_negotiation_result_is_exact_match() {
    let accepted = VersionNegotiationResult::Accepted("2025-11-25".to_string());
    assert!(accepted.is_exact_match());

    let counter_offer = VersionNegotiationResult::CounterOffer {
        requested: "1.0.0".to_string(),
        offered: "2025-11-25".to_string(),
    };
    assert!(!counter_offer.is_exact_match());
}

#[test]
fn test_initialize_request_with_protocol_version() {
    // InitializeRequest::new uses PROTOCOL_VERSION by default
    let client_info = ClientInfo::new("test", "1.0");
    let request = InitializeRequest::new(client_info, ClientCapabilities::default());

    assert_eq!(request.protocol_version, PROTOCOL_VERSION);
}

#[test]
fn test_initialize_result_with_negotiated_version() {
    // InitializeResult::new uses PROTOCOL_VERSION by default
    let server_info = ServerInfo::new("test", "1.0");
    let result = InitializeResult::new(server_info, ServerCapabilities::default());

    assert_eq!(result.protocol_version, PROTOCOL_VERSION);
}

#[test]
fn test_rmcp_version_compatibility() {
    // Verify we support the rmcp (original MCP SDK) protocol version
    assert!(is_version_supported("2024-11-05"));

    // Simulating version negotiation with an rmcp server
    let rmcp_version = "2024-11-05";
    let negotiated = negotiate_version(rmcp_version);
    assert_eq!(negotiated, rmcp_version);
}

#[test]
fn test_version_negotiation_spec_compliance() {
    // Per MCP spec: client sends its preferred version
    // Server responds with same version if supported

    // Scenario 1: Client requests latest, server supports it
    let client_request = "2025-11-25";
    let server_response = negotiate_version(client_request);
    assert_eq!(server_response, client_request);

    // Scenario 2: Client requests older version, server supports it
    let client_request = "2024-11-05";
    let server_response = negotiate_version(client_request);
    assert_eq!(server_response, client_request);

    // Scenario 3: Client requests unknown version, server offers its preferred
    let client_request = "2023-01-01";
    let server_response = negotiate_version(client_request);
    assert_eq!(server_response, PROTOCOL_VERSION);
}
