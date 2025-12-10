//! OAuth 2.1 Authorization for MCP.
//!
//! This module implements OAuth 2.1 authorization per the MCP specification
//! (2025-06-18), including:
//!
//! - Protected Resource Metadata (RFC 9728)
//! - Resource Indicators (RFC 8707)
//! - Authorization Server Metadata (RFC 8414)
//! - PKCE (RFC 7636)
//! - Dynamic Client Registration (RFC 7591)
//!
//! # Overview
//!
//! MCP authorization follows OAuth 2.1 with MCP servers acting as OAuth
//! Resource Servers that validate tokens issued by external Authorization
//! Servers.
//!
//! # Example
//!
//! ```ignore
//! use mcp_core::auth::{
//!     ProtectedResourceMetadata, AuthorizationConfig, TokenValidator,
//! };
//!
//! // Server-side: Expose protected resource metadata
//! let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
//!     .with_authorization_server("https://auth.example.com");
//!
//! // Client-side: Configure authorization
//! let config = AuthorizationConfig::new("https://auth.example.com")
//!     .with_client_id("my-client")
//!     .with_resource("https://mcp.example.com");
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// OAuth 2.1 error types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthError {
    /// The request is missing a required parameter or is otherwise malformed.
    InvalidRequest,
    /// The client is not authorized to request an access token.
    UnauthorizedClient,
    /// The resource owner denied the request.
    AccessDenied,
    /// The authorization server does not support this response type.
    UnsupportedResponseType,
    /// The requested scope is invalid, unknown, or malformed.
    InvalidScope,
    /// The authorization server encountered an unexpected error.
    ServerError,
    /// The server is temporarily unavailable.
    TemporarilyUnavailable,
    /// The provided authorization grant is invalid or expired.
    InvalidGrant,
    /// The client authentication failed.
    InvalidClient,
    /// The grant type is not supported.
    UnsupportedGrantType,
    /// The token is invalid.
    InvalidToken,
    /// Insufficient scope for the requested resource.
    InsufficientScope,
}

impl std::fmt::Display for OAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest => write!(f, "invalid_request"),
            Self::UnauthorizedClient => write!(f, "unauthorized_client"),
            Self::AccessDenied => write!(f, "access_denied"),
            Self::UnsupportedResponseType => write!(f, "unsupported_response_type"),
            Self::InvalidScope => write!(f, "invalid_scope"),
            Self::ServerError => write!(f, "server_error"),
            Self::TemporarilyUnavailable => write!(f, "temporarily_unavailable"),
            Self::InvalidGrant => write!(f, "invalid_grant"),
            Self::InvalidClient => write!(f, "invalid_client"),
            Self::UnsupportedGrantType => write!(f, "unsupported_grant_type"),
            Self::InvalidToken => write!(f, "invalid_token"),
            Self::InsufficientScope => write!(f, "insufficient_scope"),
        }
    }
}

impl std::error::Error for OAuthError {}

/// OAuth 2.1 error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthErrorResponse {
    /// The error code.
    pub error: OAuthError,
    /// Human-readable error description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
    /// URI for more information about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
}

impl OAuthErrorResponse {
    /// Create a new error response.
    #[must_use]
    pub fn new(error: OAuthError) -> Self {
        Self {
            error,
            error_description: None,
            error_uri: None,
        }
    }

    /// Add a description to the error.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.error_description = Some(description.into());
        self
    }
}

/// Protected Resource Metadata per RFC 9728.
///
/// MCP servers MUST implement this to indicate the locations of
/// authorization servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    /// The protected resource identifier (URL).
    pub resource: String,

    /// List of authorization server URLs that can issue tokens for this resource.
    /// MCP requires at least one authorization server.
    pub authorization_servers: Vec<String>,

    /// OAuth 2.0 Bearer token profile URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_methods_supported: Option<Vec<String>>,

    /// Resource documentation URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_documentation: Option<String>,

    /// Supported scopes for this resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,

    /// JWS algorithms supported for resource signing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_signing_alg_values_supported: Option<Vec<String>>,

    /// Additional metadata fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl ProtectedResourceMetadata {
    /// Create new protected resource metadata.
    ///
    /// # Arguments
    ///
    /// * `resource` - The protected resource identifier (URL of the MCP server)
    #[must_use]
    pub fn new(resource: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            authorization_servers: Vec::new(),
            bearer_methods_supported: Some(vec!["header".to_string()]),
            resource_documentation: None,
            scopes_supported: None,
            resource_signing_alg_values_supported: None,
            extra: HashMap::new(),
        }
    }

    /// Add an authorization server.
    #[must_use]
    pub fn with_authorization_server(mut self, server: impl Into<String>) -> Self {
        self.authorization_servers.push(server.into());
        self
    }

    /// Set supported scopes.
    #[must_use]
    pub fn with_scopes(mut self, scopes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.scopes_supported = Some(scopes.into_iter().map(Into::into).collect());
        self
    }

    /// Set documentation URL.
    #[must_use]
    pub fn with_documentation(mut self, url: impl Into<String>) -> Self {
        self.resource_documentation = Some(url.into());
        self
    }

    /// Validate that the metadata meets MCP requirements.
    pub fn validate(&self) -> Result<(), String> {
        if self.resource.is_empty() {
            return Err("Resource identifier is required".to_string());
        }
        if self.authorization_servers.is_empty() {
            return Err("At least one authorization server is required per MCP specification".to_string());
        }
        Ok(())
    }

    /// Get the well-known URL for this resource.
    #[must_use]
    pub fn well_known_url(resource_url: &str) -> Option<String> {
        // Parse the URL and construct the well-known path
        url::Url::parse(resource_url)
            .ok()
            .map(|url| {
                format!(
                    "{}://{}/.well-known/oauth-protected-resource",
                    url.scheme(),
                    url.host_str().unwrap_or("localhost")
                )
            })
    }
}

/// Authorization Server Metadata per RFC 8414.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    /// The authorization server's issuer identifier.
    pub issuer: String,

    /// URL of the authorization endpoint.
    pub authorization_endpoint: String,

    /// URL of the token endpoint.
    pub token_endpoint: String,

    /// URL of the JWKS endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// URL of the dynamic client registration endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,

    /// Supported scopes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_supported: Option<Vec<String>>,

    /// Supported response types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_types_supported: Option<Vec<String>>,

    /// Supported grant types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types_supported: Option<Vec<String>>,

    /// Supported token endpoint auth methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,

    /// Supported PKCE code challenge methods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_methods_supported: Option<Vec<String>>,

    /// URL of the revocation endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,

    /// URL of the introspection endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection_endpoint: Option<String>,

    /// Additional metadata fields.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl AuthorizationServerMetadata {
    /// Create authorization server metadata with required fields.
    #[must_use]
    pub fn new(
        issuer: impl Into<String>,
        authorization_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            issuer: issuer.into(),
            authorization_endpoint: authorization_endpoint.into(),
            token_endpoint: token_endpoint.into(),
            jwks_uri: None,
            registration_endpoint: None,
            scopes_supported: None,
            response_types_supported: Some(vec!["code".to_string()]),
            grant_types_supported: Some(vec![
                "authorization_code".to_string(),
                "client_credentials".to_string(),
                "refresh_token".to_string(),
            ]),
            token_endpoint_auth_methods_supported: None,
            code_challenge_methods_supported: Some(vec!["S256".to_string()]),
            revocation_endpoint: None,
            introspection_endpoint: None,
            extra: HashMap::new(),
        }
    }

    /// Create metadata from an issuer URL using default paths.
    #[must_use]
    pub fn from_issuer(issuer: impl Into<String>) -> Self {
        let issuer = issuer.into();
        Self::new(
            &issuer,
            format!("{issuer}/authorize"),
            format!("{issuer}/token"),
        )
    }

    /// Set the JWKS URI.
    #[must_use]
    pub fn with_jwks_uri(mut self, uri: impl Into<String>) -> Self {
        self.jwks_uri = Some(uri.into());
        self
    }

    /// Set the registration endpoint.
    #[must_use]
    pub fn with_registration_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.registration_endpoint = Some(endpoint.into());
        self
    }

    /// Get the well-known URL for discovering this metadata.
    #[must_use]
    pub fn well_known_url(issuer: &str) -> Option<String> {
        url::Url::parse(issuer)
            .ok()
            .map(|url| {
                format!(
                    "{}://{}/.well-known/oauth-authorization-server",
                    url.scheme(),
                    url.host_str().unwrap_or("localhost")
                )
            })
    }
}

/// OAuth 2.1 grant types supported by MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    /// Authorization code grant (for user authorization).
    AuthorizationCode,
    /// Client credentials grant (for machine-to-machine).
    ClientCredentials,
    /// Refresh token grant.
    RefreshToken,
}

impl std::fmt::Display for GrantType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthorizationCode => write!(f, "authorization_code"),
            Self::ClientCredentials => write!(f, "client_credentials"),
            Self::RefreshToken => write!(f, "refresh_token"),
        }
    }
}

/// PKCE code challenge method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeChallengeMethod {
    /// SHA-256 (recommended).
    #[default]
    S256,
    /// Plain (not recommended, for legacy support only).
    Plain,
}

impl std::fmt::Display for CodeChallengeMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::S256 => write!(f, "S256"),
            Self::Plain => write!(f, "plain"),
        }
    }
}

/// PKCE code verifier and challenge.
#[derive(Debug, Clone)]
pub struct PkceChallenge {
    /// The code verifier (random string).
    pub verifier: String,
    /// The code challenge (derived from verifier).
    pub challenge: String,
    /// The challenge method used.
    pub method: CodeChallengeMethod,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge using S256.
    #[must_use]
    pub fn new() -> Self {
        Self::with_method(CodeChallengeMethod::S256)
    }

    /// Generate a PKCE challenge with the specified method.
    #[must_use]
    pub fn with_method(method: CodeChallengeMethod) -> Self {
        use base64::Engine;
        use rand::Rng;

        // Generate a random 32-byte verifier
        let mut rng = rand::thread_rng();
        let verifier_bytes: [u8; 32] = rng.gen();
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);

        let challenge = match method {
            CodeChallengeMethod::S256 => {
                use sha2::{Digest, Sha256};
                let hash = Sha256::digest(verifier.as_bytes());
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
            }
            CodeChallengeMethod::Plain => verifier.clone(),
        };

        Self {
            verifier,
            challenge,
            method,
        }
    }

    /// Verify a code verifier against a challenge.
    #[must_use]
    pub fn verify(verifier: &str, challenge: &str, method: CodeChallengeMethod) -> bool {
        use base64::Engine;

        let computed = match method {
            CodeChallengeMethod::S256 => {
                use sha2::{Digest, Sha256};
                let hash = Sha256::digest(verifier.as_bytes());
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
            }
            CodeChallengeMethod::Plain => verifier.to_string(),
        };

        computed == challenge
    }
}

impl Default for PkceChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// Authorization request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    /// The response type (must be "code" for authorization code flow).
    pub response_type: String,
    /// The client identifier.
    pub client_id: String,
    /// The redirect URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// The requested scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// State parameter for CSRF protection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// PKCE code challenge.
    pub code_challenge: String,
    /// PKCE code challenge method.
    pub code_challenge_method: String,
    /// Resource indicator (RFC 8707) - REQUIRED by MCP.
    pub resource: String,
}

impl AuthorizationRequest {
    /// Create a new authorization request with PKCE.
    #[must_use]
    pub fn new(
        client_id: impl Into<String>,
        pkce: &PkceChallenge,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            response_type: "code".to_string(),
            client_id: client_id.into(),
            redirect_uri: None,
            scope: None,
            state: None,
            code_challenge: pkce.challenge.clone(),
            code_challenge_method: pkce.method.to_string(),
            resource: resource.into(),
        }
    }

    /// Set the redirect URI.
    #[must_use]
    pub fn with_redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    /// Set the requested scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Set the state parameter.
    #[must_use]
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Build the authorization URL.
    #[must_use]
    pub fn build_url(&self, authorization_endpoint: &str) -> Option<String> {
        let mut url = url::Url::parse(authorization_endpoint).ok()?;

        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", &self.response_type);
            query.append_pair("client_id", &self.client_id);
            query.append_pair("code_challenge", &self.code_challenge);
            query.append_pair("code_challenge_method", &self.code_challenge_method);
            query.append_pair("resource", &self.resource);

            if let Some(ref uri) = self.redirect_uri {
                query.append_pair("redirect_uri", uri);
            }
            if let Some(ref scope) = self.scope {
                query.append_pair("scope", scope);
            }
            if let Some(ref state) = self.state {
                query.append_pair("state", state);
            }
        }

        Some(url.to_string())
    }
}

/// Token request for authorization code exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRequest {
    /// The grant type.
    pub grant_type: String,
    /// The authorization code (for authorization_code grant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The redirect URI (must match the one in authorization request).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    /// The client identifier.
    pub client_id: String,
    /// The client secret (for confidential clients).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// PKCE code verifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_verifier: Option<String>,
    /// Resource indicator (RFC 8707).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    /// Refresh token (for refresh_token grant).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Requested scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl TokenRequest {
    /// Create a token request for authorization code exchange.
    #[must_use]
    pub fn authorization_code(
        code: impl Into<String>,
        client_id: impl Into<String>,
        code_verifier: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "authorization_code".to_string(),
            code: Some(code.into()),
            redirect_uri: None,
            client_id: client_id.into(),
            client_secret: None,
            code_verifier: Some(code_verifier.into()),
            resource: Some(resource.into()),
            refresh_token: None,
            scope: None,
        }
    }

    /// Create a token request for client credentials grant.
    #[must_use]
    pub fn client_credentials(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "client_credentials".to_string(),
            code: None,
            redirect_uri: None,
            client_id: client_id.into(),
            client_secret: Some(client_secret.into()),
            code_verifier: None,
            resource: Some(resource.into()),
            refresh_token: None,
            scope: None,
        }
    }

    /// Create a token request for refresh token grant.
    #[must_use]
    pub fn refresh(
        refresh_token: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            grant_type: "refresh_token".to_string(),
            code: None,
            redirect_uri: None,
            client_id: client_id.into(),
            client_secret: None,
            code_verifier: None,
            resource: None,
            refresh_token: Some(refresh_token.into()),
            scope: None,
        }
    }

    /// Set the redirect URI.
    #[must_use]
    pub fn with_redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    /// Set the requested scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
}

/// Token response from the authorization server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    /// The access token.
    pub access_token: String,
    /// The token type (typically "Bearer").
    pub token_type: String,
    /// The token lifetime in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// The refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// The granted scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

impl TokenResponse {
    /// Check if the token is expired.
    #[must_use]
    pub fn is_expired(&self, issued_at: SystemTime) -> bool {
        if let Some(expires_in) = self.expires_in {
            let expiry = issued_at + Duration::from_secs(expires_in);
            SystemTime::now() >= expiry
        } else {
            false // No expiration means never expires
        }
    }
}

/// Access token with metadata for client-side storage.
#[derive(Debug, Clone)]
pub struct StoredToken {
    /// The token response.
    pub token: TokenResponse,
    /// When the token was issued.
    pub issued_at: SystemTime,
    /// The resource this token is bound to.
    pub resource: String,
}

impl StoredToken {
    /// Create a new stored token.
    #[must_use]
    pub fn new(token: TokenResponse, resource: impl Into<String>) -> Self {
        Self {
            token,
            issued_at: SystemTime::now(),
            resource: resource.into(),
        }
    }

    /// Check if the token is expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.token.is_expired(self.issued_at)
    }

    /// Check if the token will expire within the given duration.
    #[must_use]
    pub fn expires_within(&self, duration: Duration) -> bool {
        if let Some(expires_in) = self.token.expires_in {
            let expiry = self.issued_at + Duration::from_secs(expires_in);
            SystemTime::now() + duration >= expiry
        } else {
            false
        }
    }

    /// Get the Authorization header value.
    #[must_use]
    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.token.access_token)
    }
}

/// WWW-Authenticate header builder for 401 responses.
#[derive(Debug, Clone)]
pub struct WwwAuthenticate {
    /// The authentication realm.
    pub realm: Option<String>,
    /// The resource metadata URL.
    pub resource_metadata: String,
    /// The error code.
    pub error: Option<OAuthError>,
    /// The error description.
    pub error_description: Option<String>,
}

impl WwwAuthenticate {
    /// Create a new WWW-Authenticate header.
    #[must_use]
    pub fn new(resource_metadata: impl Into<String>) -> Self {
        Self {
            realm: None,
            resource_metadata: resource_metadata.into(),
            error: None,
            error_description: None,
        }
    }

    /// Set the realm.
    #[must_use]
    pub fn with_realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = Some(realm.into());
        self
    }

    /// Set the error.
    #[must_use]
    pub fn with_error(mut self, error: OAuthError) -> Self {
        self.error = Some(error);
        self
    }

    /// Set the error description.
    #[must_use]
    pub fn with_error_description(mut self, description: impl Into<String>) -> Self {
        self.error_description = Some(description.into());
        self
    }

    /// Build the header value string per RFC 9728.
    #[must_use]
    pub fn to_header_value(&self) -> String {
        let mut parts = vec![format!("Bearer resource_metadata=\"{}\"", self.resource_metadata)];

        if let Some(ref realm) = self.realm {
            parts.push(format!("realm=\"{realm}\""));
        }
        if let Some(ref error) = self.error {
            parts.push(format!("error=\"{error}\""));
        }
        if let Some(ref desc) = self.error_description {
            parts.push(format!("error_description=\"{desc}\""));
        }

        parts.join(", ")
    }

    /// Parse a WWW-Authenticate header value.
    #[must_use]
    pub fn parse(header_value: &str) -> Option<Self> {
        if !header_value.starts_with("Bearer ") {
            return None;
        }

        let params = &header_value[7..]; // Skip "Bearer "
        let mut resource_metadata = None;
        let mut realm = None;
        let mut error = None;
        let mut error_description = None;

        for part in params.split(", ") {
            if let Some((key, value)) = part.split_once('=') {
                let value = value.trim_matches('"');
                match key.trim() {
                    "resource_metadata" => resource_metadata = Some(value.to_string()),
                    "realm" => realm = Some(value.to_string()),
                    "error" => {
                        error = match value {
                            "invalid_request" => Some(OAuthError::InvalidRequest),
                            "invalid_token" => Some(OAuthError::InvalidToken),
                            "insufficient_scope" => Some(OAuthError::InsufficientScope),
                            _ => None,
                        };
                    }
                    "error_description" => error_description = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        resource_metadata.map(|rm| Self {
            realm,
            resource_metadata: rm,
            error,
            error_description,
        })
    }
}

/// Client authorization configuration.
#[derive(Debug, Clone)]
pub struct AuthorizationConfig {
    /// The authorization server URL.
    pub authorization_server: String,
    /// The client identifier.
    pub client_id: String,
    /// The client secret (for confidential clients).
    pub client_secret: Option<String>,
    /// The redirect URI for authorization code flow.
    pub redirect_uri: Option<String>,
    /// The target resource (MCP server URL).
    pub resource: Option<String>,
    /// The requested scopes.
    pub scopes: Vec<String>,
}

impl AuthorizationConfig {
    /// Create a new authorization configuration.
    #[must_use]
    pub fn new(authorization_server: impl Into<String>) -> Self {
        Self {
            authorization_server: authorization_server.into(),
            client_id: String::new(),
            client_secret: None,
            redirect_uri: None,
            resource: None,
            scopes: Vec::new(),
        }
    }

    /// Set the client ID.
    #[must_use]
    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    /// Set the client secret.
    #[must_use]
    pub fn with_client_secret(mut self, secret: impl Into<String>) -> Self {
        self.client_secret = Some(secret.into());
        self
    }

    /// Set the redirect URI.
    #[must_use]
    pub fn with_redirect_uri(mut self, uri: impl Into<String>) -> Self {
        self.redirect_uri = Some(uri.into());
        self
    }

    /// Set the target resource (MCP server URL).
    #[must_use]
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Add a scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scopes.push(scope.into());
        self
    }

    /// Check if this is a public client (no client secret).
    #[must_use]
    pub fn is_public_client(&self) -> bool {
        self.client_secret.is_none()
    }
}

/// Dynamic Client Registration request per RFC 7591.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegistrationRequest {
    /// Requested redirect URIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uris: Option<Vec<String>>,
    /// Token endpoint authentication method.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint_auth_method: Option<String>,
    /// Requested grant types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_types: Option<Vec<String>>,
    /// Requested response types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_types: Option<Vec<String>>,
    /// Client name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    /// Client URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,
    /// Software identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_id: Option<String>,
    /// Software version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,
}

impl ClientRegistrationRequest {
    /// Create a new client registration request.
    #[must_use]
    pub fn new() -> Self {
        Self {
            redirect_uris: None,
            token_endpoint_auth_method: Some("none".to_string()), // Public client
            grant_types: Some(vec!["authorization_code".to_string()]),
            response_types: Some(vec!["code".to_string()]),
            client_name: None,
            client_uri: None,
            software_id: None,
            software_version: None,
        }
    }

    /// Set redirect URIs.
    #[must_use]
    pub fn with_redirect_uris(mut self, uris: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.redirect_uris = Some(uris.into_iter().map(Into::into).collect());
        self
    }

    /// Set the client name.
    #[must_use]
    pub fn with_client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = Some(name.into());
        self
    }

    /// Set the software ID.
    #[must_use]
    pub fn with_software_id(mut self, id: impl Into<String>) -> Self {
        self.software_id = Some(id.into());
        self
    }
}

impl Default for ClientRegistrationRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Dynamic Client Registration response per RFC 7591.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegistrationResponse {
    /// The assigned client identifier.
    pub client_id: String,
    /// The client secret (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Client secret expiration time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<u64>,
    /// Client ID issued at time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<u64>,
    /// All other registered metadata echoed back.
    #[serde(flatten)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protected_resource_metadata() {
        let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
            .with_authorization_server("https://auth.example.com")
            .with_scopes(["mcp:read", "mcp:write"]);

        assert_eq!(metadata.resource, "https://mcp.example.com");
        assert_eq!(metadata.authorization_servers.len(), 1);
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn test_protected_resource_metadata_validation() {
        let metadata = ProtectedResourceMetadata::new("https://mcp.example.com");
        assert!(metadata.validate().is_err()); // No auth servers

        let metadata = metadata.with_authorization_server("https://auth.example.com");
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn test_authorization_server_metadata() {
        let metadata = AuthorizationServerMetadata::from_issuer("https://auth.example.com");

        assert_eq!(metadata.issuer, "https://auth.example.com");
        assert_eq!(metadata.authorization_endpoint, "https://auth.example.com/authorize");
        assert_eq!(metadata.token_endpoint, "https://auth.example.com/token");
    }

    #[test]
    fn test_pkce_challenge() {
        let pkce = PkceChallenge::new();

        // Verifier should be different from challenge for S256
        assert_ne!(pkce.verifier, pkce.challenge);

        // Verification should work
        assert!(PkceChallenge::verify(&pkce.verifier, &pkce.challenge, CodeChallengeMethod::S256));

        // Wrong verifier should fail
        assert!(!PkceChallenge::verify("wrong", &pkce.challenge, CodeChallengeMethod::S256));
    }

    #[test]
    fn test_authorization_request() {
        let pkce = PkceChallenge::new();
        let request = AuthorizationRequest::new("client123", &pkce, "https://mcp.example.com")
            .with_redirect_uri("http://localhost:8080/callback")
            .with_scope("mcp:read")
            .with_state("random_state");

        assert_eq!(request.client_id, "client123");
        assert_eq!(request.resource, "https://mcp.example.com");

        let url = request.build_url("https://auth.example.com/authorize");
        assert!(url.is_some());
        let url = url.unwrap();
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client123"));
        assert!(url.contains("resource="));
    }

    #[test]
    fn test_token_request_authorization_code() {
        let request = TokenRequest::authorization_code(
            "auth_code_123",
            "client123",
            "verifier123",
            "https://mcp.example.com",
        );

        assert_eq!(request.grant_type, "authorization_code");
        assert_eq!(request.code, Some("auth_code_123".to_string()));
        assert_eq!(request.code_verifier, Some("verifier123".to_string()));
    }

    #[test]
    fn test_token_request_client_credentials() {
        let request = TokenRequest::client_credentials(
            "client123",
            "secret456",
            "https://mcp.example.com",
        );

        assert_eq!(request.grant_type, "client_credentials");
        assert_eq!(request.client_secret, Some("secret456".to_string()));
    }

    #[test]
    fn test_www_authenticate_header() {
        let header = WwwAuthenticate::new("https://mcp.example.com/.well-known/oauth-protected-resource")
            .with_realm("mcp")
            .with_error(OAuthError::InvalidToken)
            .with_error_description("Token expired");

        let value = header.to_header_value();
        assert!(value.starts_with("Bearer resource_metadata="));
        assert!(value.contains("realm=\"mcp\""));
        assert!(value.contains("error=\"invalid_token\""));
    }

    #[test]
    fn test_www_authenticate_parse() {
        let header_value = "Bearer resource_metadata=\"https://example.com/.well-known/oauth-protected-resource\", realm=\"mcp\"";
        let parsed = WwwAuthenticate::parse(header_value);

        assert!(parsed.is_some());
        let parsed = parsed.unwrap();
        assert_eq!(parsed.resource_metadata, "https://example.com/.well-known/oauth-protected-resource");
        assert_eq!(parsed.realm, Some("mcp".to_string()));
    }

    #[test]
    fn test_authorization_config() {
        let config = AuthorizationConfig::new("https://auth.example.com")
            .with_client_id("my-client")
            .with_resource("https://mcp.example.com")
            .with_scope("mcp:read");

        assert!(config.is_public_client());
        assert_eq!(config.client_id, "my-client");

        let config = config.with_client_secret("secret");
        assert!(!config.is_public_client());
    }

    #[test]
    fn test_stored_token() {
        let token = TokenResponse {
            access_token: "access123".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: Some("refresh456".to_string()),
            scope: Some("mcp:read".to_string()),
        };

        let stored = StoredToken::new(token, "https://mcp.example.com");
        assert!(!stored.is_expired());
        assert_eq!(stored.authorization_header(), "Bearer access123");
    }

    #[test]
    fn test_client_registration_request() {
        let request = ClientRegistrationRequest::new()
            .with_client_name("My MCP Client")
            .with_redirect_uris(["http://localhost:8080/callback"]);

        assert_eq!(request.client_name, Some("My MCP Client".to_string()));
        assert!(request.redirect_uris.is_some());
    }

    #[test]
    fn test_oauth_error_display() {
        assert_eq!(OAuthError::InvalidRequest.to_string(), "invalid_request");
        assert_eq!(OAuthError::InvalidToken.to_string(), "invalid_token");
        assert_eq!(OAuthError::InsufficientScope.to_string(), "insufficient_scope");
    }
}
