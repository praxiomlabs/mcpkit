//! Capability negotiation interoperability tests.
//!
//! These tests verify that the SDK's capability negotiation is compatible
//! with the MCP specification and rmcp (the reference Rust implementation).
//!
//! The tests ensure:
//! 1. Server and client capabilities serialize to the correct JSON format
//! 2. Field names use correct camelCase (listChanged, protocolVersion, etc.)
//! 3. InitializeRequest/InitializeResult format matches the specification
//! 4. Capability structures can be deserialized from rmcp format
//! 5. Optional fields are correctly skipped when None

use mcpkit::capability::{
    ClientCapabilities, ClientInfo, CompletionCapability, ElicitationCapability, InitializeRequest,
    InitializeResult, LoggingCapability, PROTOCOL_VERSION, PromptCapability, ResourceCapability,
    RootsCapability, SUPPORTED_PROTOCOL_VERSIONS, SamplingCapability, ServerCapabilities,
    ServerInfo, TaskCapability, ToolCapability, VersionNegotiationResult, is_version_supported,
    negotiate_version, negotiate_version_detailed,
};
use serde_json::json;

// =============================================================================
// Server Capabilities Schema Tests
// =============================================================================

#[test]
fn test_server_capabilities_empty_serialization() {
    let caps = ServerCapabilities::default();

    let json = serde_json::to_value(&caps).unwrap();
    let json_str = serde_json::to_string(&caps).unwrap();

    // Empty capabilities should serialize to empty object
    assert!(json.as_object().unwrap().is_empty());
    assert_eq!(json_str, "{}");
}

#[test]
fn test_server_capabilities_tools_serialization() {
    let caps = ServerCapabilities::new().with_tools();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("tools").is_some());
    assert!(json["tools"].is_object());
}

#[test]
fn test_server_capabilities_tools_with_list_changed() {
    let caps = ServerCapabilities::new().with_tools_and_changes();

    let json = serde_json::to_value(&caps).unwrap();
    let json_str = serde_json::to_string(&caps).unwrap();

    // Verify listChanged is camelCase
    assert!(json_str.contains("\"listChanged\":true"));
    assert_eq!(json["tools"]["listChanged"], true);
}

#[test]
fn test_server_capabilities_resources_serialization() {
    let caps = ServerCapabilities::new().with_resources();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("resources").is_some());
}

#[test]
fn test_server_capabilities_resources_with_subscriptions() {
    let caps = ServerCapabilities::new().with_resources_and_subscriptions();

    let json = serde_json::to_value(&caps).unwrap();
    let json_str = serde_json::to_string(&caps).unwrap();

    // Verify both subscribe and listChanged
    assert!(json_str.contains("\"subscribe\":true"));
    assert!(json_str.contains("\"listChanged\":true"));
    assert_eq!(json["resources"]["subscribe"], true);
    assert_eq!(json["resources"]["listChanged"], true);
}

#[test]
fn test_server_capabilities_prompts_serialization() {
    let caps = ServerCapabilities::new().with_prompts();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("prompts").is_some());
}

#[test]
fn test_server_capabilities_tasks_serialization() {
    let caps = ServerCapabilities::new().with_tasks();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("tasks").is_some());
}

#[test]
fn test_server_capabilities_logging_serialization() {
    let caps = ServerCapabilities::new().with_logging();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("logging").is_some());
}

#[test]
fn test_server_capabilities_completions_serialization() {
    let caps = ServerCapabilities::new().with_completions();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("completions").is_some());
}

#[test]
fn test_server_capabilities_full_serialization() {
    let caps = ServerCapabilities::new()
        .with_tools_and_changes()
        .with_resources_and_subscriptions()
        .with_prompts()
        .with_tasks()
        .with_logging()
        .with_completions();

    let json = serde_json::to_value(&caps).unwrap();

    // All capabilities should be present
    assert!(json.get("tools").is_some());
    assert!(json.get("resources").is_some());
    assert!(json.get("prompts").is_some());
    assert!(json.get("tasks").is_some());
    assert!(json.get("logging").is_some());
    assert!(json.get("completions").is_some());
}

#[test]
fn test_server_capabilities_with_experimental() {
    let mut caps = ServerCapabilities::new().with_tools();
    caps.experimental = Some(json!({
        "customFeature": true,
        "featureConfig": {
            "maxItems": 100
        }
    }));

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("experimental").is_some());
    assert_eq!(json["experimental"]["customFeature"], true);
}

// =============================================================================
// Client Capabilities Schema Tests
// =============================================================================

#[test]
fn test_client_capabilities_empty_serialization() {
    let caps = ClientCapabilities::default();

    let json = serde_json::to_value(&caps).unwrap();
    let json_str = serde_json::to_string(&caps).unwrap();

    assert!(json.as_object().unwrap().is_empty());
    assert_eq!(json_str, "{}");
}

#[test]
fn test_client_capabilities_roots_serialization() {
    let caps = ClientCapabilities::new().with_roots();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("roots").is_some());
}

#[test]
fn test_client_capabilities_roots_with_list_changed() {
    let caps = ClientCapabilities::new().with_roots_and_changes();

    let json = serde_json::to_value(&caps).unwrap();
    let json_str = serde_json::to_string(&caps).unwrap();

    // Verify listChanged is camelCase
    assert!(json_str.contains("\"listChanged\":true"));
    assert_eq!(json["roots"]["listChanged"], true);
}

#[test]
fn test_client_capabilities_sampling_serialization() {
    let caps = ClientCapabilities::new().with_sampling();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("sampling").is_some());
}

#[test]
fn test_client_capabilities_elicitation_serialization() {
    let caps = ClientCapabilities::new().with_elicitation();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("elicitation").is_some());
}

#[test]
fn test_client_capabilities_full_serialization() {
    let caps = ClientCapabilities::new()
        .with_roots_and_changes()
        .with_sampling()
        .with_elicitation();

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("roots").is_some());
    assert!(json.get("sampling").is_some());
    assert!(json.get("elicitation").is_some());
}

#[test]
fn test_client_capabilities_with_experimental() {
    let mut caps = ClientCapabilities::new().with_sampling();
    caps.experimental = Some(json!({
        "betaFeature": "enabled"
    }));

    let json = serde_json::to_value(&caps).unwrap();

    assert!(json.get("experimental").is_some());
    assert_eq!(json["experimental"]["betaFeature"], "enabled");
}

// =============================================================================
// InitializeRequest Schema Tests
// =============================================================================

#[test]
fn test_initialize_request_serialization() {
    let client_info = ClientInfo::new("test-client", "1.0.0");
    let caps = ClientCapabilities::new().with_sampling();
    let request = InitializeRequest::new(client_info, caps);

    let json = serde_json::to_value(&request).unwrap();
    let json_str = serde_json::to_string(&request).unwrap();

    // Verify camelCase field names
    assert!(json_str.contains("\"protocolVersion\""));
    assert!(json_str.contains("\"clientInfo\""));

    // Verify structure
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json.get("capabilities").is_some());
    assert_eq!(json["clientInfo"]["name"], "test-client");
    assert_eq!(json["clientInfo"]["version"], "1.0.0");
}

#[test]
fn test_initialize_request_deserialization_from_rmcp_format() {
    // Simulate an InitializeRequest as it would come from rmcp
    let rmcp_request = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "roots": {
                "listChanged": true
            },
            "sampling": {}
        },
        "clientInfo": {
            "name": "rmcp-client",
            "version": "0.1.0"
        }
    });

    let request: InitializeRequest = serde_json::from_value(rmcp_request).unwrap();

    assert_eq!(request.protocol_version, "2024-11-05");
    assert_eq!(request.client_info.name, "rmcp-client");
    assert!(request.capabilities.has_roots());
    assert!(request.capabilities.has_sampling());
}

#[test]
fn test_initialize_request_minimal() {
    // Minimal valid initialize request
    let minimal = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "clientInfo": {
            "name": "minimal-client",
            "version": "1.0"
        }
    });

    let request: InitializeRequest = serde_json::from_value(minimal).unwrap();

    assert_eq!(request.protocol_version, "2025-11-25");
    assert!(!request.capabilities.has_roots());
    assert!(!request.capabilities.has_sampling());
}

// =============================================================================
// InitializeResult Schema Tests
// =============================================================================

#[test]
fn test_initialize_result_serialization() {
    let server_info = ServerInfo::new("test-server", "2.0.0");
    let caps = ServerCapabilities::new().with_tools().with_resources();
    let result = InitializeResult::new(server_info, caps)
        .instructions("Use this server to perform operations");

    let json = serde_json::to_value(&result).unwrap();
    let json_str = serde_json::to_string(&result).unwrap();

    // Verify camelCase field names
    assert!(json_str.contains("\"protocolVersion\""));
    assert!(json_str.contains("\"serverInfo\""));

    // Verify structure
    assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
    assert!(json.get("capabilities").is_some());
    assert!(json["capabilities"]["tools"].is_object());
    assert!(json["capabilities"]["resources"].is_object());
    assert_eq!(json["serverInfo"]["name"], "test-server");
    assert_eq!(
        json["instructions"],
        "Use this server to perform operations"
    );
}

#[test]
fn test_initialize_result_deserialization_from_rmcp_format() {
    // Simulate an InitializeResult as it would come from rmcp
    let rmcp_result = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {
                "listChanged": true
            },
            "resources": {
                "subscribe": true,
                "listChanged": true
            },
            "prompts": {}
        },
        "serverInfo": {
            "name": "rmcp-server",
            "version": "0.1.0"
        }
    });

    let result: InitializeResult = serde_json::from_value(rmcp_result).unwrap();

    assert_eq!(result.protocol_version, "2024-11-05");
    assert_eq!(result.server_info.name, "rmcp-server");
    assert!(result.capabilities.has_tools());
    assert!(result.capabilities.has_resources());
    assert!(result.capabilities.has_prompts());
    assert!(result.capabilities.tools.unwrap().list_changed.unwrap());
}

#[test]
fn test_initialize_result_without_instructions() {
    let server_info = ServerInfo::new("test", "1.0");
    let result = InitializeResult::new(server_info, ServerCapabilities::default());

    let json_str = serde_json::to_string(&result).unwrap();

    // instructions should be skipped when None
    assert!(!json_str.contains("instructions"));
}

#[test]
fn test_initialize_result_minimal() {
    // Minimal valid initialize result
    let minimal = json!({
        "protocolVersion": "2025-11-25",
        "capabilities": {},
        "serverInfo": {
            "name": "minimal-server",
            "version": "1.0"
        }
    });

    let result: InitializeResult = serde_json::from_value(minimal).unwrap();

    assert_eq!(result.protocol_version, "2025-11-25");
    assert!(!result.capabilities.has_tools());
}

// =============================================================================
// ServerInfo / ClientInfo Schema Tests
// =============================================================================

#[test]
fn test_server_info_serialization() {
    let info = ServerInfo::new("my-server", "1.2.3");

    let json = serde_json::to_value(&info).unwrap();

    assert_eq!(json["name"], "my-server");
    assert_eq!(json["version"], "1.2.3");
}

#[test]
fn test_client_info_serialization() {
    let info = ClientInfo::new("my-client", "4.5.6");

    let json = serde_json::to_value(&info).unwrap();

    assert_eq!(json["name"], "my-client");
    assert_eq!(json["version"], "4.5.6");
}

// =============================================================================
// Individual Capability Type Tests
// =============================================================================

#[test]
fn test_tool_capability_serialization() {
    let cap = ToolCapability {
        list_changed: Some(true),
    };

    let json = serde_json::to_value(&cap).unwrap();
    let json_str = serde_json::to_string(&cap).unwrap();

    assert!(json_str.contains("\"listChanged\""));
    assert_eq!(json["listChanged"], true);
}

#[test]
fn test_tool_capability_empty() {
    let cap = ToolCapability::default();

    let json_str = serde_json::to_string(&cap).unwrap();

    // Empty tool capability should serialize to empty object
    assert_eq!(json_str, "{}");
}

#[test]
fn test_resource_capability_serialization() {
    let cap = ResourceCapability {
        subscribe: Some(true),
        list_changed: Some(true),
    };

    let json = serde_json::to_value(&cap).unwrap();
    let json_str = serde_json::to_string(&cap).unwrap();

    assert!(json_str.contains("\"subscribe\""));
    assert!(json_str.contains("\"listChanged\""));
    assert_eq!(json["subscribe"], true);
    assert_eq!(json["listChanged"], true);
}

#[test]
fn test_prompt_capability_serialization() {
    let cap = PromptCapability {
        list_changed: Some(true),
    };

    let json = serde_json::to_value(&cap).unwrap();
    let json_str = serde_json::to_string(&cap).unwrap();

    assert!(json_str.contains("\"listChanged\""));
    assert_eq!(json["listChanged"], true);
}

#[test]
fn test_task_capability_serialization() {
    let cap = TaskCapability {
        cancellable: Some(true),
    };

    let json = serde_json::to_value(&cap).unwrap();

    assert_eq!(json["cancellable"], true);
}

#[test]
fn test_roots_capability_serialization() {
    let cap = RootsCapability {
        list_changed: Some(true),
    };

    let json = serde_json::to_value(&cap).unwrap();
    let json_str = serde_json::to_string(&cap).unwrap();

    assert!(json_str.contains("\"listChanged\""));
    assert_eq!(json["listChanged"], true);
}

#[test]
fn test_logging_capability_serialization() {
    let cap = LoggingCapability {};

    let json_str = serde_json::to_string(&cap).unwrap();

    // Empty logging capability should serialize to empty object
    assert_eq!(json_str, "{}");
}

#[test]
fn test_completion_capability_serialization() {
    let cap = CompletionCapability {};

    let json_str = serde_json::to_string(&cap).unwrap();

    assert_eq!(json_str, "{}");
}

#[test]
fn test_sampling_capability_serialization() {
    let cap = SamplingCapability {};

    let json_str = serde_json::to_string(&cap).unwrap();

    assert_eq!(json_str, "{}");
}

#[test]
fn test_elicitation_capability_serialization() {
    let cap = ElicitationCapability {};

    let json_str = serde_json::to_string(&cap).unwrap();

    assert_eq!(json_str, "{}");
}

// =============================================================================
// Deserialization from rmcp Format Tests
// =============================================================================

#[test]
fn test_server_capabilities_deserialization_from_rmcp() {
    let rmcp_caps = json!({
        "tools": {
            "listChanged": true
        },
        "resources": {
            "subscribe": true,
            "listChanged": true
        },
        "prompts": {
            "listChanged": false
        },
        "tasks": {
            "cancellable": true
        },
        "logging": {},
        "completions": {},
        "experimental": {
            "customFeature": true
        }
    });

    let caps: ServerCapabilities = serde_json::from_value(rmcp_caps).unwrap();

    assert!(caps.has_tools());
    assert!(caps.has_resources());
    assert!(caps.has_prompts());
    assert!(caps.has_tasks());
    assert!(caps.logging.is_some());
    assert!(caps.completions.is_some());
    assert!(caps.experimental.is_some());

    assert!(caps.tools.unwrap().list_changed.unwrap());
    assert!(caps.resources.as_ref().unwrap().subscribe.unwrap());
}

#[test]
fn test_client_capabilities_deserialization_from_rmcp() {
    let rmcp_caps = json!({
        "roots": {
            "listChanged": true
        },
        "sampling": {},
        "elicitation": {}
    });

    let caps: ClientCapabilities = serde_json::from_value(rmcp_caps).unwrap();

    assert!(caps.has_roots());
    assert!(caps.has_sampling());
    assert!(caps.has_elicitation());
    assert!(caps.roots.unwrap().list_changed.unwrap());
}

// =============================================================================
// Protocol Version Negotiation Tests
// =============================================================================

#[test]
fn test_protocol_version_constant() {
    // Current latest version
    assert_eq!(PROTOCOL_VERSION, "2025-11-25");
}

#[test]
fn test_supported_versions() {
    // All 4 MCP protocol versions should be supported
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-11-25")); // Latest
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-06-18")); // Elicitation
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2025-03-26")); // OAuth 2.1
    assert!(SUPPORTED_PROTOCOL_VERSIONS.contains(&"2024-11-05")); // Original
}

#[test]
fn test_is_version_supported() {
    // All 4 versions should be supported
    assert!(is_version_supported("2025-11-25"));
    assert!(is_version_supported("2025-06-18"));
    assert!(is_version_supported("2025-03-26"));
    assert!(is_version_supported("2024-11-05"));
    // Invalid versions should not be supported
    assert!(!is_version_supported("1.0.0"));
    assert!(!is_version_supported(""));
    assert!(!is_version_supported("invalid"));
}

#[test]
fn test_negotiate_version_supported() {
    // Supported versions should be returned as-is
    assert_eq!(negotiate_version("2025-11-25"), "2025-11-25");
    assert_eq!(negotiate_version("2024-11-05"), "2024-11-05");
}

#[test]
fn test_negotiate_version_unsupported() {
    // Unsupported versions get server's preferred version
    assert_eq!(negotiate_version("1.0.0"), PROTOCOL_VERSION);
    assert_eq!(negotiate_version(""), PROTOCOL_VERSION);
    assert_eq!(negotiate_version("invalid"), PROTOCOL_VERSION);
}

#[test]
fn test_negotiate_version_detailed_accepted() {
    let result = negotiate_version_detailed("2025-11-25");

    assert!(matches!(result, VersionNegotiationResult::Accepted(_)));
    assert!(result.is_exact_match());
    assert_eq!(result.version(), "2025-11-25");
}

#[test]
fn test_negotiate_version_detailed_counter_offer() {
    let result = negotiate_version_detailed("1.0.0");

    assert!(matches!(
        result,
        VersionNegotiationResult::CounterOffer { .. }
    ));
    assert!(!result.is_exact_match());
    assert_eq!(result.version(), PROTOCOL_VERSION);

    if let VersionNegotiationResult::CounterOffer { requested, offered } = result {
        assert_eq!(requested, "1.0.0");
        assert_eq!(offered, PROTOCOL_VERSION);
    }
}

// =============================================================================
// Full Handshake Simulation Tests
// =============================================================================

#[test]
fn test_full_handshake_latest_version() {
    // Simulate a client connecting with the latest version
    let client_info = ClientInfo::new("test-client", "1.0");
    let client_caps = ClientCapabilities::new().with_sampling();
    let request = InitializeRequest::new(client_info, client_caps);

    // Client requests latest version
    assert_eq!(request.protocol_version, PROTOCOL_VERSION);

    // Server negotiates and responds
    let negotiated = negotiate_version(&request.protocol_version);
    assert_eq!(negotiated, PROTOCOL_VERSION);

    let server_info = ServerInfo::new("test-server", "1.0");
    let server_caps = ServerCapabilities::new().with_tools();
    let result = InitializeResult {
        protocol_version: negotiated.to_string(),
        capabilities: server_caps,
        server_info,
        instructions: None,
    };

    // Client validates
    assert!(is_version_supported(&result.protocol_version));
}

#[test]
fn test_full_handshake_rmcp_version() {
    // Simulate an rmcp client connecting with the original version
    let request_json = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "roots": {}
        },
        "clientInfo": {
            "name": "rmcp-client",
            "version": "0.1"
        }
    });

    let request: InitializeRequest = serde_json::from_value(request_json).unwrap();

    // Server negotiates
    let negotiated = negotiate_version(&request.protocol_version);
    assert_eq!(negotiated, "2024-11-05"); // Should accept the original version

    let server_info = ServerInfo::new("rust-mcp-server", "0.1.0");
    let server_caps = ServerCapabilities::new().with_tools().with_resources();

    let result = InitializeResult {
        protocol_version: negotiated.to_string(),
        capabilities: server_caps,
        server_info,
        instructions: None,
    };

    // Result should be serializable and match rmcp expectations
    let result_json = serde_json::to_value(&result).unwrap();
    assert_eq!(result_json["protocolVersion"], "2024-11-05");
    assert!(result_json["capabilities"]["tools"].is_object());
}

#[test]
fn test_full_handshake_unknown_version() {
    // Client with unknown version
    let request = InitializeRequest {
        protocol_version: "1.0.0".to_string(),
        capabilities: ClientCapabilities::default(),
        client_info: ClientInfo::new("old-client", "1.0"),
    };

    // Server responds with counter-offer
    let negotiated = negotiate_version(&request.protocol_version);
    assert_eq!(negotiated, PROTOCOL_VERSION);

    // Client must check if it supports the offered version
    // (In real implementation, client would disconnect if unsupported)
}

// =============================================================================
// Round-Trip Serialization Tests
// =============================================================================

#[test]
fn test_server_capabilities_round_trip() {
    let original = ServerCapabilities::new()
        .with_tools_and_changes()
        .with_resources_and_subscriptions()
        .with_prompts()
        .with_tasks()
        .with_logging()
        .with_completions();

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: ServerCapabilities = serde_json::from_str(&json).unwrap();

    assert_eq!(original.has_tools(), deserialized.has_tools());
    assert_eq!(original.has_resources(), deserialized.has_resources());
    assert_eq!(original.has_prompts(), deserialized.has_prompts());
    assert_eq!(original.has_tasks(), deserialized.has_tasks());
}

#[test]
fn test_client_capabilities_round_trip() {
    let original = ClientCapabilities::new()
        .with_roots_and_changes()
        .with_sampling()
        .with_elicitation();

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: ClientCapabilities = serde_json::from_str(&json).unwrap();

    assert_eq!(original.has_roots(), deserialized.has_roots());
    assert_eq!(original.has_sampling(), deserialized.has_sampling());
    assert_eq!(original.has_elicitation(), deserialized.has_elicitation());
}

#[test]
fn test_initialize_request_round_trip() {
    let original = InitializeRequest::new(
        ClientInfo::new("round-trip-client", "1.0"),
        ClientCapabilities::new().with_sampling(),
    );

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: InitializeRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(original.protocol_version, deserialized.protocol_version);
    assert_eq!(original.client_info.name, deserialized.client_info.name);
}

#[test]
fn test_initialize_result_round_trip() {
    let original = InitializeResult::new(
        ServerInfo::new("round-trip-server", "1.0"),
        ServerCapabilities::new().with_tools(),
    )
    .instructions("Test instructions");

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: InitializeResult = serde_json::from_str(&json).unwrap();

    assert_eq!(original.protocol_version, deserialized.protocol_version);
    assert_eq!(original.server_info.name, deserialized.server_info.name);
    assert_eq!(original.instructions, deserialized.instructions);
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_capabilities_with_null_experimental() {
    let json = json!({
        "tools": {},
        "experimental": null
    });

    let caps: ServerCapabilities = serde_json::from_value(json).unwrap();

    assert!(caps.has_tools());
    // experimental is None when null
    assert!(caps.experimental.is_none());
}

#[test]
fn test_capabilities_with_extra_fields() {
    // rmcp might send extra fields we don't know about
    let json = json!({
        "tools": {},
        "unknownCapability": {
            "someField": true
        }
    });

    // Should still deserialize successfully (serde default behavior)
    let caps: ServerCapabilities = serde_json::from_value(json).unwrap();

    assert!(caps.has_tools());
}

#[test]
fn test_empty_capability_structs_serialize_to_empty_object() {
    // Per MCP spec, empty capability objects should be {}
    assert_eq!(serde_json::to_string(&LoggingCapability {}).unwrap(), "{}");
    assert_eq!(
        serde_json::to_string(&CompletionCapability {}).unwrap(),
        "{}"
    );
    assert_eq!(serde_json::to_string(&SamplingCapability {}).unwrap(), "{}");
    assert_eq!(
        serde_json::to_string(&ElicitationCapability {}).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&ToolCapability::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&ResourceCapability::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&PromptCapability::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&TaskCapability::default()).unwrap(),
        "{}"
    );
    assert_eq!(
        serde_json::to_string(&RootsCapability::default()).unwrap(),
        "{}"
    );
}
