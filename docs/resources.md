# Working with Resources

Resources allow MCP servers to expose data that AI assistants can read. Unlike tools (which execute actions), resources provide access to information.

## Basic Resource Definition

Use the `#[resource]` attribute to define a resource handler:

```rust
use mcpkit::prelude::*;

struct ConfigServer;

#[mcp_server(name = "config", version = "1.0.0")]
impl ConfigServer {
    #[resource(
        uri_pattern = "config://app/settings",
        name = "Application Settings",
        description = "Current application configuration",
        mime_type = "application/json"
    )]
    async fn get_settings(&self) -> ResourceContents {
        ResourceContents::text(
            "config://app/settings",
            r#"{"debug": true, "version": "1.0.0"}"#,
        )
    }
}
```

## URI Patterns

Resources use URI patterns to identify them. Patterns can include variables:

```rust
// Static URI
#[resource(uri_pattern = "file://config.json", ...)]

// Dynamic URI with variable
#[resource(uri_pattern = "user://{user_id}/profile", ...)]

// Multiple variables
#[resource(uri_pattern = "db://{database}/{table}", ...)]
```

## Resource Attributes

```rust
#[resource(
    uri_pattern = "myserver://data/{id}",  // Required: URI pattern
    name = "Data Resource",                 // Required: Display name
    description = "Access data by ID",      // Optional: Description
    mime_type = "application/json",         // Optional: MIME type
)]
```

## Return Types

### Text Content

```rust
ResourceContents::text(
    "config://app/settings",
    r#"{"setting": "value"}"#,
)
```

### Binary Content

```rust
ResourceContents::blob(
    "file://image.png",
    image_bytes,
    "image/png",
)
```

### Multiple Contents

```rust
ResourceContents::multiple(vec![
    ResourceContent::text("file://readme.md", "# README"),
    ResourceContent::text("file://license.txt", "MIT License"),
])
```

## Resource Templates

For dynamic resources, use templates:

```rust
#[mcp_server(name = "files", version = "1.0.0")]
impl FileServer {
    #[resource(
        uri_pattern = "file://{path}",
        name = "File Contents",
        description = "Read file contents by path",
        mime_type = "text/plain"
    )]
    async fn read_file(&self, uri: &str) -> ResourceContents {
        // Extract path from URI
        let path = uri.strip_prefix("file://").unwrap_or(uri);

        match std::fs::read_to_string(path) {
            Ok(content) => ResourceContents::text(uri, &content),
            Err(e) => ResourceContents::error(format!("Failed to read: {}", e)),
        }
    }
}
```

## Resource Subscriptions

Clients can subscribe to resource updates. Implement subscription support:

```rust
use mcpkit_server::handler::ResourceHandler;

impl ResourceHandler for MyServer {
    async fn subscribe(&self, uri: &str, ctx: &Context<'_>) -> Result<bool, McpError> {
        // Track subscription
        self.subscriptions.lock().await.insert(uri.to_string());
        Ok(true)
    }

    async fn unsubscribe(&self, uri: &str, ctx: &Context<'_>) -> Result<bool, McpError> {
        // Remove subscription
        self.subscriptions.lock().await.remove(uri);
        Ok(true)
    }
}
```

To notify clients of updates, send a `resources/updated` notification.

## Error Handling

```rust
#[resource(
    uri_pattern = "data://{id}",
    name = "Data",
)]
async fn get_data(&self, uri: &str) -> Result<ResourceContents, McpError> {
    let id = extract_id(uri)?;

    self.db.get(id)
        .await
        .map(|data| ResourceContents::json(uri, &data))
        .map_err(|e| McpError::resource_not_found(uri))
}
```

## Complete Example

```rust
use mcpkit::prelude::*;
use std::collections::HashMap;
use std::sync::RwLock;

struct DocumentServer {
    documents: RwLock<HashMap<String, String>>,
}

#[mcp_server(name = "documents", version = "1.0.0")]
impl DocumentServer {
    /// List all available documents
    #[resource(
        uri_pattern = "docs://list",
        name = "Document List",
        description = "List all document IDs",
        mime_type = "application/json"
    )]
    async fn list_documents(&self) -> ResourceContents {
        let docs = self.documents.read().unwrap();
        let ids: Vec<&String> = docs.keys().collect();
        ResourceContents::json("docs://list", &ids)
    }

    /// Get a specific document by ID
    #[resource(
        uri_pattern = "docs://{id}",
        name = "Document",
        description = "Get document content by ID",
        mime_type = "text/plain"
    )]
    async fn get_document(&self, uri: &str) -> Result<ResourceContents, McpError> {
        let id = uri.strip_prefix("docs://")
            .ok_or_else(|| McpError::invalid_request("Invalid URI format"))?;

        let docs = self.documents.read().unwrap();
        docs.get(id)
            .map(|content| ResourceContents::text(uri, content))
            .ok_or_else(|| McpError::resource_not_found(uri))
    }
}
```

## Best Practices

1. **Use Meaningful URIs**: Make URI patterns self-documenting
2. **Set MIME Types**: Help clients understand the content type
3. **Handle Missing Resources**: Return appropriate errors for not found
4. **Support Subscriptions**: Allow clients to track changes
5. **Cache Wisely**: Cache expensive resources when appropriate
6. **Document Templates**: Clearly document what variables are expected
