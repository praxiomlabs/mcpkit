//! JWT validation helpers for MCP authorization.
//!
//! This module provides JWT (JSON Web Token) validation functionality for MCP
//! servers, including JWKS (JSON Web Key Set) fetching from authorization servers.
//!
//! # Features
//!
//! - JWT signature verification using RS256 and ES256 algorithms
//! - JWKS fetching from authorization server endpoints
//! - Standard claims validation (exp, iat, aud, iss)
//! - Custom scope validation for MCP
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_core::auth::jwt::{validate_token_with_fetch, TokenValidation};
//!
//! // Validate a token by fetching JWKS from the authorization server
//! let validation = TokenValidation::new()
//!     .with_issuer("https://auth.example.com")
//!     .with_audience("https://mcp.example.com");
//!
//! let claims = validate_token_with_fetch(
//!     "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...",
//!     "https://auth.example.com/.well-known/jwks.json",
//!     &validation,
//! ).await?;
//!
//! println!("Token subject: {:?}", claims.sub);
//! ```
//!
//! # Security Considerations
//!
//! - Always validate tokens before granting access to protected resources
//! - Use HTTPS for JWKS endpoints to prevent MITM attacks
//! - Consider implementing JWKS caching in production (not included here)
//! - Validate issuer and audience claims to prevent token confusion attacks

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Errors that can occur during JWT validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum JwtError {
    /// The token format is invalid.
    #[error("invalid token format: {message}")]
    InvalidFormat {
        /// Error message.
        message: String,
    },

    /// The token signature is invalid.
    #[error("invalid signature: {message}")]
    InvalidSignature {
        /// Error message.
        message: String,
    },

    /// The token has expired.
    #[error("token expired")]
    Expired,

    /// The token is not yet valid (nbf claim).
    #[error("token not yet valid")]
    NotYetValid,

    /// The issuer claim is invalid.
    #[error("invalid issuer: expected {expected}, got {actual}")]
    InvalidIssuer {
        /// Expected issuer.
        expected: String,
        /// Actual issuer.
        actual: String,
    },

    /// The audience claim is invalid.
    #[error("invalid audience: expected {expected}")]
    InvalidAudience {
        /// Expected audience.
        expected: String,
    },

    /// A required claim is missing.
    #[error("missing required claim: {claim}")]
    MissingClaim {
        /// The missing claim name.
        claim: String,
    },

    /// The required scope is not present.
    #[error("insufficient scope: required {required}")]
    InsufficientScope {
        /// The required scope.
        required: String,
    },

    /// Failed to fetch JWKS.
    #[error("JWKS fetch failed: {message}")]
    JwksFetchError {
        /// Error message.
        message: String,
    },

    /// No matching key found in JWKS.
    #[error("no matching key found for kid: {kid}")]
    NoMatchingKey {
        /// The key ID.
        kid: String,
    },

    /// The algorithm is not supported.
    #[error("unsupported algorithm: {algorithm}")]
    UnsupportedAlgorithm {
        /// The algorithm.
        algorithm: String,
    },
}

/// JWT validation configuration.
#[derive(Debug, Clone, Default)]
pub struct TokenValidation {
    /// Expected issuer (iss claim).
    pub issuer: Option<String>,
    /// Expected audience (aud claim).
    pub audience: Option<String>,
    /// Required scopes (space-separated in scope claim).
    pub required_scopes: Vec<String>,
    /// Whether to validate expiration.
    pub validate_exp: bool,
    /// Clock skew tolerance in seconds for time validation.
    pub leeway_seconds: u64,
    /// Custom claims that must be present.
    pub required_claims: Vec<String>,
}

impl TokenValidation {
    /// Create a new validation configuration with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            issuer: None,
            audience: None,
            required_scopes: Vec::new(),
            validate_exp: true,
            leeway_seconds: 60, // 1 minute default leeway
            required_claims: Vec::new(),
        }
    }

    /// Set the expected issuer.
    #[must_use]
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Set the expected audience.
    #[must_use]
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// Add a required scope.
    #[must_use]
    pub fn with_required_scope(mut self, scope: impl Into<String>) -> Self {
        self.required_scopes.push(scope.into());
        self
    }

    /// Set multiple required scopes.
    #[must_use]
    pub fn with_required_scopes(
        mut self,
        scopes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.required_scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// Set the clock skew leeway.
    #[must_use]
    pub const fn with_leeway(mut self, seconds: u64) -> Self {
        self.leeway_seconds = seconds;
        self
    }

    /// Disable expiration validation (not recommended).
    #[must_use]
    pub const fn without_exp_validation(mut self) -> Self {
        self.validate_exp = false;
        self
    }

    /// Add a required custom claim.
    #[must_use]
    pub fn with_required_claim(mut self, claim: impl Into<String>) -> Self {
        self.required_claims.push(claim.into());
        self
    }
}

/// Standard JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Issuer (iss).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,

    /// Subject (sub).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,

    /// Audience (aud) - can be string or array.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<Audience>,

    /// Expiration time (exp) - Unix timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<u64>,

    /// Issued at (iat) - Unix timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,

    /// Not before (nbf) - Unix timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<u64>,

    /// JWT ID (jti).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,

    /// OAuth scope claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Additional custom claims.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl TokenClaims {
    /// Check if the token has a specific scope.
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|s| s.split_whitespace().any(|part| part == scope))
    }

    /// Get all scopes as a vector.
    #[must_use]
    pub fn scopes(&self) -> Vec<&str> {
        self.scope
            .as_ref()
            .map(|s| s.split_whitespace().collect())
            .unwrap_or_default()
    }
}

/// Audience claim which can be a single string or array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    /// Single audience.
    Single(String),
    /// Multiple audiences.
    Multiple(Vec<String>),
}

impl Audience {
    /// Check if the audience contains a specific value.
    #[must_use]
    pub fn contains(&self, aud: &str) -> bool {
        match self {
            Self::Single(s) => s == aud,
            Self::Multiple(v) => v.iter().any(|a| a == aud),
        }
    }
}

/// JSON Web Key Set (JWKS) response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwksSet {
    /// The keys in the set.
    pub keys: Vec<Jwk>,
}

impl JwksSet {
    /// Find a key by its ID.
    #[must_use]
    pub fn find_key(&self, kid: &str) -> Option<&Jwk> {
        self.keys.iter().find(|k| k.kid.as_deref() == Some(kid))
    }

    /// Get the first key (useful when there's only one key).
    #[must_use]
    pub fn first_key(&self) -> Option<&Jwk> {
        self.keys.first()
    }
}

/// JSON Web Key (JWK).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    /// Key type (kty) - "RSA" or "EC".
    pub kty: String,

    /// Key ID (kid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,

    /// Key use (use) - "sig" for signing.
    #[serde(rename = "use", skip_serializing_if = "Option::is_none")]
    pub key_use: Option<String>,

    /// Algorithm (alg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alg: Option<String>,

    // RSA parameters
    /// RSA modulus (n).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,

    /// RSA exponent (e).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,

    // EC parameters
    /// EC curve (crv).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crv: Option<String>,

    /// EC x coordinate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,

    /// EC y coordinate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,
}

impl Jwk {
    /// Check if this is an RSA key.
    #[must_use]
    pub fn is_rsa(&self) -> bool {
        self.kty == "RSA"
    }

    /// Check if this is an EC key.
    #[must_use]
    pub fn is_ec(&self) -> bool {
        self.kty == "EC"
    }

    /// Check if this key is for signing.
    #[must_use]
    pub fn is_signing_key(&self) -> bool {
        self.key_use.as_deref() == Some("sig") || self.key_use.is_none()
    }
}

/// JWT header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader {
    /// Algorithm (alg).
    pub alg: String,

    /// Type (typ).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typ: Option<String>,

    /// Key ID (kid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
}

/// Decode JWT header without verification.
///
/// This is useful for extracting the key ID before fetching JWKS.
///
/// # Errors
///
/// Returns `JwtError::InvalidFormat` if the token format is invalid.
pub fn decode_header(token: &str) -> Result<JwtHeader, JwtError> {
    use base64::Engine;

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidFormat {
            message: "token must have 3 parts".to_string(),
        });
    }

    let header_json =
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[0])
            .map_err(|e| JwtError::InvalidFormat {
                message: format!("invalid header encoding: {e}"),
            })?;

    serde_json::from_slice(&header_json).map_err(|e| JwtError::InvalidFormat {
        message: format!("invalid header JSON: {e}"),
    })
}

/// Decode JWT claims without verification.
///
/// **WARNING**: This does not verify the signature! Only use for debugging or
/// when you've already validated the token through other means.
///
/// # Errors
///
/// Returns `JwtError::InvalidFormat` if the token format is invalid.
pub fn decode_claims_unverified(token: &str) -> Result<TokenClaims, JwtError> {
    use base64::Engine;

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidFormat {
            message: "token must have 3 parts".to_string(),
        });
    }

    let payload_json =
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| JwtError::InvalidFormat {
                message: format!("invalid payload encoding: {e}"),
            })?;

    serde_json::from_slice(&payload_json).map_err(|e| JwtError::InvalidFormat {
        message: format!("invalid payload JSON: {e}"),
    })
}

/// Validate claims against the validation configuration.
///
/// This does not verify the signature - use `validate_token` for full validation.
///
/// # Errors
///
/// Returns an error if any validation check fails.
pub fn validate_claims(claims: &TokenClaims, validation: &TokenValidation) -> Result<(), JwtError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Validate expiration (use saturating arithmetic to prevent overflow)
    if validation.validate_exp {
        if let Some(exp) = claims.exp {
            if now > exp.saturating_add(validation.leeway_seconds) {
                return Err(JwtError::Expired);
            }
        }
    }

    // Validate not before (use saturating arithmetic to prevent overflow)
    if let Some(nbf) = claims.nbf {
        if now.saturating_add(validation.leeway_seconds) < nbf {
            return Err(JwtError::NotYetValid);
        }
    }

    // Validate issuer
    if let Some(ref expected_iss) = validation.issuer {
        match &claims.iss {
            Some(actual_iss) if actual_iss != expected_iss => {
                return Err(JwtError::InvalidIssuer {
                    expected: expected_iss.clone(),
                    actual: actual_iss.clone(),
                });
            }
            None => {
                return Err(JwtError::MissingClaim {
                    claim: "iss".to_string(),
                });
            }
            _ => {}
        }
    }

    // Validate audience
    if let Some(ref expected_aud) = validation.audience {
        match &claims.aud {
            Some(aud) if aud.contains(expected_aud) => {}
            Some(_) => {
                return Err(JwtError::InvalidAudience {
                    expected: expected_aud.clone(),
                });
            }
            None => {
                return Err(JwtError::MissingClaim {
                    claim: "aud".to_string(),
                });
            }
        }
    }

    // Validate required scopes
    for required_scope in &validation.required_scopes {
        if !claims.has_scope(required_scope) {
            return Err(JwtError::InsufficientScope {
                required: required_scope.clone(),
            });
        }
    }

    // Validate required custom claims
    for claim_name in &validation.required_claims {
        if !claims.extra.contains_key(claim_name) {
            // Check standard claims too
            let has_claim = match claim_name.as_str() {
                "iss" => claims.iss.is_some(),
                "sub" => claims.sub.is_some(),
                "aud" => claims.aud.is_some(),
                "exp" => claims.exp.is_some(),
                "iat" => claims.iat.is_some(),
                "nbf" => claims.nbf.is_some(),
                "jti" => claims.jti.is_some(),
                "scope" => claims.scope.is_some(),
                _ => false,
            };
            if !has_claim {
                return Err(JwtError::MissingClaim {
                    claim: claim_name.clone(),
                });
            }
        }
    }

    Ok(())
}

/// Supported JWT signing algorithms.
#[cfg(feature = "jwt")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtAlgorithm {
    /// RSA with SHA-256 (most common for OAuth 2.1).
    RS256,
    /// RSA with SHA-384.
    RS384,
    /// RSA with SHA-512.
    RS512,
    /// ECDSA with P-256 curve and SHA-256.
    ES256,
    /// ECDSA with P-384 curve and SHA-384.
    ES384,
}

#[cfg(feature = "jwt")]
impl JwtAlgorithm {
    /// Parse algorithm from JWT header "alg" value.
    #[must_use]
    pub fn from_str(alg: &str) -> Option<Self> {
        match alg {
            "RS256" => Some(Self::RS256),
            "RS384" => Some(Self::RS384),
            "RS512" => Some(Self::RS512),
            "ES256" => Some(Self::ES256),
            "ES384" => Some(Self::ES384),
            _ => None,
        }
    }

    /// Convert to jsonwebtoken Algorithm.
    fn to_jsonwebtoken_algorithm(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::RS256 => jsonwebtoken::Algorithm::RS256,
            Self::RS384 => jsonwebtoken::Algorithm::RS384,
            Self::RS512 => jsonwebtoken::Algorithm::RS512,
            Self::ES256 => jsonwebtoken::Algorithm::ES256,
            Self::ES384 => jsonwebtoken::Algorithm::ES384,
        }
    }
}

#[cfg(feature = "jwt")]
impl std::fmt::Display for JwtAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RS256 => write!(f, "RS256"),
            Self::RS384 => write!(f, "RS384"),
            Self::RS512 => write!(f, "RS512"),
            Self::ES256 => write!(f, "ES256"),
            Self::ES384 => write!(f, "ES384"),
        }
    }
}

/// Create a jsonwebtoken DecodingKey from our Jwk type.
#[cfg(feature = "jwt")]
fn create_decoding_key(jwk: &Jwk, alg: JwtAlgorithm) -> Result<jsonwebtoken::DecodingKey, JwtError> {
    match alg {
        JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 => {
            let n = jwk.n.as_ref().ok_or_else(|| JwtError::InvalidFormat {
                message: "RSA key missing 'n' component".to_string(),
            })?;
            let e = jwk.e.as_ref().ok_or_else(|| JwtError::InvalidFormat {
                message: "RSA key missing 'e' component".to_string(),
            })?;
            jsonwebtoken::DecodingKey::from_rsa_components(n, e).map_err(|e| {
                JwtError::InvalidFormat {
                    message: format!("invalid RSA key: {e}"),
                }
            })
        }
        JwtAlgorithm::ES256 | JwtAlgorithm::ES384 => {
            let x = jwk.x.as_ref().ok_or_else(|| JwtError::InvalidFormat {
                message: "EC key missing 'x' component".to_string(),
            })?;
            let y = jwk.y.as_ref().ok_or_else(|| JwtError::InvalidFormat {
                message: "EC key missing 'y' component".to_string(),
            })?;
            jsonwebtoken::DecodingKey::from_ec_components(x, y).map_err(|e| {
                JwtError::InvalidFormat {
                    message: format!("invalid EC key: {e}"),
                }
            })
        }
    }
}

/// Build jsonwebtoken Validation from our TokenValidation.
#[cfg(feature = "jwt")]
fn build_jwt_validation(
    validation: &TokenValidation,
    alg: JwtAlgorithm,
) -> jsonwebtoken::Validation {
    let mut jwt_validation = jsonwebtoken::Validation::new(alg.to_jsonwebtoken_algorithm());

    // Set leeway for time-based claims
    jwt_validation.leeway = validation.leeway_seconds;

    // Set issuer validation
    if let Some(ref iss) = validation.issuer {
        jwt_validation.set_issuer(&[iss]);
    }

    // Set audience validation
    if let Some(ref aud) = validation.audience {
        jwt_validation.set_audience(&[aud]);
    }

    // Set expiration validation
    jwt_validation.validate_exp = validation.validate_exp;

    // Add required claims
    for claim in &validation.required_claims {
        jwt_validation.set_required_spec_claims(&[claim.as_str()]);
    }

    jwt_validation
}

/// Validate a JWT access token with full signature verification.
///
/// This function verifies the token's cryptographic signature using the provided
/// JWKS and validates the claims against the validation configuration.
///
/// # Arguments
///
/// * `token` - The JWT access token to validate
/// * `jwks` - The JSON Web Key Set containing public keys
/// * `validation` - The validation configuration
///
/// # Algorithm Selection
///
/// The function automatically selects the correct key from the JWKS based on:
/// 1. The `kid` (key ID) in the JWT header, if present
/// 2. The `alg` (algorithm) in the JWT header
///
/// # Supported Algorithms
///
/// - **RS256, RS384, RS512**: RSA with SHA-256/384/512
/// - **ES256, ES384**: ECDSA with P-256/P-384 curves
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_core::auth::jwt::{validate_token, TokenValidation, JwksSet};
///
/// let jwks: JwksSet = fetch_jwks_from_somewhere();
/// let validation = TokenValidation::new()
///     .with_issuer("https://auth.example.com")
///     .with_audience("https://mcp.example.com");
///
/// let claims = validate_token(
///     "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...",
///     &jwks,
///     &validation,
/// )?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The token format is invalid
/// - No matching key is found in the JWKS
/// - The signature verification fails
/// - Claims validation fails
#[cfg(feature = "jwt")]
pub fn validate_token(
    token: &str,
    jwks: &JwksSet,
    validation: &TokenValidation,
) -> Result<TokenClaims, JwtError> {
    // Decode header to get algorithm and key ID
    let header = decode_header(token)?;

    // Parse algorithm
    let alg = JwtAlgorithm::from_str(&header.alg).ok_or_else(|| JwtError::UnsupportedAlgorithm {
        algorithm: header.alg.clone(),
    })?;

    // Find the matching key in JWKS
    let jwk = if let Some(ref kid) = header.kid {
        // If kid is specified, find that specific key
        jwks.find_key(kid).ok_or_else(|| JwtError::NoMatchingKey {
            kid: kid.clone(),
        })?
    } else {
        // No kid specified, try the first signing key with matching algorithm
        jwks.keys
            .iter()
            .find(|k| {
                k.is_signing_key()
                    && k.alg.as_deref() == Some(&header.alg)
            })
            .or_else(|| jwks.first_key())
            .ok_or_else(|| JwtError::NoMatchingKey {
                kid: "<no kid specified>".to_string(),
            })?
    };

    // Create decoding key from JWK
    let decoding_key = create_decoding_key(jwk, alg)?;

    // Build validation configuration
    let jwt_validation = build_jwt_validation(validation, alg);

    // Decode and verify the token
    let token_data =
        jsonwebtoken::decode::<TokenClaims>(token, &decoding_key, &jwt_validation).map_err(
            |e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::Expired,
                jsonwebtoken::errors::ErrorKind::ImmatureSignature => JwtError::NotYetValid,
                jsonwebtoken::errors::ErrorKind::InvalidSignature => JwtError::InvalidSignature {
                    message: "signature verification failed".to_string(),
                },
                jsonwebtoken::errors::ErrorKind::InvalidAudience => JwtError::InvalidAudience {
                    expected: validation.audience.clone().unwrap_or_default(),
                },
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => JwtError::InvalidIssuer {
                    expected: validation.issuer.clone().unwrap_or_default(),
                    actual: "<unknown>".to_string(),
                },
                _ => JwtError::InvalidFormat {
                    message: format!("token validation failed: {e}"),
                },
            },
        )?;

    // Additional scope validation (jsonwebtoken doesn't handle this)
    let claims = token_data.claims;
    for required_scope in &validation.required_scopes {
        if !claims.has_scope(required_scope) {
            return Err(JwtError::InsufficientScope {
                required: required_scope.clone(),
            });
        }
    }

    Ok(claims)
}

/// Fetch JWKS from an authorization server's JWKS endpoint.
///
/// This function makes an HTTP GET request to the JWKS URI and parses the
/// response as a JSON Web Key Set.
///
/// # Arguments
///
/// * `jwks_uri` - The URL of the JWKS endpoint (typically from authorization
///   server metadata's `jwks_uri` field)
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_core::auth::jwt::fetch_jwks;
///
/// let jwks = fetch_jwks("https://auth.example.com/.well-known/jwks.json").await?;
/// if let Some(key) = jwks.find_key("my-key-id") {
///     println!("Found key: {:?}", key);
/// }
/// ```
///
/// # Errors
///
/// Returns `JwtError::JwksFetchError` if:
/// - The HTTP request fails
/// - The response cannot be parsed as a JWKS
#[cfg(feature = "jwt")]
pub async fn fetch_jwks(jwks_uri: &str) -> Result<JwksSet, JwtError> {
    let response = reqwest::get(jwks_uri).await.map_err(|e| JwtError::JwksFetchError {
        message: format!("HTTP request failed: {e}"),
    })?;

    if !response.status().is_success() {
        return Err(JwtError::JwksFetchError {
            message: format!("HTTP {} from JWKS endpoint", response.status()),
        });
    }

    response.json().await.map_err(|e| JwtError::JwksFetchError {
        message: format!("Failed to parse JWKS response: {e}"),
    })
}

/// Validate a JWT access token with full signature verification by fetching
/// JWKS from the authorization server.
///
/// This is a convenience function that:
/// 1. Fetches the JWKS from the authorization server
/// 2. Verifies the token's cryptographic signature using the public keys
/// 3. Validates the claims against the validation configuration
///
/// # Arguments
///
/// * `token` - The JWT access token to validate
/// * `jwks_uri` - The URL of the JWKS endpoint
/// * `validation` - The validation configuration
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_core::auth::jwt::{validate_token_with_fetch, TokenValidation};
///
/// let validation = TokenValidation::new()
///     .with_issuer("https://auth.example.com")
///     .with_audience("https://mcp.example.com");
///
/// let claims = validate_token_with_fetch(
///     "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...",
///     "https://auth.example.com/.well-known/jwks.json",
///     &validation,
/// ).await?;
///
/// println!("Token subject: {:?}", claims.sub);
/// ```
///
/// # Security
///
/// This function performs full cryptographic signature verification using
/// the public keys from the JWKS endpoint. It supports RS256, RS384, RS512,
/// ES256, and ES384 algorithms.
///
/// For production use, consider implementing JWKS caching to avoid fetching
/// keys on every token validation.
///
/// # Errors
///
/// Returns an error if:
/// - JWKS fetching fails
/// - No matching key is found in the JWKS
/// - Signature verification fails
/// - Claims validation fails
#[cfg(feature = "jwt")]
pub async fn validate_token_with_fetch(
    token: &str,
    jwks_uri: &str,
    validation: &TokenValidation,
) -> Result<TokenClaims, JwtError> {
    // Fetch JWKS from the authorization server
    let jwks = fetch_jwks(jwks_uri).await?;

    // Validate token with signature verification
    validate_token(token, &jwks, validation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_validation_builder() {
        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com")
            .with_required_scope("mcp:read")
            .with_leeway(120);

        assert_eq!(validation.issuer, Some("https://auth.example.com".to_string()));
        assert_eq!(
            validation.audience,
            Some("https://mcp.example.com".to_string())
        );
        assert_eq!(validation.required_scopes, vec!["mcp:read"]);
        assert_eq!(validation.leeway_seconds, 120);
    }

    #[test]
    fn test_audience_contains() {
        let single = Audience::Single("api".to_string());
        assert!(single.contains("api"));
        assert!(!single.contains("other"));

        let multiple = Audience::Multiple(vec!["api".to_string(), "web".to_string()]);
        assert!(multiple.contains("api"));
        assert!(multiple.contains("web"));
        assert!(!multiple.contains("other"));
    }

    #[test]
    fn test_token_claims_scopes() {
        let claims = TokenClaims {
            iss: None,
            sub: None,
            aud: None,
            exp: None,
            iat: None,
            nbf: None,
            jti: None,
            scope: Some("mcp:read mcp:write".to_string()),
            extra: HashMap::new(),
        };

        assert!(claims.has_scope("mcp:read"));
        assert!(claims.has_scope("mcp:write"));
        assert!(!claims.has_scope("mcp:admin"));

        let scopes = claims.scopes();
        assert_eq!(scopes, vec!["mcp:read", "mcp:write"]);
    }

    #[test]
    fn test_decode_header() {
        // A minimal JWT header: {"alg":"RS256","typ":"JWT","kid":"key-1"}
        let token = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6ImtleS0xIn0.e30.sig";
        let header = decode_header(token).unwrap();

        assert_eq!(header.alg, "RS256");
        assert_eq!(header.typ, Some("JWT".to_string()));
        assert_eq!(header.kid, Some("key-1".to_string()));
    }

    #[test]
    fn test_decode_header_invalid() {
        assert!(decode_header("invalid").is_err());
        assert!(decode_header("a.b").is_err());
        assert!(decode_header("").is_err());
    }

    #[test]
    fn test_decode_claims_unverified() {
        // JWT with claims: {"iss":"test","sub":"user123","exp":9999999999}
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJ0ZXN0Iiwic3ViIjoidXNlcjEyMyIsImV4cCI6OTk5OTk5OTk5OX0.sig";
        let claims = decode_claims_unverified(token).unwrap();

        assert_eq!(claims.iss, Some("test".to_string()));
        assert_eq!(claims.sub, Some("user123".to_string()));
        assert_eq!(claims.exp, Some(9_999_999_999));
    }

    #[test]
    fn test_validate_claims_expired() {
        let claims = TokenClaims {
            iss: None,
            sub: None,
            aud: None,
            exp: Some(1000), // Long expired
            iat: None,
            nbf: None,
            jti: None,
            scope: None,
            extra: HashMap::new(),
        };

        let validation = TokenValidation::new();
        let result = validate_claims(&claims, &validation);
        assert!(matches!(result, Err(JwtError::Expired)));
    }

    #[test]
    fn test_validate_claims_wrong_issuer() {
        let claims = TokenClaims {
            iss: Some("wrong-issuer".to_string()),
            sub: None,
            aud: None,
            exp: Some(u64::MAX),
            iat: None,
            nbf: None,
            jti: None,
            scope: None,
            extra: HashMap::new(),
        };

        let validation = TokenValidation::new().with_issuer("expected-issuer");
        let result = validate_claims(&claims, &validation);
        assert!(matches!(result, Err(JwtError::InvalidIssuer { .. })));
    }

    #[test]
    fn test_validate_claims_insufficient_scope() {
        let claims = TokenClaims {
            iss: None,
            sub: None,
            aud: None,
            exp: Some(u64::MAX),
            iat: None,
            nbf: None,
            jti: None,
            scope: Some("mcp:read".to_string()),
            extra: HashMap::new(),
        };

        let validation = TokenValidation::new()
            .without_exp_validation()
            .with_required_scope("mcp:admin");
        let result = validate_claims(&claims, &validation);
        assert!(matches!(result, Err(JwtError::InsufficientScope { .. })));
    }

    #[test]
    fn test_validate_claims_success() {
        let claims = TokenClaims {
            iss: Some("https://auth.example.com".to_string()),
            sub: Some("user123".to_string()),
            aud: Some(Audience::Single("https://mcp.example.com".to_string())),
            exp: Some(u64::MAX),
            iat: Some(1000),
            nbf: None,
            jti: None,
            scope: Some("mcp:read mcp:write".to_string()),
            extra: HashMap::new(),
        };

        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com")
            .with_required_scope("mcp:read");

        let result = validate_claims(&claims, &validation);
        assert!(result.is_ok());
    }

    #[test]
    fn test_jwk_type_detection() {
        let rsa_key = Jwk {
            kty: "RSA".to_string(),
            kid: Some("rsa-key".to_string()),
            key_use: Some("sig".to_string()),
            alg: Some("RS256".to_string()),
            n: Some("...".to_string()),
            e: Some("AQAB".to_string()),
            crv: None,
            x: None,
            y: None,
        };

        assert!(rsa_key.is_rsa());
        assert!(!rsa_key.is_ec());
        assert!(rsa_key.is_signing_key());

        let ec_key = Jwk {
            kty: "EC".to_string(),
            kid: Some("ec-key".to_string()),
            key_use: Some("sig".to_string()),
            alg: Some("ES256".to_string()),
            n: None,
            e: None,
            crv: Some("P-256".to_string()),
            x: Some("...".to_string()),
            y: Some("...".to_string()),
        };

        assert!(!ec_key.is_rsa());
        assert!(ec_key.is_ec());
        assert!(ec_key.is_signing_key());
    }

    #[test]
    fn test_jwks_find_key() {
        let jwks = JwksSet {
            keys: vec![
                Jwk {
                    kty: "RSA".to_string(),
                    kid: Some("key-1".to_string()),
                    key_use: Some("sig".to_string()),
                    alg: Some("RS256".to_string()),
                    n: Some("...".to_string()),
                    e: Some("AQAB".to_string()),
                    crv: None,
                    x: None,
                    y: None,
                },
                Jwk {
                    kty: "RSA".to_string(),
                    kid: Some("key-2".to_string()),
                    key_use: Some("sig".to_string()),
                    alg: Some("RS256".to_string()),
                    n: Some("...".to_string()),
                    e: Some("AQAB".to_string()),
                    crv: None,
                    x: None,
                    y: None,
                },
            ],
        };

        let key = jwks.find_key("key-1");
        assert!(key.is_some());
        assert_eq!(key.unwrap().kid, Some("key-1".to_string()));

        let key = jwks.find_key("key-2");
        assert!(key.is_some());

        let key = jwks.find_key("nonexistent");
        assert!(key.is_none());

        let first = jwks.first_key();
        assert!(first.is_some());
    }

    #[test]
    fn test_jwt_error_display() {
        let err = JwtError::Expired;
        assert_eq!(err.to_string(), "token expired");

        let err = JwtError::InvalidIssuer {
            expected: "a".to_string(),
            actual: "b".to_string(),
        };
        assert!(err.to_string().contains("expected a"));
        assert!(err.to_string().contains("got b"));
    }
}

/// Tests that require the `jwt` feature for signature verification.
#[cfg(all(test, feature = "jwt"))]
mod signature_tests {
    use super::*;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::traits::PublicKeyParts;

    /// Create a signed JWT for testing using jsonwebtoken's encode function.
    fn create_test_jwt(
        alg: jsonwebtoken::Algorithm,
        encoding_key: &jsonwebtoken::EncodingKey,
        kid: Option<&str>,
        claims: &TokenClaims,
    ) -> String {
        let mut header = jsonwebtoken::Header::new(alg);
        header.kid = kid.map(String::from);
        jsonwebtoken::encode(&header, claims, encoding_key).expect("failed to encode JWT")
    }

    /// Create test claims with far-future expiration.
    fn make_test_claims() -> TokenClaims {
        TokenClaims {
            iss: Some("https://auth.example.com".to_string()),
            sub: Some("user123".to_string()),
            aud: Some(Audience::Single("https://mcp.example.com".to_string())),
            exp: Some(u64::MAX / 2), // Far future but not overflow
            iat: Some(1_000_000),
            nbf: None,
            jti: Some("test-jti-123".to_string()),
            scope: Some("mcp:read mcp:write".to_string()),
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_jwt_algorithm_from_str() {
        assert_eq!(JwtAlgorithm::from_str("RS256"), Some(JwtAlgorithm::RS256));
        assert_eq!(JwtAlgorithm::from_str("RS384"), Some(JwtAlgorithm::RS384));
        assert_eq!(JwtAlgorithm::from_str("RS512"), Some(JwtAlgorithm::RS512));
        assert_eq!(JwtAlgorithm::from_str("ES256"), Some(JwtAlgorithm::ES256));
        assert_eq!(JwtAlgorithm::from_str("ES384"), Some(JwtAlgorithm::ES384));
        assert_eq!(JwtAlgorithm::from_str("HS256"), None);
        assert_eq!(JwtAlgorithm::from_str("invalid"), None);
    }

    #[test]
    fn test_jwt_algorithm_display() {
        assert_eq!(JwtAlgorithm::RS256.to_string(), "RS256");
        assert_eq!(JwtAlgorithm::ES256.to_string(), "ES256");
        assert_eq!(JwtAlgorithm::RS512.to_string(), "RS512");
    }

    #[test]
    fn test_validate_token_rs256() {
        // Generate RSA key pair for testing
        use rand::rngs::OsRng;
        use rsa::RsaPrivateKey;
        use base64::Engine;

        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key");
        let public_key = private_key.to_public_key();

        // Get modulus and exponent for JWKS
        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&e_bytes);

        // Create JWKS with the public key
        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                kid: Some("test-key-1".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("RS256".to_string()),
                n: Some(n),
                e: Some(e),
                crv: None,
                x: None,
                y: None,
            }],
        };

        // Create encoding key from private key
        let pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("failed to encode private key");
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
            .expect("failed to create encoding key");

        // Create and sign a token
        let claims = make_test_claims();
        let token = create_test_jwt(
            jsonwebtoken::Algorithm::RS256,
            &encoding_key,
            Some("test-key-1"),
            &claims,
        );

        // Validate the token
        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com");

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_ok(), "validation failed: {:?}", result.err());

        let validated_claims = result.unwrap();
        assert_eq!(validated_claims.sub, Some("user123".to_string()));
        assert_eq!(validated_claims.iss, Some("https://auth.example.com".to_string()));
    }

    #[test]
    fn test_validate_token_es256() {
        use p256::ecdsa::{SigningKey, VerifyingKey};
        use p256::elliptic_curve::sec1::ToEncodedPoint;
        use p256::pkcs8::EncodePrivateKey as EcEncodePrivateKey;
        use rand::rngs::OsRng;
        use base64::Engine;

        // Generate EC P-256 key pair
        let signing_key = SigningKey::random(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);

        // Get x and y coordinates for JWKS (uncompressed point)
        let point = verifying_key.as_affine().to_encoded_point(false);
        let x_bytes = point.x().expect("x coordinate");
        let y_bytes = point.y().expect("y coordinate");
        let x = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(x_bytes);
        let y = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(y_bytes);

        // Create JWKS with the public key
        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "EC".to_string(),
                kid: Some("test-ec-key-1".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("ES256".to_string()),
                n: None,
                e: None,
                crv: Some("P-256".to_string()),
                x: Some(x),
                y: Some(y),
            }],
        };

        // Create encoding key from private key
        let pkcs8_der = signing_key.to_pkcs8_der().expect("failed to encode EC key");
        let encoding_key = jsonwebtoken::EncodingKey::from_ec_der(pkcs8_der.as_bytes());

        // Create and sign a token
        let claims = make_test_claims();
        let token = create_test_jwt(
            jsonwebtoken::Algorithm::ES256,
            &encoding_key,
            Some("test-ec-key-1"),
            &claims,
        );

        // Validate the token
        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com");

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_ok(), "ES256 validation failed: {:?}", result.err());

        let validated_claims = result.unwrap();
        assert_eq!(validated_claims.sub, Some("user123".to_string()));
    }

    #[test]
    fn test_validate_token_invalid_signature() {
        use rand::rngs::OsRng;
        use rsa::RsaPrivateKey;
        use base64::Engine;

        let mut rng = OsRng;

        // Generate two different key pairs
        let private_key_1 = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key 1");
        let private_key_2 = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key 2");
        let public_key_2 = private_key_2.to_public_key();

        // Create JWKS with public key from key pair 2
        let n_bytes = public_key_2.n().to_bytes_be();
        let e_bytes = public_key_2.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&e_bytes);

        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                kid: Some("key-2".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("RS256".to_string()),
                n: Some(n),
                e: Some(e),
                crv: None,
                x: None,
                y: None,
            }],
        };

        // Sign token with key pair 1 (different from JWKS)
        let pem_1 = private_key_1
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("failed to encode private key");
        let encoding_key_1 = jsonwebtoken::EncodingKey::from_rsa_pem(pem_1.as_bytes())
            .expect("failed to create encoding key");

        let claims = make_test_claims();
        let token = create_test_jwt(
            jsonwebtoken::Algorithm::RS256,
            &encoding_key_1,
            Some("key-2"), // Use kid from JWKS but signed with different key
            &claims,
        );

        // Validation should fail due to signature mismatch
        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com");

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::InvalidSignature { .. }));
    }

    #[test]
    fn test_validate_token_no_matching_key() {
        // Create JWKS with a key that has a different kid
        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                kid: Some("different-key".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("RS256".to_string()),
                n: Some("test".to_string()),
                e: Some("AQAB".to_string()),
                crv: None,
                x: None,
                y: None,
            }],
        };

        // Create a token with header specifying kid that doesn't exist in JWKS
        // Header: {"alg":"RS256","kid":"nonexistent-key"}
        let token = "eyJhbGciOiJSUzI1NiIsImtpZCI6Im5vbmV4aXN0ZW50LWtleSJ9.eyJpc3MiOiJ0ZXN0In0.sig";

        let validation = TokenValidation::new();
        let result = validate_token(token, &jwks, &validation);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::NoMatchingKey { .. }));
    }

    #[test]
    fn test_validate_token_unsupported_algorithm() {
        let jwks = JwksSet { keys: vec![] };

        // Token with HS256 algorithm (not supported)
        // Header: {"alg":"HS256"}
        let token = "eyJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJ0ZXN0In0.sig";

        let validation = TokenValidation::new();
        let result = validate_token(token, &jwks, &validation);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::UnsupportedAlgorithm { .. }));
    }

    #[test]
    fn test_validate_token_scope_validation() {
        use rand::rngs::OsRng;
        use rsa::RsaPrivateKey;
        use base64::Engine;

        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key");
        let public_key = private_key.to_public_key();

        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&e_bytes);

        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                kid: Some("test-key".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("RS256".to_string()),
                n: Some(n),
                e: Some(e),
                crv: None,
                x: None,
                y: None,
            }],
        };

        let pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("failed to encode private key");
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
            .expect("failed to create encoding key");

        // Create claims with limited scope
        let mut claims = make_test_claims();
        claims.scope = Some("mcp:read".to_string()); // Only read scope

        let token = create_test_jwt(
            jsonwebtoken::Algorithm::RS256,
            &encoding_key,
            Some("test-key"),
            &claims,
        );

        // Request a scope that isn't present
        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com")
            .with_required_scope("mcp:admin"); // Not in token

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::InsufficientScope { .. }));
    }

    #[test]
    fn test_validate_token_expired() {
        use rand::rngs::OsRng;
        use rsa::RsaPrivateKey;
        use base64::Engine;

        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key");
        let public_key = private_key.to_public_key();

        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&e_bytes);

        let jwks = JwksSet {
            keys: vec![Jwk {
                kty: "RSA".to_string(),
                kid: Some("test-key".to_string()),
                key_use: Some("sig".to_string()),
                alg: Some("RS256".to_string()),
                n: Some(n),
                e: Some(e),
                crv: None,
                x: None,
                y: None,
            }],
        };

        let pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("failed to encode private key");
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
            .expect("failed to create encoding key");

        // Create expired claims
        let mut claims = make_test_claims();
        claims.exp = Some(1000); // Expired long ago

        let token = create_test_jwt(
            jsonwebtoken::Algorithm::RS256,
            &encoding_key,
            Some("test-key"),
            &claims,
        );

        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com");

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::Expired));
    }

    #[test]
    fn test_validate_token_key_selection_by_algorithm() {
        use rand::rngs::OsRng;
        use rsa::RsaPrivateKey;
        use base64::Engine;

        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("failed to generate key");
        let public_key = private_key.to_public_key();

        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let n = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&n_bytes);
        let e = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&e_bytes);

        // JWKS with multiple keys - one without kid
        let jwks = JwksSet {
            keys: vec![
                Jwk {
                    kty: "RSA".to_string(),
                    kid: None, // No kid
                    key_use: Some("sig".to_string()),
                    alg: Some("RS256".to_string()),
                    n: Some(n),
                    e: Some(e),
                    crv: None,
                    x: None,
                    y: None,
                },
            ],
        };

        let pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("failed to encode private key");
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes())
            .expect("failed to create encoding key");

        // Create token without kid in header
        let claims = make_test_claims();
        let token = create_test_jwt(
            jsonwebtoken::Algorithm::RS256,
            &encoding_key,
            None, // No kid in token
            &claims,
        );

        let validation = TokenValidation::new()
            .with_issuer("https://auth.example.com")
            .with_audience("https://mcp.example.com");

        let result = validate_token(&token, &jwks, &validation);
        assert!(result.is_ok(), "key selection by algorithm failed: {:?}", result.err());
    }

    #[test]
    fn test_create_decoding_key_missing_rsa_components() {
        let incomplete_jwk = Jwk {
            kty: "RSA".to_string(),
            kid: Some("incomplete".to_string()),
            key_use: Some("sig".to_string()),
            alg: Some("RS256".to_string()),
            n: None, // Missing!
            e: Some("AQAB".to_string()),
            crv: None,
            x: None,
            y: None,
        };

        let result = create_decoding_key(&incomplete_jwk, JwtAlgorithm::RS256);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::InvalidFormat { .. }));
    }

    #[test]
    fn test_create_decoding_key_missing_ec_components() {
        let incomplete_jwk = Jwk {
            kty: "EC".to_string(),
            kid: Some("incomplete-ec".to_string()),
            key_use: Some("sig".to_string()),
            alg: Some("ES256".to_string()),
            n: None,
            e: None,
            crv: Some("P-256".to_string()),
            x: Some("test".to_string()),
            y: None, // Missing!
        };

        let result = create_decoding_key(&incomplete_jwk, JwtAlgorithm::ES256);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), JwtError::InvalidFormat { .. }));
    }
}
