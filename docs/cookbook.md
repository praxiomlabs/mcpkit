# Cookbook

This cookbook provides complete, production-ready examples for common MCP server patterns.

## Table of Contents

1. [Database Integration](#database-integration)
2. [Authentication](#authentication)
3. [Rate Limiting](#rate-limiting)
4. [Structured Output](#structured-output)

---

## Database Integration

### SQLite with sqlx

A complete example of an MCP server with SQLite database integration.

```rust
use mcpkit::prelude::*;
use mcpkit_server::ServerBuilder;
use mcpkit_transport::StdioTransport;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    id: i64,
    name: String,
    email: String,
    created_at: String,
}

struct DatabaseServer {
    pool: SqlitePool,
}

impl DatabaseServer {
    async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await?;

        Ok(Self { pool })
    }
}

#[mcp_server(name = "database-server", version = "1.0.0")]
impl DatabaseServer {
    #[tool(
        description = "Query users from the database",
        params(
            filter(description = "Optional name filter (partial match)"),
            limit(description = "Maximum number of results (default: 10)")
        )
    )]
    async fn query_users(
        &self,
        filter: Option<String>,
        limit: Option<i32>,
    ) -> Result<ToolOutput, McpError> {
        let limit = limit.unwrap_or(10).min(100); // Cap at 100

        let users: Vec<User> = if let Some(filter) = filter {
            sqlx::query_as!(
                User,
                r#"
                SELECT id, name, email, created_at
                FROM users
                WHERE name LIKE ?
                ORDER BY created_at DESC
                LIMIT ?
                "#,
                format!("%{}%", filter),
                limit
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| McpError::internal(format!("Database error: {}", e)))?
        } else {
            sqlx::query_as!(
                User,
                r#"
                SELECT id, name, email, created_at
                FROM users
                ORDER BY created_at DESC
                LIMIT ?
                "#,
                limit
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| McpError::internal(format!("Database error: {}", e)))?
        };

        Ok(ToolOutput::json(serde_json::json!({
            "count": users.len(),
            "users": users,
        })))
    }

    #[tool(
        description = "Create a new user",
        params(
            name(description = "User's full name"),
            email(description = "User's email address")
        )
    )]
    async fn create_user(
        &self,
        name: String,
        email: String,
    ) -> Result<ToolOutput, McpError> {
        // Validate email format
        if !email.contains('@') {
            return Ok(ToolOutput::error_with_suggestion(
                "Invalid email format",
                "Email must contain @ symbol"
            ));
        }

        let result = sqlx::query(
            "INSERT INTO users (name, email) VALUES (?, ?)"
        )
            .bind(&name)
            .bind(&email)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint") {
                    return McpError::invalid_params("create_user", "Email already exists");
                }
                McpError::internal(format!("Database error: {}", e))
            })?;

        Ok(ToolOutput::json(serde_json::json!({
            "success": true,
            "id": result.last_insert_rowid(),
            "message": format!("User '{}' created successfully", name),
        })))
    }

    #[tool(
        description = "Get a specific user by ID",
        params(id(description = "The user's ID"))
    )]
    async fn get_user(&self, id: i64) -> Result<ToolOutput, McpError> {
        let user = sqlx::query_as!(
            User,
            "SELECT id, name, email, created_at FROM users WHERE id = ?",
            id
        )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| McpError::internal(format!("Database error: {}", e)))?;

        match user {
            Some(user) => Ok(ToolOutput::json(&user)),
            None => Ok(ToolOutput::error(format!("User with ID {} not found", id))),
        }
    }

    #[resource(uri_pattern = "user://{id}", name = "User Profile")]
    async fn user_resource(&self, uri: &str) -> Result<ResourceContents, McpError> {
        let id: i64 = uri.strip_prefix("user://")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| McpError::invalid_request("Invalid user URI"))?;

        let user = sqlx::query_as!(
            User,
            "SELECT id, name, email, created_at FROM users WHERE id = ?",
            id
        )
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| McpError::internal(e.to_string()))?
            .ok_or_else(|| McpError::resource_not_found(uri))?;

        Ok(ResourceContents::json(uri, &user))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::init();

    // Create server with database
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:./data.db?mode=rwc".to_string());

    let server = DatabaseServer::new(&database_url).await?;

    // Build and run MCP server
    let transport = StdioTransport::new();
    ServerBuilder::new(server)
        .with_tools_and_resources()
        .build()
        .serve(transport)
        .await?;

    Ok(())
}
```

### PostgreSQL with Connection Pool

```rust
use sqlx::{postgres::PgPoolOptions, PgPool};

struct PostgresServer {
    pool: PgPool,
}

impl PostgresServer {
    async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .idle_timeout(std::time::Duration::from_secs(600))
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }
}

#[mcp_server(name = "postgres-server", version = "1.0.0")]
impl PostgresServer {
    #[tool(description = "Execute a read-only SQL query")]
    async fn query(&self, sql: String) -> Result<ToolOutput, McpError> {
        // Only allow SELECT statements
        let normalized = sql.trim().to_uppercase();
        if !normalized.starts_with("SELECT") {
            return Ok(ToolOutput::error_with_suggestion(
                "Only SELECT queries are allowed",
                "Start your query with SELECT"
            ));
        }

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| McpError::internal(format!("Query error: {}", e)))?;

        let results: Vec<serde_json::Value> = rows.iter()
            .map(|row| {
                // Convert row to JSON object
                // (implementation depends on your schema)
                serde_json::json!({})
            })
            .collect();

        Ok(ToolOutput::json(serde_json::json!({
            "rows": results,
            "count": results.len(),
        })))
    }
}
```

---

## Authentication

### API Key Authentication

```rust
use mcpkit::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
struct ApiKeyAuth {
    valid_keys: Arc<RwLock<HashMap<String, ApiKeyInfo>>>,
}

struct ApiKeyInfo {
    name: String,
    permissions: Vec<String>,
    rate_limit: u32,
}

impl ApiKeyAuth {
    fn new() -> Self {
        Self {
            valid_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn add_key(&self, key: &str, info: ApiKeyInfo) {
        self.valid_keys.write().await.insert(key.to_string(), info);
    }

    async fn validate(&self, key: &str) -> Option<ApiKeyInfo> {
        self.valid_keys.read().await.get(key).cloned()
    }

    async fn has_permission(&self, key: &str, permission: &str) -> bool {
        if let Some(info) = self.validate(key).await {
            info.permissions.contains(&permission.to_string())
        } else {
            false
        }
    }
}

struct AuthenticatedServer {
    auth: ApiKeyAuth,
    current_key: Option<String>,
}

#[mcp_server(name = "authenticated-server", version = "1.0.0")]
impl AuthenticatedServer {
    #[tool(
        description = "Authenticate with an API key",
        params(api_key(description = "Your API key"))
    )]
    async fn authenticate(&mut self, api_key: String) -> ToolOutput {
        if let Some(info) = self.auth.validate(&api_key).await {
            self.current_key = Some(api_key);
            ToolOutput::json(serde_json::json!({
                "success": true,
                "message": format!("Authenticated as {}", info.name),
                "permissions": info.permissions,
            }))
        } else {
            ToolOutput::error("Invalid API key")
        }
    }

    #[tool(
        description = "Read protected data (requires 'read' permission)",
        params(resource(description = "Resource to read"))
    )]
    async fn read_data(&self, resource: String) -> Result<ToolOutput, McpError> {
        let key = self.current_key.as_ref()
            .ok_or_else(|| McpError::invalid_request("Not authenticated"))?;

        if !self.auth.has_permission(key, "read").await {
            return Err(McpError::ResourceAccessDenied {
                uri: resource,
                reason: Some("Missing 'read' permission".to_string()),
            });
        }

        // Read the data...
        Ok(ToolOutput::text(format!("Data from {}", resource)))
    }

    #[tool(
        description = "Write data (requires 'write' permission)",
        params(
            resource(description = "Resource to write to"),
            data(description = "Data to write")
        )
    )]
    async fn write_data(&self, resource: String, data: String) -> Result<ToolOutput, McpError> {
        let key = self.current_key.as_ref()
            .ok_or_else(|| McpError::invalid_request("Not authenticated"))?;

        if !self.auth.has_permission(key, "write").await {
            return Err(McpError::ResourceAccessDenied {
                uri: resource,
                reason: Some("Missing 'write' permission".to_string()),
            });
        }

        // Write the data...
        Ok(ToolOutput::json(serde_json::json!({
            "success": true,
            "resource": resource,
            "bytes_written": data.len(),
        })))
    }
}
```

### JWT Authentication

```rust
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,         // Subject (user ID)
    exp: u64,           // Expiration time
    iat: u64,           // Issued at
    permissions: Vec<String>,
}

struct JwtAuth {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtAuth {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
        }
    }

    fn create_token(&self, user_id: &str, permissions: Vec<String>) -> Result<String, McpError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: user_id.to_string(),
            exp: now + 3600, // 1 hour
            iat: now,
            permissions,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| McpError::internal(format!("Token creation failed: {}", e)))
    }

    fn validate_token(&self, token: &str) -> Result<Claims, McpError> {
        decode::<Claims>(token, &self.decoding_key, &Validation::default())
            .map(|data| data.claims)
            .map_err(|e| McpError::invalid_request(format!("Invalid token: {}", e)))
    }
}

struct JwtServer {
    auth: JwtAuth,
    users: HashMap<String, (String, Vec<String>)>, // username -> (password_hash, permissions)
}

#[mcp_server(name = "jwt-server", version = "1.0.0")]
impl JwtServer {
    #[tool(
        description = "Login and get a JWT token",
        params(
            username(description = "Your username"),
            password(description = "Your password")
        )
    )]
    async fn login(&self, username: String, password: String) -> Result<ToolOutput, McpError> {
        let (password_hash, permissions) = self.users.get(&username)
            .ok_or_else(|| McpError::invalid_params("login", "Invalid credentials"))?;

        // Verify password (use bcrypt or argon2 in production)
        if !verify_password(&password, password_hash) {
            return Err(McpError::invalid_params("login", "Invalid credentials"));
        }

        let token = self.auth.create_token(&username, permissions.clone())?;

        Ok(ToolOutput::json(serde_json::json!({
            "token": token,
            "expires_in": 3600,
            "token_type": "Bearer",
        })))
    }

    #[tool(
        description = "Access a protected resource",
        params(
            token(description = "Your JWT token"),
            resource(description = "Resource to access")
        )
    )]
    async fn access_resource(
        &self,
        token: String,
        resource: String,
    ) -> Result<ToolOutput, McpError> {
        let claims = self.auth.validate_token(&token)?;

        // Check if token has required permission
        if !claims.permissions.contains(&"read".to_string()) {
            return Err(McpError::ResourceAccessDenied {
                uri: resource,
                reason: Some("Insufficient permissions".to_string()),
            });
        }

        Ok(ToolOutput::text(format!(
            "User {} accessed resource {}",
            claims.sub, resource
        )))
    }
}

fn verify_password(password: &str, hash: &str) -> bool {
    // Use bcrypt::verify or argon2::verify in production
    password == hash // INSECURE - for example only
}
```

---

## Rate Limiting

### Token Bucket Rate Limiter

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
struct TokenBucket {
    capacity: u32,
    refill_rate: f64,  // tokens per second
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            refill_rate,
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }

    fn try_acquire(&mut self, tokens: u32) -> bool {
        self.refill();

        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        self.last_refill = now;
    }

    fn wait_time(&self, tokens: u32) -> Duration {
        if self.tokens >= tokens as f64 {
            Duration::ZERO
        } else {
            let needed = tokens as f64 - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    default_capacity: u32,
    default_rate: f64,
}

impl RateLimiter {
    fn new(default_capacity: u32, default_rate: f64) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            default_capacity,
            default_rate,
        }
    }

    async fn check(&self, client_id: &str, cost: u32) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().await;

        let bucket = buckets
            .entry(client_id.to_string())
            .or_insert_with(|| TokenBucket::new(self.default_capacity, self.default_rate));

        if bucket.try_acquire(cost) {
            Ok(())
        } else {
            Err(bucket.wait_time(cost))
        }
    }
}

struct RateLimitedServer {
    limiter: RateLimiter,
    client_id: String,
}

#[mcp_server(name = "rate-limited-server", version = "1.0.0")]
impl RateLimitedServer {
    #[tool(
        description = "A rate-limited operation (costs 1 token)",
        params(input(description = "Input data"))
    )]
    async fn standard_operation(&self, input: String) -> Result<ToolOutput, McpError> {
        self.check_rate_limit(1).await?;

        // Perform operation...
        Ok(ToolOutput::text(format!("Processed: {}", input)))
    }

    #[tool(
        description = "An expensive operation (costs 10 tokens)",
        params(query(description = "Search query"))
    )]
    async fn expensive_search(&self, query: String) -> Result<ToolOutput, McpError> {
        self.check_rate_limit(10).await?;

        // Perform expensive search...
        Ok(ToolOutput::json(serde_json::json!({
            "query": query,
            "results": [],
        })))
    }

    async fn check_rate_limit(&self, cost: u32) -> Result<(), McpError> {
        match self.limiter.check(&self.client_id, cost).await {
            Ok(()) => Ok(()),
            Err(wait_time) => {
                Ok(())  // Could also return error instead of waiting
                // Or return informative error:
                // Err(McpError::Internal {
                //     message: format!(
                //         "Rate limit exceeded. Try again in {:.1}s",
                //         wait_time.as_secs_f64()
                //     ),
                //     source: None,
                // })
            }
        }
    }
}
```

### Sliding Window Rate Limiter

```rust
use std::collections::VecDeque;

struct SlidingWindowLimiter {
    requests: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    window: Duration,
    max_requests: usize,
}

impl SlidingWindowLimiter {
    fn new(window: Duration, max_requests: usize) -> Self {
        Self {
            requests: Arc::new(Mutex::new(HashMap::new())),
            window,
            max_requests,
        }
    }

    async fn check(&self, client_id: &str) -> Result<RateLimitInfo, McpError> {
        let mut requests = self.requests.lock().await;
        let now = Instant::now();

        let queue = requests
            .entry(client_id.to_string())
            .or_insert_with(VecDeque::new);

        // Remove old requests outside the window
        while let Some(&oldest) = queue.front() {
            if now.duration_since(oldest) > self.window {
                queue.pop_front();
            } else {
                break;
            }
        }

        let remaining = self.max_requests.saturating_sub(queue.len());

        if queue.len() >= self.max_requests {
            let oldest = queue.front().unwrap();
            let reset_at = *oldest + self.window;
            let retry_after = reset_at.duration_since(now);

            return Err(McpError::Internal {
                message: format!(
                    "Rate limit exceeded. {} requests per {:?}. Retry after {:.1}s",
                    self.max_requests,
                    self.window,
                    retry_after.as_secs_f64()
                ),
                source: None,
            });
        }

        queue.push_back(now);

        Ok(RateLimitInfo {
            limit: self.max_requests,
            remaining: remaining.saturating_sub(1),
            reset_in: self.window,
        })
    }
}

struct RateLimitInfo {
    limit: usize,
    remaining: usize,
    reset_in: Duration,
}
```

---

## Structured Output

### Typed Tool Outputs

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    id: String,
    title: String,
    snippet: String,
    relevance_score: f64,
    url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResponse {
    query: String,
    total_results: usize,
    results: Vec<SearchResult>,
    took_ms: u64,
    has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileInfo {
    path: String,
    size_bytes: u64,
    mime_type: String,
    created: String,
    modified: String,
    is_directory: bool,
    permissions: String,
}

struct StructuredServer;

#[mcp_server(name = "structured-server", version = "1.0.0")]
impl StructuredServer {
    #[tool(
        description = "Search for documents and return structured results",
        params(
            query(description = "Search query"),
            limit(description = "Maximum number of results (default: 10)")
        )
    )]
    async fn search(&self, query: String, limit: Option<usize>) -> Result<ToolOutput, McpError> {
        let limit = limit.unwrap_or(10);
        let start = std::time::Instant::now();

        // Perform search...
        let results = vec![
            SearchResult {
                id: "doc-1".to_string(),
                title: "Example Document".to_string(),
                snippet: format!("Content matching '{}'...", query),
                relevance_score: 0.95,
                url: Some("https://example.com/doc/1".to_string()),
            },
        ];

        let response = SearchResponse {
            query: query.clone(),
            total_results: results.len(),
            results,
            took_ms: start.elapsed().as_millis() as u64,
            has_more: false,
        };

        Ok(ToolOutput::json(&response))
    }

    #[tool(
        description = "Get detailed file information",
        params(path(description = "File path"))
    )]
    async fn file_info(&self, path: String) -> Result<ToolOutput, McpError> {
        let metadata = std::fs::metadata(&path)
            .map_err(|e| McpError::resource_not_found(&path))?;

        let info = FileInfo {
            path: path.clone(),
            size_bytes: metadata.len(),
            mime_type: guess_mime_type(&path),
            created: format_time(metadata.created().ok()),
            modified: format_time(metadata.modified().ok()),
            is_directory: metadata.is_dir(),
            permissions: format_permissions(&metadata),
        };

        Ok(ToolOutput::json(&info))
    }
}

fn guess_mime_type(path: &str) -> String {
    match std::path::Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        Some("html") => "text/html",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }.to_string()
}

fn format_time(time: Option<std::time::SystemTime>) -> String {
    time.and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs().to_string())
    }).unwrap_or_else(|| "unknown".to_string())
}

fn format_permissions(metadata: &std::fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        format!("{:o}", metadata.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        if metadata.permissions().readonly() {
            "readonly".to_string()
        } else {
            "read-write".to_string()
        }
    }
}
```

### Error Responses with Context

```rust
#[derive(Debug, Serialize)]
struct ErrorDetails {
    code: String,
    message: String,
    field: Option<String>,
    suggestion: Option<String>,
    documentation_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct ValidationErrors {
    errors: Vec<ErrorDetails>,
}

impl ValidationErrors {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn add(&mut self, code: &str, message: &str) -> &mut Self {
        self.errors.push(ErrorDetails {
            code: code.to_string(),
            message: message.to_string(),
            field: None,
            suggestion: None,
            documentation_url: None,
        });
        self
    }

    fn add_field_error(&mut self, field: &str, code: &str, message: &str) -> &mut Self {
        self.errors.push(ErrorDetails {
            code: code.to_string(),
            message: message.to_string(),
            field: Some(field.to_string()),
            suggestion: None,
            documentation_url: None,
        });
        self
    }

    fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    fn to_tool_output(&self) -> ToolOutput {
        ToolOutput::error_json(&serde_json::json!({
            "valid": false,
            "error_count": self.errors.len(),
            "errors": self.errors,
        }))
    }
}

#[mcp_server(name = "validation-server", version = "1.0.0")]
impl ValidationServer {
    #[tool(description = "Create a new record with validation")]
    async fn create_record(
        &self,
        name: String,
        email: String,
        age: Option<i32>,
    ) -> ToolOutput {
        let mut errors = ValidationErrors::new();

        // Validate name
        if name.is_empty() {
            errors.add_field_error("name", "REQUIRED", "Name is required");
        } else if name.len() < 2 {
            errors.add_field_error("name", "TOO_SHORT", "Name must be at least 2 characters");
        }

        // Validate email
        if !email.contains('@') {
            errors.add_field_error("email", "INVALID_FORMAT", "Email must contain @");
        }

        // Validate age if provided
        if let Some(age) = age {
            if age < 0 || age > 150 {
                errors.add_field_error("age", "OUT_OF_RANGE", "Age must be between 0 and 150");
            }
        }

        if !errors.is_empty() {
            return errors.to_tool_output();
        }

        // Create the record...
        ToolOutput::json(serde_json::json!({
            "valid": true,
            "created": {
                "id": "new-id-123",
                "name": name,
                "email": email,
                "age": age,
            }
        }))
    }
}
```

### Multi-Format Output

```rust
#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Json,
    Markdown,
    Plain,
    Csv,
}

impl OutputFormat {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "markdown" | "md" => Self::Markdown,
            "plain" | "text" => Self::Plain,
            "csv" => Self::Csv,
            _ => Self::Json,
        }
    }
}

struct MultiFormatServer;

#[mcp_server(name = "multiformat-server", version = "1.0.0")]
impl MultiFormatServer {
    #[tool(
        description = "List items in the requested format",
        params(
            format(description = "Output format: json, markdown, plain, or csv")
        )
    )]
    async fn list_items(&self, format: Option<String>) -> ToolOutput {
        let format = format.map(|f| OutputFormat::from_str(&f)).unwrap_or(OutputFormat::Json);

        let items = vec![
            ("apple", 10, 1.50),
            ("banana", 25, 0.75),
            ("cherry", 100, 5.00),
        ];

        match format {
            OutputFormat::Json => {
                let json_items: Vec<_> = items.iter()
                    .map(|(name, qty, price)| serde_json::json!({
                        "name": name,
                        "quantity": qty,
                        "price": price,
                    }))
                    .collect();
                ToolOutput::json(serde_json::json!({ "items": json_items }))
            }
            OutputFormat::Markdown => {
                let mut md = String::from("| Name | Quantity | Price |\n|------|----------|-------|\n");
                for (name, qty, price) in &items {
                    md.push_str(&format!("| {} | {} | ${:.2} |\n", name, qty, price));
                }
                ToolOutput::text(md)
            }
            OutputFormat::Plain => {
                let text = items.iter()
                    .map(|(name, qty, price)| format!("{}: {} @ ${:.2}", name, qty, price))
                    .collect::<Vec<_>>()
                    .join("\n");
                ToolOutput::text(text)
            }
            OutputFormat::Csv => {
                let mut csv = String::from("name,quantity,price\n");
                for (name, qty, price) in &items {
                    csv.push_str(&format!("{},{},{:.2}\n", name, qty, price));
                }
                ToolOutput::text(csv)
            }
        }
    }
}
```

---

## Additional Patterns

### Caching

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct CachedValue<T> {
    value: T,
    expires_at: Instant,
}

struct Cache<T> {
    data: Arc<Mutex<HashMap<String, CachedValue<T>>>>,
    ttl: Duration,
}

impl<T: Clone> Cache<T> {
    fn new(ttl: Duration) -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    async fn get(&self, key: &str) -> Option<T> {
        let data = self.data.lock().await;
        data.get(key)
            .filter(|v| v.expires_at > Instant::now())
            .map(|v| v.value.clone())
    }

    async fn set(&self, key: &str, value: T) {
        let mut data = self.data.lock().await;
        data.insert(key.to_string(), CachedValue {
            value,
            expires_at: Instant::now() + self.ttl,
        });
    }

    async fn get_or_insert<F, Fut>(&self, key: &str, f: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        if let Some(value) = self.get(key).await {
            return value;
        }

        let value = f().await;
        self.set(key, value.clone()).await;
        value
    }
}
```

### Pagination

```rust
#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    page: usize,
    page_size: usize,
    total_items: usize,
    total_pages: usize,
    has_next: bool,
    has_prev: bool,
}

impl<T> PaginatedResponse<T> {
    fn new(all_items: Vec<T>, page: usize, page_size: usize) -> Self {
        let total_items = all_items.len();
        let total_pages = (total_items + page_size - 1) / page_size;
        let start = (page - 1) * page_size;
        let end = (start + page_size).min(total_items);

        let items = if start < total_items {
            all_items.into_iter().skip(start).take(page_size).collect()
        } else {
            Vec::new()
        };

        Self {
            items,
            page,
            page_size,
            total_items,
            total_pages,
            has_next: page < total_pages,
            has_prev: page > 1,
        }
    }
}

#[mcp_server(name = "paginated-server", version = "1.0.0")]
impl PaginatedServer {
    #[tool(
        description = "List items with pagination",
        params(
            page(description = "Page number (starting from 1)"),
            page_size(description = "Items per page (default: 10, max: 100)")
        )
    )]
    async fn list(&self, page: Option<usize>, page_size: Option<usize>) -> ToolOutput {
        let page = page.unwrap_or(1).max(1);
        let page_size = page_size.unwrap_or(10).min(100);

        let all_items = self.fetch_all_items().await;
        let response = PaginatedResponse::new(all_items, page, page_size);

        ToolOutput::json(&response)
    }
}
```
