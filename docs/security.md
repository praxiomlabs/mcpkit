# Security Guide

This document describes security considerations, threat models, and best practices for implementing MCP servers and clients using the Rust MCP SDK.

## Overview

The Model Context Protocol (MCP) enables AI systems to interact with external services and data sources. This powerful capability requires careful security consideration to prevent unauthorized access, data leakage, and abuse.

## Threat Model

### Attack Surface

1. **Transport Layer**
   - Network interception (MITM attacks)
   - DNS rebinding attacks (WebSocket)
   - Connection hijacking
   - Denial of service (message flooding)

2. **Protocol Layer**
   - Malformed JSON-RPC messages
   - Message size attacks (memory exhaustion)
   - Request ID collision/prediction
   - Parameter injection

3. **Application Layer**
   - Tool injection attacks
   - Resource path traversal
   - Prompt injection via tool results
   - Capability escalation

4. **Authentication/Authorization**
   - Session hijacking
   - Credential theft
   - Insufficient access controls
   - Cross-tenant data access

### Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                         AI System (LLM)                         │
│  - May be manipulated through prompt injection                  │
│  - Should not be trusted with sensitive operations blindly      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ MCP Protocol
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        MCP Client                               │
│  - Validates server responses                                   │
│  - Enforces capability restrictions                             │
│  - May implement approval flows                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Transport (stdio/HTTP/WebSocket)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        MCP Server                               │
│  - Validates all inputs                                         │
│  - Enforces authorization                                       │
│  - Implements rate limiting                                     │
│  - Sanitizes outputs                                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Internal APIs/Database
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Backend Systems                              │
│  - Databases, APIs, file systems                                │
│  - Should enforce their own access controls                     │
└─────────────────────────────────────────────────────────────────┘
```

## Security Controls

### 1. Transport Security

#### TLS Configuration

Always use TLS for network transports. The SDK uses `rustls` by default, avoiding OpenSSL vulnerabilities.

##### Basic TLS (WebSocket)

```rust
use mcpkit_transport::websocket::WebSocketConfig;

// wss:// automatically enables TLS
let config = WebSocketConfig::new("wss://example.com/mcp");
```

##### Basic TLS (HTTP)

```rust
use mcpkit_transport::http::HttpTransportConfig;

// https:// automatically enables TLS
let config = HttpTransportConfig::new("https://example.com/mcp");
```

##### Custom TLS Configuration

For advanced TLS settings, configure the TLS connector directly:

```rust
use mcpkit_transport::websocket::WebSocketConfig;
use rustls::ClientConfig;
use std::sync::Arc;

// Create custom rustls config
let mut tls_config = ClientConfig::builder()
    .with_safe_defaults()
    .with_root_certificates(get_root_certs())
    .with_no_client_auth();

// Optionally configure ALPN protocols
tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

// Apply to transport
let config = WebSocketConfig::new("wss://example.com/mcp")
    .with_tls_config(Arc::new(tls_config));
```

##### Loading Custom Root Certificates

```rust
use rustls::{RootCertStore, Certificate};
use std::fs::File;
use std::io::BufReader;

fn load_root_certs(path: &str) -> RootCertStore {
    let mut roots = RootCertStore::empty();

    // Load system certs
    roots.add_trust_anchors(
        webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
    );

    // Add custom CA certificate
    let file = File::open(path).expect("Cannot open CA file");
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .expect("Cannot read certs");

    for cert in certs {
        roots.add(&Certificate(cert)).expect("Cannot add cert");
    }

    roots
}
```

##### Mutual TLS (mTLS)

For client certificate authentication:

```rust
use rustls::{ClientConfig, Certificate, PrivateKey};
use std::sync::Arc;

fn create_mtls_config(
    client_cert_path: &str,
    client_key_path: &str,
    ca_cert_path: &str,
) -> ClientConfig {
    // Load CA certs
    let roots = load_root_certs(ca_cert_path);

    // Load client certificate chain
    let cert_file = File::open(client_cert_path).unwrap();
    let certs: Vec<Certificate> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .unwrap()
        .into_iter()
        .map(Certificate)
        .collect();

    // Load client private key
    let key_file = File::open(client_key_path).unwrap();
    let keys: Vec<PrivateKey> = rustls_pemfile::pkcs8_private_keys(&mut BufReader::new(key_file))
        .unwrap()
        .into_iter()
        .map(PrivateKey)
        .collect();

    let key = keys.into_iter().next().expect("No private key found");

    // Build config with client auth
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_client_auth_cert(certs, key)
        .expect("Invalid client cert/key")
}

// Use with transport
let tls_config = Arc::new(create_mtls_config(
    "client.crt",
    "client.key",
    "ca.crt",
));
let config = WebSocketConfig::new("wss://secure.example.com/mcp")
    .with_tls_config(tls_config);
```

##### Certificate Pinning

For high-security deployments, pin to specific certificates or public keys:

```rust
use rustls::{ClientConfig, ServerCertVerifier, ServerCertVerified};
use rustls::client::{ServerCertVerifierBuilder, WebPkiServerVerifier};
use sha2::{Sha256, Digest};
use std::sync::Arc;

/// Custom certificate verifier that pins to specific certificate hashes
struct PinnedCertVerifier {
    /// SHA-256 hashes of pinned certificates (DER-encoded)
    pinned_certs: Vec<[u8; 32]>,
    /// SHA-256 hashes of pinned public keys (SPKI)
    pinned_keys: Vec<[u8; 32]>,
    /// Fallback to normal verification
    fallback: Arc<WebPkiServerVerifier>,
}

impl PinnedCertVerifier {
    pub fn new(
        pinned_certs: Vec<[u8; 32]>,
        pinned_keys: Vec<[u8; 32]>,
        roots: RootCertStore,
    ) -> Self {
        let fallback = WebPkiServerVerifier::builder(Arc::new(roots))
            .build()
            .expect("Cannot build verifier");

        Self {
            pinned_certs,
            pinned_keys,
            fallback,
        }
    }

    /// Pin to a certificate by its SHA-256 hash
    pub fn pin_certificate(cert_der: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(cert_der);
        hasher.finalize().into()
    }

    /// Pin to a public key by its SPKI SHA-256 hash
    pub fn pin_public_key(spki: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(spki);
        hasher.finalize().into()
    }
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &Certificate,
        intermediates: &[Certificate],
        server_name: &ServerName,
        ocsp_response: &[u8],
        now: SystemTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // First verify the chain normally
        self.fallback.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        )?;

        // Then check certificate pin
        let cert_hash: [u8; 32] = {
            let mut hasher = Sha256::new();
            hasher.update(&end_entity.0);
            hasher.finalize().into()
        };

        if self.pinned_certs.contains(&cert_hash) {
            return Ok(ServerCertVerified::assertion());
        }

        // Check public key pin (requires parsing the certificate)
        // ... SPKI extraction and comparison ...

        if !self.pinned_certs.is_empty() || !self.pinned_keys.is_empty() {
            return Err(rustls::Error::General(
                "Certificate does not match any pinned certificate or key".into()
            ));
        }

        Ok(ServerCertVerified::assertion())
    }
}

// Usage
fn create_pinned_config() -> ClientConfig {
    let roots = load_root_certs();

    // Pin to specific certificate (get hash from cert file)
    let pinned_cert_hash = [
        0x12, 0x34, 0x56, 0x78, // ... full SHA-256 hash
        // ...
    ];

    let verifier = Arc::new(PinnedCertVerifier::new(
        vec![pinned_cert_hash],
        vec![], // No public key pins
        roots,
    ));

    ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth()
}
```

##### TLS Version Requirements

```rust
use rustls::{ClientConfig, version};

// Require TLS 1.3 only
let config = ClientConfig::builder()
    .with_safe_default_cipher_suites()
    .with_safe_default_kx_groups()
    .with_protocol_versions(&[&version::TLS13])
    .expect("TLS 1.3 not supported")
    .with_root_certificates(roots)
    .with_no_client_auth();
```

##### Cipher Suite Configuration

```rust
use rustls::{ClientConfig, cipher_suite};

// Only use specific cipher suites
let cipher_suites = vec![
    cipher_suite::TLS13_AES_256_GCM_SHA384,
    cipher_suite::TLS13_AES_128_GCM_SHA256,
    cipher_suite::TLS13_CHACHA20_POLY1305_SHA256,
];

let config = ClientConfig::builder()
    .with_cipher_suites(&cipher_suites)
    .with_safe_default_kx_groups()
    .with_safe_default_protocol_versions()
    .expect("Invalid config")
    .with_root_certificates(roots)
    .with_no_client_auth();
```

##### Server-Side TLS (for HTTP/WebSocket servers)

```rust
use rustls::{ServerConfig, Certificate, PrivateKey};
use std::sync::Arc;

fn create_server_tls_config(
    cert_path: &str,
    key_path: &str,
) -> Arc<ServerConfig> {
    // Load certificate chain
    let cert_file = File::open(cert_path).unwrap();
    let certs: Vec<Certificate> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .unwrap()
        .into_iter()
        .map(Certificate)
        .collect();

    // Load private key
    let key_file = File::open(key_path).unwrap();
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))
        .unwrap()
        .expect("No private key")
        .into();

    Arc::new(
        ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()  // or with_client_cert_verifier() for mTLS
            .with_single_cert(certs, key)
            .expect("Invalid cert/key")
    )
}

// Use with WebSocket listener
let tls_config = create_server_tls_config("server.crt", "server.key");
let listener = WebSocketListener::bind_tls("0.0.0.0:9443", tls_config).await?;
```

##### Certificate Rotation

For zero-downtime certificate rotation:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

struct RotatingTlsConfig {
    config: RwLock<Arc<ServerConfig>>,
}

impl RotatingTlsConfig {
    pub fn new(initial_config: ServerConfig) -> Self {
        Self {
            config: RwLock::new(Arc::new(initial_config)),
        }
    }

    pub async fn rotate(&self, new_cert_path: &str, new_key_path: &str) {
        let new_config = create_server_tls_config(new_cert_path, new_key_path);
        let mut config = self.config.write().await;
        *config = new_config;
    }

    pub async fn get(&self) -> Arc<ServerConfig> {
        self.config.read().await.clone()
    }
}

// Watch for certificate changes (e.g., from Let's Encrypt)
async fn watch_certificates(tls: Arc<RotatingTlsConfig>) {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        // Check if certificates have changed
        if certificates_changed() {
            tls.rotate("new_server.crt", "new_server.key").await;
            tracing::info!("TLS certificates rotated");
        }
    }
}
```

#### Message Size Limits

All transports enforce message size limits (default: 16 MB):

```rust
// HTTP transport
let config = HttpTransportConfig::new("https://example.com/mcp")
    .with_max_message_size(4 * 1024 * 1024); // 4 MB

// WebSocket transport
let config = WebSocketConfig::new("wss://example.com/mcp")
    .max_message_size(4 * 1024 * 1024);

// Unix socket transport
let config = UnixSocketConfig::new("/tmp/mcp.sock")
    .with_max_message_size(4 * 1024 * 1024);

// Stdio transport uses MAX_MESSAGE_SIZE constant (16 MB)
```

#### WebSocket Origin Validation

Protect against DNS rebinding attacks by validating origins:

```rust
// Server-side WebSocket origin validation
async fn handle_connection(
    ws: WebSocket,
    origin: Option<HeaderValue>,
) -> Result<(), Error> {
    // Validate origin header
    if let Some(origin) = origin {
        let allowed_origins = ["https://trusted-client.com"];
        if !allowed_origins.contains(&origin.to_str().unwrap_or("")) {
            return Err(Error::ForbiddenOrigin);
        }
    }
    // Continue with connection...
}
```

### 2. Input Validation

#### Tool Parameter Validation

Always validate tool inputs:

```rust
#[mcp_server(name = "file-server", version = "1.0.0")]
impl FileServer {
    #[tool(description = "Read a file from the allowed directory")]
    async fn read_file(&self, path: String) -> Result<ToolOutput, McpError> {
        // Validate path doesn't escape allowed directory
        let canonical = std::fs::canonicalize(&path)
            .map_err(|e| McpError::invalid_params("read_file", e.to_string()))?;

        if !canonical.starts_with(&self.allowed_root) {
            return Err(McpError::invalid_params(
                "read_file",
                "Path escapes allowed directory"
            ));
        }

        // Additional validation
        if path.contains("..") || path.contains('\0') {
            return Err(McpError::invalid_params(
                "read_file",
                "Invalid characters in path"
            ));
        }

        // Safe to read
        let content = std::fs::read_to_string(&canonical)?;
        Ok(ToolOutput::text(content))
    }
}
```

#### Resource URI Validation

Validate resource URIs thoroughly:

```rust
#[resource(uri_pattern = "db://{table}/{id}", name = "Database Record")]
async fn get_record(
    &self,
    uri: &str,
) -> Result<ResourceContents, McpError> {
    // Parse and validate URI components
    let parts: Vec<&str> = uri.strip_prefix("db://")
        .ok_or_else(|| McpError::invalid_request("Invalid URI scheme"))?
        .split('/')
        .collect();

    let table = parts.get(0)
        .ok_or_else(|| McpError::invalid_request("Missing table"))?;
    let id = parts.get(1)
        .ok_or_else(|| McpError::invalid_request("Missing id"))?;

    // Whitelist allowed tables
    let allowed_tables = ["users", "products", "orders"];
    if !allowed_tables.contains(table) {
        return Err(McpError::ResourceAccessDenied {
            uri: uri.to_string(),
            reason: Some("Table not accessible".to_string()),
        });
    }

    // Validate ID format (prevent SQL injection)
    if !id.chars().all(|c| c.is_alphanumeric()) {
        return Err(McpError::invalid_params("get_record", "Invalid ID format"));
    }

    // Safe to query
    let data = self.db.get(table, id).await?;
    Ok(ResourceContents::json(uri, &data))
}
```

### 3. Authentication & Authorization

#### OAuth 2.1 Integration (Recommended)

For production deployments, implement OAuth 2.1 per the MCP specification (2025-06-18).
The SDK provides comprehensive OAuth 2.1 types in `mcpkit_core::auth`:

```rust
use mcpkit_core::auth::{
    ProtectedResourceMetadata, AuthorizationServerMetadata,
    AuthorizationRequest, TokenRequest, PkceChallenge,
    WwwAuthenticate, AuthorizationConfig,
};

// Server-side: Expose Protected Resource Metadata (RFC 9728)
let metadata = ProtectedResourceMetadata::new("https://mcp.example.com")
    .with_authorization_server("https://auth.example.com")
    .with_scopes(["mcp:read", "mcp:write"]);

// Client-side: Build authorization request with PKCE (required)
let pkce = PkceChallenge::new();
let auth_request = AuthorizationRequest::new(
    "my-client-id",
    &pkce,
    "https://mcp.example.com", // Resource indicator (RFC 8707)
)
.with_redirect_uri("http://localhost:8080/callback")
.with_scope("mcp:read");

// Exchange code for token
let token_request = TokenRequest::authorization_code(
    authorization_code,
    "my-client-id",
    &pkce.verifier,
    "https://mcp.example.com",
);

// Server returns 401 with WWW-Authenticate header per RFC 9728
let www_auth = WwwAuthenticate::new(
    "https://mcp.example.com/.well-known/oauth-protected-resource"
)
.with_error(mcpkit_core::auth::OAuthError::InvalidToken);
```

Key OAuth 2.1 requirements per MCP specification:
- PKCE is **required** for all clients (use `PkceChallenge::new()`)
- Resource indicators (RFC 8707) are **required** (prevents token mis-redemption)
- Protected Resource Metadata (RFC 9728) is **required** for servers
- Tokens must be sent via `Authorization: Bearer` header only

#### Capability-Based Access Control

Use server capabilities to restrict what clients can do:

```rust
use mcpkit_core::capability::ServerCapabilities;

// Only expose specific capabilities
let capabilities = ServerCapabilities::new()
    .with_tools()     // Allow tool invocation
    // .with_resources() - Not exposing resources
    // .with_prompts()   - Not exposing prompts
    ;
```

### 4. Output Sanitization

#### Preventing Prompt Injection

Sanitize tool outputs to prevent prompt injection:

```rust
#[tool(description = "Search for documents")]
async fn search(&self, query: String) -> ToolOutput {
    let results = self.db.search(&query).await;

    // Sanitize results to prevent prompt injection
    let sanitized: Vec<_> = results.iter()
        .map(|r| sanitize_for_llm(&r.content))
        .collect();

    ToolOutput::json(serde_json::json!({
        "results": sanitized,
        "total": results.len(),
    }))
}

fn sanitize_for_llm(content: &str) -> String {
    // Remove or escape potentially dangerous sequences
    content
        .replace("```", "'''")  // Escape code blocks
        .replace("[[", "[")     // Escape markdown links
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
```

### 5. Rate Limiting

Implement rate limiting to prevent abuse:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::{Duration, Instant};

struct RateLimiter {
    requests: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            requests: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window,
        }
    }

    async fn check(&self, client_id: &str) -> Result<(), McpError> {
        let mut requests = self.requests.lock().await;
        let now = Instant::now();

        let entry = requests.entry(client_id.to_string())
            .or_insert_with(Vec::new);

        // Remove old requests
        entry.retain(|t| now.duration_since(*t) < self.window);

        if entry.len() >= self.max_requests {
            return Err(McpError::Internal {
                message: "Rate limit exceeded".to_string(),
                source: None,
            });
        }

        entry.push(now);
        Ok(())
    }
}
```

### 6. Secure Defaults

The SDK provides secure defaults:

| Feature | Default | Notes |
|---------|---------|-------|
| TLS | rustls | No OpenSSL dependency |
| Message size limit | 16 MB | Prevents memory exhaustion |
| No unsafe code | Enforced | `unsafe_code = "deny"` |
| Request timeout | Transport-specific | Prevents hanging connections |

## Common Vulnerabilities

### Path Traversal

**Vulnerable:**
```rust
async fn read_file(&self, path: String) -> ToolOutput {
    // UNSAFE: Direct path use
    let content = std::fs::read_to_string(&path)?;
    ToolOutput::text(content)
}
```

**Secure:**
```rust
async fn read_file(&self, path: String) -> Result<ToolOutput, McpError> {
    // Canonicalize and validate
    let canonical = std::fs::canonicalize(&path)
        .map_err(|_| McpError::resource_not_found(&path))?;

    if !canonical.starts_with(&self.allowed_root) {
        return Err(McpError::ResourceAccessDenied {
            uri: path,
            reason: Some("Access denied".to_string()),
        });
    }

    let content = std::fs::read_to_string(&canonical)?;
    Ok(ToolOutput::text(content))
}
```

### SQL Injection

**Vulnerable:**
```rust
async fn query(&self, table: String, id: String) -> ToolOutput {
    // UNSAFE: String interpolation
    let query = format!("SELECT * FROM {} WHERE id = '{}'", table, id);
    let result = self.db.execute_raw(&query).await?;
    ToolOutput::json(result)
}
```

**Secure:**
```rust
async fn query(&self, table: String, id: String) -> Result<ToolOutput, McpError> {
    // Whitelist tables
    let allowed = ["users", "products"];
    if !allowed.contains(&table.as_str()) {
        return Err(McpError::invalid_params("query", "Invalid table"));
    }

    // Use parameterized queries
    let result = sqlx::query("SELECT * FROM $1 WHERE id = $2")
        .bind(&table)
        .bind(&id)
        .fetch_all(&self.pool)
        .await?;

    Ok(ToolOutput::json(result))
}
```

### Command Injection

**Vulnerable:**
```rust
async fn run_command(&self, cmd: String) -> ToolOutput {
    // UNSAFE: Shell execution
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .output()?;
    ToolOutput::text(String::from_utf8_lossy(&output.stdout))
}
```

**Secure:**
```rust
async fn run_command(&self, operation: String, args: Vec<String>) -> Result<ToolOutput, McpError> {
    // Whitelist allowed operations
    let allowed_ops = ["list", "status", "version"];
    if !allowed_ops.contains(&operation.as_str()) {
        return Err(McpError::invalid_params("run_command", "Operation not allowed"));
    }

    // Map to safe commands
    let (cmd, safe_args) = match operation.as_str() {
        "list" => ("ls", vec!["-la"]),
        "status" => ("git", vec!["status"]),
        "version" => ("cat", vec!["/etc/os-release"]),
        _ => unreachable!(),
    };

    // Execute without shell
    let output = std::process::Command::new(cmd)
        .args(&safe_args)
        .output()
        .map_err(|e| McpError::internal(e.to_string()))?;

    Ok(ToolOutput::text(String::from_utf8_lossy(&output.stdout)))
}
```

## Security Checklist

### Server Implementation

- [ ] Validate all tool parameters
- [ ] Sanitize tool outputs
- [ ] Implement rate limiting
- [ ] Use TLS for network transports
- [ ] Set appropriate message size limits
- [ ] Log security-relevant events
- [ ] Implement proper authentication
- [ ] Use capability-based access control
- [ ] Handle errors without leaking sensitive info
- [ ] Regularly update dependencies

### Client Implementation

- [ ] Validate server responses
- [ ] Implement request timeouts
- [ ] Handle connection errors gracefully
- [ ] Don't trust server-provided data blindly
- [ ] Implement user approval for sensitive operations
- [ ] Use secure credential storage
- [ ] Verify server identity (TLS)

### Deployment

- [ ] Run servers with minimal privileges
- [ ] Use network isolation where possible
- [ ] Monitor for suspicious activity
- [ ] Implement audit logging
- [ ] Have incident response procedures
- [ ] Regular security assessments

## Reporting Security Issues

If you discover a security vulnerability in the Rust MCP SDK, please report it responsibly:

1. **DO NOT** open a public GitHub issue
2. Email security concerns to [security contact from SECURITY.md]
3. Include detailed reproduction steps
4. Allow time for a fix before public disclosure

See [SECURITY.md](../SECURITY.md) for full details.

## References

- [MCP Specification - Security Considerations](https://modelcontextprotocol.io/specification/2025-06-18)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [Rust Security Best Practices](https://anssi-fr.github.io/rust-guide/)
- [OAuth 2.1 Specification](https://datatracker.ietf.org/doc/html/draft-ietf-oauth-v2-1-08)
