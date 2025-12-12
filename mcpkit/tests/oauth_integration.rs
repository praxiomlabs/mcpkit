//! OAuth 2.1 integration tests.
//!
//! These tests verify that the OAuth 2.1 implementation is compatible with
//! the MCP specification (2025-06-18) and follows standard OAuth 2.1 patterns.
//!
//! The tests cover:
//! 1. Protected Resource Metadata (RFC 9728)
//! 2. Authorization Server Metadata (RFC 8414)
//! 3. PKCE (RFC 7636)
//! 4. Authorization Code flow
//! 5. Client Credentials flow
//! 6. Dynamic Client Registration (RFC 7591)
//! 7. Token handling and validation
//! 8. WWW-Authenticate header parsing

use mcpkit::auth::{
    AuthorizationConfig, AuthorizationRequest, AuthorizationServerMetadata,
    ClientRegistrationRequest, ClientRegistrationResponse, CodeChallengeMethod, GrantType,
    OAuthError, OAuthErrorResponse, PkceChallenge, ProtectedResourceMetadata, StoredToken,
    TokenRequest, TokenResponse, WwwAuthenticate,
};
use serde_json::json;
use std::time::{Duration, SystemTime};

// =============================================================================
// Protected Resource Metadata Tests (RFC 9728)
// =============================================================================

#[test]
fn test_protected_resource_metadata_basic() {
    let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
        .with_authorization_server("https://auth.example.com");

    assert_eq!(metadata.resource, "https://mcp.example.com");
    assert_eq!(metadata.authorization_servers.len(), 1);
    assert!(metadata.validate().is_ok());
}

#[test]
fn test_protected_resource_metadata_multiple_auth_servers() {
    let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
        .with_authorization_server("https://auth1.example.com")
        .with_authorization_server("https://auth2.example.com");

    assert_eq!(metadata.authorization_servers.len(), 2);
    assert!(metadata.validate().is_ok());
}

#[test]
fn test_protected_resource_metadata_with_scopes() {
    let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
        .with_authorization_server("https://auth.example.com")
        .with_scopes(["mcp:read", "mcp:write", "mcp:admin"]);

    let scopes = metadata.scopes_supported.as_ref().unwrap();
    assert_eq!(scopes.len(), 3);
    assert!(scopes.contains(&"mcp:read".to_string()));
}

#[test]
fn test_protected_resource_metadata_validation_no_auth_server() {
    let metadata = ProtectedResourceMetadata::new("https://mcp.example.com");

    let result = metadata.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("authorization server is required"));
}

#[test]
fn test_protected_resource_metadata_validation_empty_resource() {
    let metadata =
        ProtectedResourceMetadata::new("").with_authorization_server("https://auth.example.com");

    let result = metadata.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Resource identifier"));
}

#[test]
fn test_protected_resource_metadata_serialization() {
    let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
        .with_authorization_server("https://auth.example.com")
        .with_scopes(["mcp:read"])
        .with_documentation("https://docs.example.com");

    let json = serde_json::to_value(&metadata).unwrap();

    assert_eq!(json["resource"], "https://mcp.example.com");
    assert!(json["authorization_servers"].is_array());
    assert!(json["scopes_supported"].is_array());
    assert_eq!(json["resource_documentation"], "https://docs.example.com");
}

#[test]
fn test_protected_resource_metadata_deserialization() {
    let json = json!({
        "resource": "https://mcp.example.com",
        "authorization_servers": ["https://auth.example.com"],
        "scopes_supported": ["mcp:read", "mcp:write"],
        "bearer_methods_supported": ["header"]
    });

    let metadata: ProtectedResourceMetadata = serde_json::from_value(json).unwrap();

    assert_eq!(metadata.resource, "https://mcp.example.com");
    assert_eq!(metadata.authorization_servers.len(), 1);
    assert!(metadata.validate().is_ok());
}

#[test]
fn test_protected_resource_metadata_well_known_url() {
    let url = ProtectedResourceMetadata::well_known_url("https://mcp.example.com");

    assert!(url.is_some());
    assert_eq!(
        url.unwrap(),
        "https://mcp.example.com/.well-known/oauth-protected-resource"
    );
}

// =============================================================================
// Authorization Server Metadata Tests (RFC 8414)
// =============================================================================

#[test]
fn test_authorization_server_metadata_basic() {
    let metadata = AuthorizationServerMetadata::new(
        "https://auth.example.com",
        "https://auth.example.com/authorize",
        "https://auth.example.com/token",
    );

    assert_eq!(metadata.issuer, "https://auth.example.com");
    assert_eq!(
        metadata.authorization_endpoint,
        "https://auth.example.com/authorize"
    );
    assert_eq!(metadata.token_endpoint, "https://auth.example.com/token");
}

#[test]
fn test_authorization_server_metadata_from_issuer() {
    let metadata = AuthorizationServerMetadata::from_issuer("https://auth.example.com");

    assert_eq!(metadata.issuer, "https://auth.example.com");
    assert_eq!(
        metadata.authorization_endpoint,
        "https://auth.example.com/authorize"
    );
    assert_eq!(metadata.token_endpoint, "https://auth.example.com/token");
}

#[test]
fn test_authorization_server_metadata_with_extras() {
    let metadata = AuthorizationServerMetadata::from_issuer("https://auth.example.com")
        .with_jwks_uri("https://auth.example.com/.well-known/jwks.json")
        .with_registration_endpoint("https://auth.example.com/register");

    assert!(metadata.jwks_uri.is_some());
    assert!(metadata.registration_endpoint.is_some());
}

#[test]
fn test_authorization_server_metadata_default_values() {
    let metadata = AuthorizationServerMetadata::from_issuer("https://auth.example.com");

    // Check default response types
    let response_types = metadata.response_types_supported.as_ref().unwrap();
    assert!(response_types.contains(&"code".to_string()));

    // Check default grant types
    let grant_types = metadata.grant_types_supported.as_ref().unwrap();
    assert!(grant_types.contains(&"authorization_code".to_string()));
    assert!(grant_types.contains(&"client_credentials".to_string()));
    assert!(grant_types.contains(&"refresh_token".to_string()));

    // Check PKCE support
    let code_challenge_methods = metadata.code_challenge_methods_supported.as_ref().unwrap();
    assert!(code_challenge_methods.contains(&"S256".to_string()));
}

#[test]
fn test_authorization_server_metadata_serialization() {
    let metadata = AuthorizationServerMetadata::from_issuer("https://auth.example.com")
        .with_jwks_uri("https://auth.example.com/.well-known/jwks.json");

    let json = serde_json::to_value(&metadata).unwrap();

    assert_eq!(json["issuer"], "https://auth.example.com");
    assert_eq!(
        json["authorization_endpoint"],
        "https://auth.example.com/authorize"
    );
    assert!(json["grant_types_supported"].is_array());
}

#[test]
fn test_authorization_server_metadata_deserialization() {
    let json = json!({
        "issuer": "https://auth.example.com",
        "authorization_endpoint": "https://auth.example.com/authorize",
        "token_endpoint": "https://auth.example.com/token",
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "client_credentials"],
        "code_challenge_methods_supported": ["S256"]
    });

    let metadata: AuthorizationServerMetadata = serde_json::from_value(json).unwrap();

    assert_eq!(metadata.issuer, "https://auth.example.com");
}

#[test]
fn test_authorization_server_metadata_well_known_url() {
    let url = AuthorizationServerMetadata::well_known_url("https://auth.example.com");

    assert!(url.is_some());
    assert_eq!(
        url.unwrap(),
        "https://auth.example.com/.well-known/oauth-authorization-server"
    );
}

// =============================================================================
// PKCE Tests (RFC 7636)
// =============================================================================

#[test]
fn test_pkce_challenge_generation() {
    let pkce = PkceChallenge::new();

    // Verifier should be non-empty
    assert!(!pkce.verifier.is_empty());
    // Challenge should be non-empty
    assert!(!pkce.challenge.is_empty());
    // For S256, verifier and challenge should be different
    assert_ne!(pkce.verifier, pkce.challenge);
    // Default method is S256
    assert_eq!(pkce.method, CodeChallengeMethod::S256);
}

#[test]
fn test_pkce_challenge_s256() {
    let pkce = PkceChallenge::with_method(CodeChallengeMethod::S256);

    assert_eq!(pkce.method, CodeChallengeMethod::S256);
    // Verification should work
    assert!(PkceChallenge::verify(
        &pkce.verifier,
        &pkce.challenge,
        CodeChallengeMethod::S256
    ));
}

#[test]
fn test_pkce_challenge_plain() {
    let pkce = PkceChallenge::with_method(CodeChallengeMethod::Plain);

    assert_eq!(pkce.method, CodeChallengeMethod::Plain);
    // For plain, verifier equals challenge
    assert_eq!(pkce.verifier, pkce.challenge);
    // Verification should work
    assert!(PkceChallenge::verify(
        &pkce.verifier,
        &pkce.challenge,
        CodeChallengeMethod::Plain
    ));
}

#[test]
fn test_pkce_verification_wrong_verifier() {
    let pkce = PkceChallenge::new();

    assert!(!PkceChallenge::verify(
        "wrong_verifier",
        &pkce.challenge,
        CodeChallengeMethod::S256
    ));
}

#[test]
fn test_pkce_verification_wrong_method() {
    let pkce = PkceChallenge::with_method(CodeChallengeMethod::S256);

    // Using Plain method with S256 challenge should fail
    assert!(!PkceChallenge::verify(
        &pkce.verifier,
        &pkce.challenge,
        CodeChallengeMethod::Plain
    ));
}

#[test]
fn test_pkce_uniqueness() {
    // Each PKCE challenge should be unique
    let pkce1 = PkceChallenge::new();
    let pkce2 = PkceChallenge::new();

    assert_ne!(pkce1.verifier, pkce2.verifier);
    assert_ne!(pkce1.challenge, pkce2.challenge);
}

#[test]
fn test_code_challenge_method_display() {
    assert_eq!(CodeChallengeMethod::S256.to_string(), "S256");
    assert_eq!(CodeChallengeMethod::Plain.to_string(), "plain");
}

// =============================================================================
// Authorization Request Tests
// =============================================================================

#[test]
fn test_authorization_request_basic() {
    let pkce = PkceChallenge::new();
    let request = AuthorizationRequest::new("client123", &pkce, "https://mcp.example.com");

    assert_eq!(request.response_type, "code");
    assert_eq!(request.client_id, "client123");
    assert_eq!(request.resource, "https://mcp.example.com");
    assert_eq!(request.code_challenge, pkce.challenge);
    assert_eq!(request.code_challenge_method, "S256");
}

#[test]
fn test_authorization_request_with_options() {
    let pkce = PkceChallenge::new();
    let request = AuthorizationRequest::new("client123", &pkce, "https://mcp.example.com")
        .with_redirect_uri("http://localhost:8080/callback")
        .with_scope("mcp:read mcp:write")
        .with_state("random_state_value");

    assert_eq!(
        request.redirect_uri,
        Some("http://localhost:8080/callback".to_string())
    );
    assert_eq!(request.scope, Some("mcp:read mcp:write".to_string()));
    assert_eq!(request.state, Some("random_state_value".to_string()));
}

#[test]
fn test_authorization_request_build_url() {
    let pkce = PkceChallenge::new();
    let request = AuthorizationRequest::new("client123", &pkce, "https://mcp.example.com")
        .with_redirect_uri("http://localhost:8080/callback")
        .with_scope("mcp:read")
        .with_state("state123");

    let url = request
        .build_url("https://auth.example.com/authorize")
        .unwrap();

    assert!(url.starts_with("https://auth.example.com/authorize?"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=client123"));
    assert!(url.contains("code_challenge="));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("resource="));
    assert!(url.contains("redirect_uri="));
    assert!(url.contains("scope=mcp%3Aread")); // URL-encoded
    assert!(url.contains("state=state123"));
}

#[test]
fn test_authorization_request_serialization() {
    let pkce = PkceChallenge::new();
    let request = AuthorizationRequest::new("client123", &pkce, "https://mcp.example.com");

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["response_type"], "code");
    assert_eq!(json["client_id"], "client123");
    assert_eq!(json["code_challenge_method"], "S256");
}

// =============================================================================
// Token Request Tests
// =============================================================================

#[test]
fn test_token_request_authorization_code() {
    let request = TokenRequest::authorization_code(
        "auth_code_123",
        "client123",
        "verifier456",
        "https://mcp.example.com",
    );

    assert_eq!(request.grant_type, "authorization_code");
    assert_eq!(request.code, Some("auth_code_123".to_string()));
    assert_eq!(request.client_id, "client123");
    assert_eq!(request.code_verifier, Some("verifier456".to_string()));
    assert_eq!(
        request.resource,
        Some("https://mcp.example.com".to_string())
    );
}

#[test]
fn test_token_request_client_credentials() {
    let request =
        TokenRequest::client_credentials("client123", "secret789", "https://mcp.example.com");

    assert_eq!(request.grant_type, "client_credentials");
    assert_eq!(request.client_id, "client123");
    assert_eq!(request.client_secret, Some("secret789".to_string()));
    assert_eq!(
        request.resource,
        Some("https://mcp.example.com".to_string())
    );
}

#[test]
fn test_token_request_refresh() {
    let request = TokenRequest::refresh("refresh_token_123", "client123");

    assert_eq!(request.grant_type, "refresh_token");
    assert_eq!(request.refresh_token, Some("refresh_token_123".to_string()));
    assert_eq!(request.client_id, "client123");
}

#[test]
fn test_token_request_with_options() {
    let request =
        TokenRequest::authorization_code("code", "client", "verifier", "https://mcp.example.com")
            .with_redirect_uri("http://localhost:8080/callback")
            .with_scope("mcp:read");

    assert!(request.redirect_uri.is_some());
    assert!(request.scope.is_some());
}

#[test]
fn test_token_request_serialization() {
    let request = TokenRequest::authorization_code(
        "code123",
        "client123",
        "verifier456",
        "https://mcp.example.com",
    );

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["grant_type"], "authorization_code");
    assert_eq!(json["code"], "code123");
    assert_eq!(json["code_verifier"], "verifier456");
}

// =============================================================================
// Token Response Tests
// =============================================================================

#[test]
fn test_token_response_basic() {
    let response = TokenResponse {
        access_token: "access_token_123".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600),
        refresh_token: Some("refresh_token_456".to_string()),
        scope: Some("mcp:read".to_string()),
    };

    assert_eq!(response.access_token, "access_token_123");
    assert_eq!(response.token_type, "Bearer");
    assert_eq!(response.expires_in, Some(3600));
}

#[test]
fn test_token_response_expiration_check() {
    let response = TokenResponse {
        access_token: "token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600), // 1 hour
        refresh_token: None,
        scope: None,
    };

    // Token issued now should not be expired
    assert!(!response.is_expired(SystemTime::now()));

    // Token issued 2 hours ago should be expired (with 1 hour expiry)
    let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
    assert!(response.is_expired(two_hours_ago));
}

#[test]
fn test_token_response_no_expiration() {
    let response = TokenResponse {
        access_token: "token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: None, // No expiration
        refresh_token: None,
        scope: None,
    };

    // Token without expiration should never be expired
    let long_ago = SystemTime::UNIX_EPOCH;
    assert!(!response.is_expired(long_ago));
}

#[test]
fn test_token_response_deserialization() {
    let json = json!({
        "access_token": "access123",
        "token_type": "Bearer",
        "expires_in": 3600,
        "refresh_token": "refresh456",
        "scope": "mcp:read mcp:write"
    });

    let response: TokenResponse = serde_json::from_value(json).unwrap();

    assert_eq!(response.access_token, "access123");
    assert_eq!(response.expires_in, Some(3600));
}

// =============================================================================
// Stored Token Tests
// =============================================================================

#[test]
fn test_stored_token_basic() {
    let token = TokenResponse {
        access_token: "access123".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600),
        refresh_token: None,
        scope: None,
    };

    let stored = StoredToken::new(token, "https://mcp.example.com");

    assert!(!stored.is_expired());
    assert_eq!(stored.resource, "https://mcp.example.com");
}

#[test]
fn test_stored_token_authorization_header() {
    let token = TokenResponse {
        access_token: "my_access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: None,
        refresh_token: None,
        scope: None,
    };

    let stored = StoredToken::new(token, "https://mcp.example.com");

    assert_eq!(stored.authorization_header(), "Bearer my_access_token");
}

#[test]
fn test_stored_token_expires_within() {
    let token = TokenResponse {
        access_token: "token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(60), // 1 minute
        refresh_token: None,
        scope: None,
    };

    let stored = StoredToken::new(token, "https://mcp.example.com");

    // Should expire within 2 minutes
    assert!(stored.expires_within(Duration::from_secs(120)));
    // Should not expire within 30 seconds (from now)
    assert!(!stored.expires_within(Duration::from_secs(30)));
}

// =============================================================================
// WWW-Authenticate Header Tests
// =============================================================================

#[test]
fn test_www_authenticate_basic() {
    let header =
        WwwAuthenticate::new("https://mcp.example.com/.well-known/oauth-protected-resource");

    let value = header.to_header_value();

    assert!(value.starts_with("Bearer resource_metadata="));
    assert!(value.contains("oauth-protected-resource"));
}

#[test]
fn test_www_authenticate_with_options() {
    let header =
        WwwAuthenticate::new("https://mcp.example.com/.well-known/oauth-protected-resource")
            .with_realm("MCP Server")
            .with_error(OAuthError::InvalidToken)
            .with_error_description("The access token expired");

    let value = header.to_header_value();

    assert!(value.contains("realm=\"MCP Server\""));
    assert!(value.contains("error=\"invalid_token\""));
    assert!(value.contains("error_description=\"The access token expired\""));
}

#[test]
fn test_www_authenticate_parse() {
    let header_value = "Bearer resource_metadata=\"https://mcp.example.com/.well-known/oauth-protected-resource\", realm=\"MCP\"";

    let parsed = WwwAuthenticate::parse(header_value);

    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert!(parsed
        .resource_metadata
        .contains("oauth-protected-resource"));
    assert_eq!(parsed.realm, Some("MCP".to_string()));
}

#[test]
fn test_www_authenticate_parse_with_error() {
    let header_value = "Bearer resource_metadata=\"https://example.com\", error=\"invalid_token\", error_description=\"Token expired\"";

    let parsed = WwwAuthenticate::parse(header_value);

    assert!(parsed.is_some());
    let parsed = parsed.unwrap();
    assert_eq!(parsed.error, Some(OAuthError::InvalidToken));
    assert_eq!(parsed.error_description, Some("Token expired".to_string()));
}

#[test]
fn test_www_authenticate_parse_invalid() {
    // Not a Bearer scheme
    assert!(WwwAuthenticate::parse("Basic realm=\"test\"").is_none());
    // Empty string
    assert!(WwwAuthenticate::parse("").is_none());
}

// =============================================================================
// OAuth Error Tests
// =============================================================================

#[test]
fn test_oauth_error_display() {
    assert_eq!(OAuthError::InvalidRequest.to_string(), "invalid_request");
    assert_eq!(
        OAuthError::UnauthorizedClient.to_string(),
        "unauthorized_client"
    );
    assert_eq!(OAuthError::AccessDenied.to_string(), "access_denied");
    assert_eq!(OAuthError::InvalidToken.to_string(), "invalid_token");
    assert_eq!(
        OAuthError::InsufficientScope.to_string(),
        "insufficient_scope"
    );
}

#[test]
fn test_oauth_error_response() {
    let response =
        OAuthErrorResponse::new(OAuthError::InvalidToken).with_description("Token has expired");

    assert_eq!(response.error, OAuthError::InvalidToken);
    assert_eq!(
        response.error_description,
        Some("Token has expired".to_string())
    );
}

#[test]
fn test_oauth_error_serialization() {
    let response =
        OAuthErrorResponse::new(OAuthError::InvalidRequest).with_description("Missing parameter");

    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["error"], "invalid_request");
    assert_eq!(json["error_description"], "Missing parameter");
}

#[test]
fn test_oauth_error_deserialization() {
    let json = json!({
        "error": "invalid_grant",
        "error_description": "The authorization code has expired"
    });

    let response: OAuthErrorResponse = serde_json::from_value(json).unwrap();

    assert_eq!(response.error, OAuthError::InvalidGrant);
    assert!(response.error_description.unwrap().contains("expired"));
}

// =============================================================================
// Grant Type Tests
// =============================================================================

#[test]
fn test_grant_type_display() {
    assert_eq!(
        GrantType::AuthorizationCode.to_string(),
        "authorization_code"
    );
    assert_eq!(
        GrantType::ClientCredentials.to_string(),
        "client_credentials"
    );
    assert_eq!(GrantType::RefreshToken.to_string(), "refresh_token");
}

#[test]
fn test_grant_type_serialization() {
    let json = serde_json::to_value(GrantType::AuthorizationCode).unwrap();
    assert_eq!(json, "authorization_code");
}

// =============================================================================
// Authorization Config Tests
// =============================================================================

#[test]
fn test_authorization_config_basic() {
    let config = AuthorizationConfig::new("https://auth.example.com")
        .with_client_id("my-client")
        .with_resource("https://mcp.example.com");

    assert_eq!(config.authorization_server, "https://auth.example.com");
    assert_eq!(config.client_id, "my-client");
    assert!(config.is_public_client()); // No secret = public client
}

#[test]
fn test_authorization_config_confidential_client() {
    let config = AuthorizationConfig::new("https://auth.example.com")
        .with_client_id("my-client")
        .with_client_secret("my-secret");

    assert!(!config.is_public_client()); // Has secret = confidential client
}

#[test]
fn test_authorization_config_with_scopes() {
    let config = AuthorizationConfig::new("https://auth.example.com")
        .with_client_id("my-client")
        .with_scope("mcp:read")
        .with_scope("mcp:write");

    assert_eq!(config.scopes.len(), 2);
    assert!(config.scopes.contains(&"mcp:read".to_string()));
    assert!(config.scopes.contains(&"mcp:write".to_string()));
}

// =============================================================================
// Dynamic Client Registration Tests (RFC 7591)
// =============================================================================

#[test]
fn test_client_registration_request_basic() {
    let request = ClientRegistrationRequest::new();

    // Defaults for public client
    assert_eq!(request.token_endpoint_auth_method, Some("none".to_string()));
    assert!(request
        .grant_types
        .as_ref()
        .unwrap()
        .contains(&"authorization_code".to_string()));
}

#[test]
fn test_client_registration_request_with_options() {
    let request = ClientRegistrationRequest::new()
        .with_client_name("My MCP Client")
        .with_redirect_uris([
            "http://localhost:8080/callback",
            "http://localhost:9090/callback",
        ])
        .with_software_id("mcp-client-123");

    assert_eq!(request.client_name, Some("My MCP Client".to_string()));
    assert_eq!(request.redirect_uris.as_ref().unwrap().len(), 2);
    assert_eq!(request.software_id, Some("mcp-client-123".to_string()));
}

#[test]
fn test_client_registration_request_serialization() {
    let request = ClientRegistrationRequest::new().with_client_name("Test Client");

    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["client_name"], "Test Client");
    assert_eq!(json["token_endpoint_auth_method"], "none");
}

#[test]
fn test_client_registration_response_deserialization() {
    let json = json!({
        "client_id": "generated_client_id_123",
        "client_secret": "generated_secret_456",
        "client_secret_expires_at": 0,
        "client_id_issued_at": 1234567890
    });

    let response: ClientRegistrationResponse = serde_json::from_value(json).unwrap();

    assert_eq!(response.client_id, "generated_client_id_123");
    assert_eq!(
        response.client_secret,
        Some("generated_secret_456".to_string())
    );
}

// =============================================================================
// Full OAuth Flow Simulation Tests
// =============================================================================

#[test]
fn test_authorization_code_flow_simulation() {
    // Step 1: Generate PKCE challenge
    let pkce = PkceChallenge::new();

    // Step 2: Build authorization request
    let auth_request = AuthorizationRequest::new("test-client", &pkce, "https://mcp.example.com")
        .with_redirect_uri("http://localhost:8080/callback")
        .with_scope("mcp:read")
        .with_state("csrf_state");

    let auth_url = auth_request
        .build_url("https://auth.example.com/authorize")
        .unwrap();
    assert!(auth_url.contains("code_challenge="));

    // Step 3: Simulate receiving authorization code
    let auth_code = "simulated_auth_code";

    // Step 4: Exchange code for token
    let token_request = TokenRequest::authorization_code(
        auth_code,
        "test-client",
        &pkce.verifier,
        "https://mcp.example.com",
    )
    .with_redirect_uri("http://localhost:8080/callback");

    assert_eq!(token_request.grant_type, "authorization_code");
    assert_eq!(
        token_request.code_verifier.as_ref().unwrap(),
        &pkce.verifier
    );

    // Step 5: Simulate token response
    let token_response = TokenResponse {
        access_token: "access_token_from_server".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600),
        refresh_token: Some("refresh_token_from_server".to_string()),
        scope: Some("mcp:read".to_string()),
    };

    // Step 6: Store token
    let stored = StoredToken::new(token_response, "https://mcp.example.com");
    assert!(!stored.is_expired());
    assert_eq!(
        stored.authorization_header(),
        "Bearer access_token_from_server"
    );
}

#[test]
fn test_client_credentials_flow_simulation() {
    // Step 1: Create token request
    let token_request = TokenRequest::client_credentials(
        "machine-client",
        "machine-secret",
        "https://mcp.example.com",
    );

    assert_eq!(token_request.grant_type, "client_credentials");
    assert!(token_request.client_secret.is_some());

    // Step 2: Simulate token response
    let token_response = TokenResponse {
        access_token: "machine_access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600),
        refresh_token: None, // Machine tokens often don't have refresh tokens
        scope: Some("mcp:admin".to_string()),
    };

    // Step 3: Store and use token
    let stored = StoredToken::new(token_response, "https://mcp.example.com");
    assert_eq!(stored.authorization_header(), "Bearer machine_access_token");
}

#[test]
fn test_token_refresh_flow_simulation() {
    // Simulate having an existing refresh token
    let refresh_token = "existing_refresh_token";

    // Create refresh request
    let token_request = TokenRequest::refresh(refresh_token, "test-client");

    assert_eq!(token_request.grant_type, "refresh_token");
    assert_eq!(token_request.refresh_token.as_ref().unwrap(), refresh_token);

    // Simulate new token response
    let new_token = TokenResponse {
        access_token: "new_access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in: Some(3600),
        refresh_token: Some("new_refresh_token".to_string()),
        scope: None,
    };

    let stored = StoredToken::new(new_token, "https://mcp.example.com");
    assert_eq!(stored.authorization_header(), "Bearer new_access_token");
}

// =============================================================================
// MCP-Specific OAuth Tests
// =============================================================================

#[test]
fn test_mcp_401_response_handling() {
    // Simulate receiving a 401 with WWW-Authenticate header
    let www_authenticate =
        WwwAuthenticate::new("https://mcp.example.com/.well-known/oauth-protected-resource")
            .with_error(OAuthError::InvalidToken)
            .with_error_description("Access token expired");

    let header_value = www_authenticate.to_header_value();

    // Parse the header
    let parsed = WwwAuthenticate::parse(&header_value).unwrap();

    // Client should discover the resource metadata URL
    assert!(parsed
        .resource_metadata
        .contains("oauth-protected-resource"));

    // And understand the error
    assert_eq!(parsed.error, Some(OAuthError::InvalidToken));
}

#[test]
fn test_resource_indicator_required() {
    // MCP requires resource indicator in authorization requests
    let pkce = PkceChallenge::new();
    let request = AuthorizationRequest::new("client", &pkce, "https://mcp.example.com");

    // Resource MUST be set
    assert!(!request.resource.is_empty());

    let url = request
        .build_url("https://auth.example.com/authorize")
        .unwrap();
    assert!(url.contains("resource="));
}
