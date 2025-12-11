# Error Handling

The Rust MCP SDK provides a unified error handling system that's both ergonomic and compatible with JSON-RPC error responses.

## Design Philosophy

The SDK uses a **single, context-rich error type** rather than nested error hierarchies. This design provides:

- **Single error type**: All errors flow through `McpError`
- **Rich context**: Errors preserve context through the entire call stack
- **JSON-RPC compatible**: Easy conversion to JSON-RPC error responses
- **Diagnostic-friendly**: Integrates with `miette` for beautiful error reports
- **Size-optimized**: Large error variants are boxed to keep `Result<T, McpError>` small (~24 bytes)

### Two Error Patterns

The SDK has **two distinct error handling patterns** for different scenarios:

#### Pattern 1: `Result<T, McpError>` - SDK/Framework Errors

Use `Result<T, McpError>` for errors that indicate something went wrong with the MCP protocol, transport, or SDK internals:

| Scenario | Use `McpError` |
|----------|----------------|
| Transport failures | Connection lost, timeout, I/O errors |
| Protocol errors | Invalid JSON-RPC, version mismatch, missing fields |
| Resource not found | Requested resource/tool/prompt doesn't exist |
| Capability errors | Feature not supported by client/server |
| Internal errors | Unexpected SDK state, serialization failures |

These errors typically indicate the request cannot be completed and require intervention (reconnection, configuration change, bug fix).

```rust
async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
    self.ensure_capability("tools", self.has_tools())?;
    // Transport errors propagate as McpError
    let result = self.request("tools/list", None).await?;
    Ok(result.tools)
}
```

#### Pattern 2: `ToolOutput::error()` - User/LLM-Correctable Errors

Use `ToolOutput::error()` for errors that the LLM can potentially self-correct by adjusting its input:

| Scenario | Use `ToolOutput::error()` |
|----------|--------------------------|
| Validation failures | Invalid argument format, out-of-range values |
| Business logic errors | Division by zero, empty query, invalid date |
| Missing optional data | Lookup returned no results |
| Rate limiting | Too many requests (suggest retry) |

These errors are returned to the LLM with `is_error: true` in the response, allowing the model to understand what went wrong and try again.

```rust
#[tool(description = "Divide two numbers")]
async fn divide(&self, a: f64, b: f64) -> ToolOutput {
    if b == 0.0 {
        return ToolOutput::error_with_suggestion(
            "Cannot divide by zero",
            "Use a non-zero divisor",
        );
    }
    ToolOutput::text((a / b).to_string())
}
```

### Decision Guide

| Scenario | Use | Reason |
|----------|-----|--------|
| Database connection failed | `McpError` | Infrastructure issue |
| User provided invalid email format | `ToolOutput::error` | LLM can fix input |
| Tool doesn't exist | `McpError` | Protocol/discovery issue |
| Search returned no results | `ToolOutput::text("No results")` | Expected outcome, not an error |
| API rate limit exceeded | `ToolOutput::error_with_suggestion` | Temporary, can retry |
| Authentication required | `McpError` | Configuration issue |
| Invalid number format in input | `ToolOutput::error` | LLM can fix input |

## The McpError Type

All errors in the SDK flow through `McpError`:

```rust
use mcpkit_core::error::McpError;

fn process() -> Result<(), McpError> {
    // Your code here
    Ok(())
}
```

## Creating Errors

### Protocol Errors

```rust
// Parse error (-32700)
McpError::parse("Invalid JSON syntax")

// Invalid request (-32600)
McpError::invalid_request("Missing required field")

// Method not found (-32601)
McpError::method_not_found("unknown_method")

// With suggestions
McpError::method_not_found_with_suggestions(
    "tool_list",
    vec!["tools/list".to_string(), "tools/call".to_string()],
)

// Invalid params (-32602)
McpError::invalid_params("search", "Query cannot be empty")

// Internal error (-32603)
McpError::internal("Unexpected state")
```

### Domain Errors

```rust
// Tool execution error
McpError::tool_error("search", "Database connection failed")

// Resource not found
McpError::resource_not_found("config://missing")

// Resource access denied
McpError::ResourceAccessDenied {
    uri: "secret://data".to_string(),
    reason: Some("Insufficient permissions".to_string()),
}

// Timeout
McpError::timeout("database query", Duration::from_secs(30))

// Cancelled
McpError::cancelled("file upload")
```

### Transport Errors

```rust
use mcpkit_core::error::{McpError, TransportErrorKind, TransportContext};

// Using the helper function (recommended)
McpError::transport_with_context(
    TransportErrorKind::ConnectionFailed,
    "Could not connect to server",
    TransportContext::new("websocket")
        .with_remote_addr("ws://localhost:9000"),
)

// Or using the simple helper
McpError::transport(TransportErrorKind::ConnectionFailed, "Connection refused")
```

## Context Chaining

Add context to errors using the `McpResultExt` trait:

```rust
use mcpkit_core::error::{McpError, McpResultExt};

fn load_config() -> Result<Config, McpError> {
    let content = read_file("config.json")
        .context("Failed to load configuration")?;

    let config: Config = serde_json::from_str(&content)
        .map_err(|e| McpError::parse_with_source("Invalid config format", e))
        .context("Failed to parse configuration file")?;

    Ok(config)
}
```

### Lazy Context

For expensive context creation:

```rust
fn process_user(user_id: u64) -> Result<(), McpError> {
    fetch_user(user_id)
        .with_context(|| format!("Failed to process user {}", user_id))?;
    Ok(())
}
```

## Error Recovery

The SDK classifies errors as recoverable or not:

```rust
let error = McpError::resource_not_found("file://missing.txt");

if error.is_recoverable() {
    // AI can try a different approach
    println!("Recoverable: try alternative");
} else {
    // Fatal error, cannot proceed
    println!("Fatal: abort operation");
}
```

### Recoverable Errors

- `InvalidParams` - AI can fix the parameters
- `ResourceNotFound` - AI can try a different resource
- `Timeout` - AI can retry
- `ToolExecution` with `is_recoverable: true`

### Non-Recoverable Errors

- `Internal` - System error
- `Parse` - Protocol violation
- `ConnectionFailed` - Infrastructure issue

## JSON-RPC Error Codes

Errors automatically map to JSON-RPC error codes:

```rust
let error = McpError::method_not_found("unknown");
assert_eq!(error.code(), -32601);  // Standard JSON-RPC code
```

### Standard Codes

| Code | Constant | Meaning |
|------|----------|---------|
| -32700 | `PARSE_ERROR` | Invalid JSON |
| -32600 | `INVALID_REQUEST` | Invalid request object |
| -32601 | `METHOD_NOT_FOUND` | Method doesn't exist |
| -32602 | `INVALID_PARAMS` | Invalid parameters |
| -32603 | `INTERNAL_ERROR` | Internal error |
| -32000 to -32099 | Server errors | Application-defined |

## Converting to JSON-RPC

Errors convert to JSON-RPC error responses:

```rust
use mcpkit_core::error::JsonRpcError;

let mcp_error = McpError::method_not_found("unknown");
let json_error: JsonRpcError = (&mcp_error).into();

// Serialize for the wire
let json = serde_json::to_string(&json_error)?;
```

## Error Handling in Tools

### Simple Error Return

```rust
#[tool(description = "Read a file")]
async fn read_file(&self, path: String) -> ToolOutput {
    match std::fs::read_to_string(&path) {
        Ok(content) => ToolOutput::text(content),
        Err(e) => ToolOutput::error(format!("Cannot read {}: {}", path, e)),
    }
}
```

### Using Result

```rust
#[tool(description = "Parse JSON data")]
async fn parse_json(&self, data: String) -> Result<ToolOutput, McpError> {
    let value: serde_json::Value = serde_json::from_str(&data)
        .map_err(|e| McpError::invalid_params("parse_json", e.to_string()))?;

    Ok(ToolOutput::json(value))
}
```

### With Context

```rust
#[tool(description = "Process user data")]
async fn process_user(&self, user_id: String) -> Result<ToolOutput, McpError> {
    let user = self.db.get_user(&user_id)
        .await
        .context("Failed to fetch user")?;

    let processed = self.process(user)
        .await
        .with_context(|| format!("Failed to process user {}", user_id))?;

    Ok(ToolOutput::json(processed))
}
```

## Error Handling in Resources

```rust
#[resource(uri_pattern = "data://{id}", name = "Data")]
async fn get_data(&self, uri: &str) -> Result<ResourceContents, McpError> {
    let id = uri.strip_prefix("data://")
        .ok_or_else(|| McpError::invalid_request("Invalid URI format"))?;

    let data = self.db.get(id)
        .await
        .map_err(|_| McpError::resource_not_found(uri))?;

    Ok(ResourceContents::json(uri, &data))
}
```

## Best Practices

1. **Use Context**: Add context to help debug issues
2. **Be Specific**: Create errors with detailed messages
3. **Consider Recovery**: Mark errors as recoverable when appropriate
4. **Validate Early**: Check inputs at function entry
5. **Preserve Chains**: Don't swallow underlying errors
6. **Log Appropriately**: Log errors before returning them

## Complete Example

```rust
use mcpkit::prelude::*;
use mcpkit_core::error::{McpError, McpResultExt};

struct DataService {
    db: Database,
}

#[mcp_server(name = "data-service", version = "1.0.0")]
impl DataService {
    #[tool(description = "Query data with filters")]
    async fn query(
        &self,
        table: String,
        filter: Option<String>,
        limit: Option<u32>,
    ) -> Result<ToolOutput, McpError> {
        // Validate inputs
        if table.is_empty() {
            return Err(McpError::invalid_params(
                "query",
                "Table name cannot be empty",
            ));
        }

        let limit = limit.unwrap_or(100);
        if limit > 1000 {
            return Err(McpError::invalid_params(
                "query",
                "Limit cannot exceed 1000",
            ));
        }

        // Parse filter if provided
        let parsed_filter = if let Some(f) = filter {
            Some(self.parse_filter(&f)
                .context("Failed to parse filter expression")?)
        } else {
            None
        };

        // Execute query
        let results = self.db
            .query(&table, parsed_filter.as_ref(), limit)
            .await
            .with_context(|| format!("Query failed on table '{}'", table))?;

        Ok(ToolOutput::json(serde_json::json!({
            "table": table,
            "count": results.len(),
            "results": results,
        })))
    }

    fn parse_filter(&self, filter: &str) -> Result<Filter, McpError> {
        // Parse logic here
        serde_json::from_str(filter)
            .map_err(|e| McpError::invalid_params(
                "query",
                format!("Invalid filter JSON: {}", e),
            ))
    }
}
```
