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

Always use TLS for network transports:

```rust
use mcp_transport::websocket::WebSocketConfig;

let config = WebSocketConfig::new("wss://example.com/mcp")
    // Use rustls (not openssl) for TLS - this is the default
    .build();
```

The SDK uses `rustls` for TLS, avoiding OpenSSL vulnerabilities.

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
The SDK provides comprehensive OAuth 2.1 types in `mcp_core::auth`:

```rust
use mcp_core::auth::{
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
.with_error(mcp_core::auth::OAuthError::InvalidToken);
```

Key OAuth 2.1 requirements per MCP specification:
- PKCE is **required** for all clients (use `PkceChallenge::new()`)
- Resource indicators (RFC 8707) are **required** (prevents token mis-redemption)
- Protected Resource Metadata (RFC 9728) is **required** for servers
- Tokens must be sent via `Authorization: Bearer` header only

#### Capability-Based Access Control

Use server capabilities to restrict what clients can do:

```rust
use mcp_core::capability::ServerCapabilities;

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
