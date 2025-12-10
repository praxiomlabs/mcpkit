# Working with Tools

Tools are the primary way MCP servers expose functionality to AI assistants. This guide covers everything you need to know about defining and implementing tools.

## Basic Tool Definition

Use the `#[tool]` attribute to mark a method as an MCP tool:

```rust
use mcp::prelude::*;

struct MyServer;

#[mcp_server(name = "my-server", version = "1.0.0")]
impl MyServer {
    #[tool(description = "Say hello to someone")]
    async fn greet(&self, name: String) -> ToolOutput {
        ToolOutput::text(format!("Hello, {}!", name))
    }
}
```

## Tool Attributes

The `#[tool]` attribute supports several options:

```rust
#[tool(
    description = "Description shown to the AI",
    name = "custom_name",        // Override the tool name
    destructive = true,          // Hint: may cause destructive changes
    idempotent = true,           // Hint: calling multiple times has same effect
    read_only = true,            // Hint: only reads data
)]
async fn my_tool(&self, ...) -> ToolOutput {
    // ...
}
```

## Parameter Types

### Required Parameters

All non-`Option` parameters are required:

```rust
#[tool(description = "Add two numbers")]
async fn add(&self, a: f64, b: f64) -> ToolOutput {
    ToolOutput::text(format!("{}", a + b))
}
```

### Optional Parameters

Use `Option<T>` for optional parameters:

```rust
#[tool(description = "Greet someone")]
async fn greet(&self, name: String, title: Option<String>) -> ToolOutput {
    let greeting = match title {
        Some(t) => format!("Hello, {} {}!", t, name),
        None => format!("Hello, {}!", name),
    };
    ToolOutput::text(greeting)
}
```

### Supported Types

- `String`, `&str`
- `i32`, `i64`, `u32`, `u64`, `f32`, `f64`
- `bool`
- `Vec<T>` (arrays)
- `Option<T>` (optional)
- `serde_json::Value` (arbitrary JSON)

### Complex Input Types

For complex inputs, use `#[derive(ToolInput)]`:

```rust
use mcp::prelude::*;
use serde::Deserialize;

#[derive(ToolInput, Deserialize)]
struct SearchParams {
    /// The search query
    query: String,
    /// Maximum results to return
    #[mcp(default = 10)]
    limit: usize,
    /// Optional category filter
    category: Option<String>,
}

#[mcp_server(name = "search", version = "1.0.0")]
impl SearchServer {
    #[tool(description = "Search for items")]
    async fn search(&self, params: SearchParams) -> ToolOutput {
        // Use params.query, params.limit, params.category
        ToolOutput::text(format!("Searching for: {}", params.query))
    }
}
```

## Return Types

### Success Output

```rust
// Text output
ToolOutput::text("Hello, World!")

// JSON output
ToolOutput::json(serde_json::json!({
    "status": "success",
    "count": 42
}))

// Multiple content items
ToolOutput::success(CallToolResult {
    content: vec![
        Content::text("First result"),
        Content::text("Second result"),
    ],
    is_error: Some(false),
})
```

### Error Output

```rust
// Simple error
ToolOutput::error("Something went wrong")

// Error with recovery hint
ToolOutput::recoverable_error(
    "File not found",
    "Try checking the file path",
)
```

### Result Type

For fallible operations, return `Result<ToolOutput, McpError>`:

```rust
#[tool(description = "Read a file")]
async fn read_file(&self, path: String) -> Result<ToolOutput, McpError> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| McpError::tool_error("read_file", e.to_string()))?;
    Ok(ToolOutput::text(content))
}
```

## Annotations

Tool annotations provide hints to AI assistants about how to use tools:

```rust
#[tool(
    description = "Delete a file permanently",
    destructive = true,    // Warns AI this is dangerous
)]
async fn delete_file(&self, path: String) -> ToolOutput {
    // ...
}

#[tool(
    description = "Get current time",
    read_only = true,      // Safe to call freely
    idempotent = true,     // Same result each time
)]
async fn get_time(&self) -> ToolOutput {
    // ...
}
```

## Accessing Context

Tools can access the request context for advanced operations:

```rust
use mcp_server::Context;

#[tool(description = "Tool with context")]
async fn with_context(&self, ctx: &Context<'_>, input: String) -> ToolOutput {
    // Access client capabilities
    let client_caps = ctx.client_capabilities();

    // Access progress reporting (if available)
    if let Some(token) = ctx.progress_token() {
        // Report progress...
    }

    ToolOutput::text(format!("Processed: {}", input))
}
```

## Best Practices

1. **Clear Descriptions**: Write descriptions that help AI understand when to use the tool
2. **Validate Early**: Validate inputs at the start of your function
3. **Handle Errors Gracefully**: Return meaningful error messages
4. **Use Annotations**: Help AI make safe decisions with `destructive`, `read_only`, etc.
5. **Keep Tools Focused**: Each tool should do one thing well

## Example: Full-Featured Tool

```rust
use mcp::prelude::*;
use mcp_server::Context;

struct DatabaseServer {
    db: Database,
}

#[mcp_server(name = "database", version = "1.0.0")]
impl DatabaseServer {
    /// Search the database for records matching the query.
    /// Returns up to `limit` results (default 10).
    #[tool(
        description = "Search database records by query",
        read_only = true,
    )]
    async fn search(
        &self,
        ctx: &Context<'_>,
        /// The search query string
        query: String,
        /// Maximum results to return (1-100)
        limit: Option<u32>,
    ) -> Result<ToolOutput, McpError> {
        // Validate inputs
        let limit = limit.unwrap_or(10).min(100).max(1);

        if query.trim().is_empty() {
            return Err(McpError::invalid_params(
                "search",
                "Query cannot be empty",
            ));
        }

        // Perform search
        let results = self.db.search(&query, limit as usize)
            .await
            .map_err(|e| McpError::tool_error("search", e.to_string()))?;

        // Return results
        Ok(ToolOutput::json(serde_json::json!({
            "query": query,
            "count": results.len(),
            "results": results,
        })))
    }
}
```
