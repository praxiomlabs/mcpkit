//! OAuth 2.1 Authorization for MCP.
//!
//! This module implements OAuth 2.1 authorization per the MCP specification.
//! MCP uses OAuth 2.1 with PKCE for secure authorization of clients to MCP servers.
//!
//! # Features
//!
//! - **Protected Resource Metadata** (RFC 9728): Allows clients to discover
//!   which authorization servers can issue tokens for an MCP server.
//!
//! - **Authorization Server Metadata** (RFC 8414): Standard OAuth 2.1 metadata
//!   for discovering authorization endpoints.
//!
//! - **PKCE** (RFC 7636): Proof Key for Code Exchange, required by MCP for all
//!   authorization code flows.
//!
//! - **Dynamic Client Registration** (RFC 7591): Allows new clients to register
//!   with an authorization server automatically.
//!
//! - **JWT Validation** (optional, requires `jwt` feature): Validate JWT access
//!   tokens using JWKS from authorization servers.
//!
//! # Authorization Flow
//!
//! 1. Client discovers protected resource metadata from MCP server
//! 2. Client discovers authorization server metadata
//! 3. Client registers (if needed) with the authorization server
//! 4. Client performs authorization code flow with PKCE
//! 5. Client receives access token for the MCP server
//! 6. Client includes token in `Authorization: Bearer <token>` header
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::auth::{
//!     ProtectedResourceMetadata, AuthorizationServerMetadata,
//!     PkceChallenge, AuthorizationRequest, TokenRequest,
//! };
//!
//! // Create protected resource metadata for an MCP server
//! let resource_meta = ProtectedResourceMetadata::new("https://mcp.example.com")
//!     .with_authorization_server("https://auth.example.com")
//!     .with_scopes(["mcp:read", "mcp:write"]);
//!
//! // Generate PKCE challenge for authorization
//! let pkce = PkceChallenge::new();
//!
//! // Build authorization request
//! let auth_request = AuthorizationRequest::new(
//!     "my-client-id",
//!     &pkce,
//!     "https://mcp.example.com",
//! )
//! .with_scope("mcp:read mcp:write")
//! .with_redirect_uri("http://localhost:8080/callback");
//!
//! // Get the authorization URL
//! let auth_url = auth_request.build_url("https://auth.example.com/authorize");
//! ```

pub mod identity;
mod oauth;

#[cfg(feature = "jwt")]
pub mod jwt;

pub use identity::{SessionBindingError, VerifiedUser, check_session_binding};

// Re-export all OAuth types
pub use oauth::{
    AuthorizationConfig, AuthorizationRequest, AuthorizationServerMetadata,
    ClientRegistrationRequest, ClientRegistrationResponse, CodeChallengeMethod, GrantType,
    OAuthError, OAuthErrorResponse, PkceChallenge, ProtectedResourceMetadata, StoredToken,
    TokenRequest, TokenResponse, WwwAuthenticate,
};
