# Macro Reference and Troubleshooting

This document covers the procedural macros provided by the Rust MCP SDK, their usage, debugging techniques, and common issues.

## Available Macros

| Macro | Purpose |
|-------|---------|
| `#[mcp_server]` | Define an MCP server with tools, resources, and prompts |
| `#[tool]` | Mark a method as a callable tool |
| `#[resource]` | Mark a method as a resource handler |
| `#[prompt]` | Mark a method as a prompt template |
| `#[derive(ToolInput)]` | Generate JSON Schema for tool input types |

## Debug Mode: `debug_expand`

When troubleshooting macro issues, use `debug_expand` to see the generated code:

```rust
#[mcp_server(
    name = "my-server",
    version = "1.0.0",
    debug_expand = true  // Print generated code during compilation
)]
impl MyServer {
    #[tool(description = "Example tool")]
    async fn example(&self) -> ToolOutput {
        ToolOutput::text("Hello")
    }
}
```

When you compile with `debug_expand = true`, the macro will print the expanded code to stderr:

```
=== Generated code for MyServer ===
impl mcpkit_server::handler::ServerHandler for MyServer {
    fn info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo {
            name: "my-server".to_string(),
            version: "1.0.0".to_string(),
            instructions: None,
        }
    }
    // ... more generated code
}
```

This is invaluable for understanding what code the macro generates and debugging issues.

### Using cargo-expand

For more detailed macro expansion, use `cargo-expand`:

```bash
# Install cargo-expand
cargo install cargo-expand

# Expand all macros in a file
cargo expand --lib path::to::module

# Expand only a specific item
cargo expand --lib my_server::MyServer
```

## Macro Attributes Reference

### `#[mcp_server]`

```rust
#[mcp_server(
    name = "server-name",           // Required: server name
    version = "1.0.0",              // Required: server version
    instructions = "Usage guide",   // Optional: usage instructions
    debug_expand = false,           // Optional: print generated code
)]
impl MyServer {
    // Tools, resources, prompts go here
}
```

### `#[tool]`

```rust
#[tool(
    name = "custom_name",           // Optional: override method name
    description = "What it does",   // Required: tool description
    destructive = false,            // Optional: marks tool as destructive
    idempotent = true,              // Optional: can be called multiple times safely
    read_only = true,               // Optional: doesn't modify state
    params(                         // Optional: parameter descriptions
        arg1(description = "First argument"),
        arg2(description = "Second argument"),
    ),
)]
async fn my_tool(&self, arg1: String, arg2: i32) -> ToolOutput {
    // Implementation
}
```

### `#[resource]`

```rust
#[resource(
    uri_pattern = "scheme://{id}",  // Required: URI pattern with placeholders
    name = "Resource Name",         // Required: human-readable name
    description = "Description",    // Optional: resource description
    mime_type = "text/plain",       // Optional: MIME type
)]
async fn my_resource(&self, uri: &str) -> Result<ResourceContents, McpError> {
    // Implementation
}
```

### `#[prompt]`

```rust
#[prompt(
    name = "prompt_name",           // Optional: override method name
    description = "What it does",   // Optional: prompt description
    params(                         // Optional: parameter descriptions
        topic(description = "The topic"),
    ),
)]
async fn my_prompt(&self, topic: String) -> PromptResult {
    // Implementation
}
```

## Common Issues and Solutions

### Issue: "cannot find type `ServerHandler` in this scope"

**Cause:** Missing import from `mcpkit_server`.

**Solution:**
```rust
use mcpkit::prelude::*;
// Or
use mcpkit_server::handler::ServerHandler;
```

### Issue: "the trait bound `X: Deserialize<'_>` is not satisfied"

**Cause:** Tool parameter types must implement `Deserialize`.

**Solution:**
```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct MyInput {
    field: String,
}

#[tool(description = "Example")]
async fn my_tool(&self, input: MyInput) -> ToolOutput {
    // ...
}
```

### Issue: "`async fn` is not permitted in traits"

**Cause:** Using wrong return type or missing `async_trait`.

**Solution:**
The macros handle async automatically. Ensure you're using `async fn`:
```rust
#[tool(description = "Example")]
async fn my_tool(&self) -> ToolOutput {  // Note: async fn
    ToolOutput::text("result")
}
```

### Issue: "expected struct `ToolOutput`, found `Result<_, _>`"

**Cause:** Tool returning `Result` when signature expects `ToolOutput`.

**Solution:**
Choose one pattern:
```rust
// Pattern 1: Return ToolOutput directly (handle errors inline)
#[tool(description = "Example")]
async fn my_tool(&self) -> ToolOutput {
    match do_something() {
        Ok(val) => ToolOutput::text(val),
        Err(e) => ToolOutput::error(e.to_string()),
    }
}

// Pattern 2: Return Result (errors propagate as McpError)
#[tool(description = "Example")]
async fn my_tool(&self) -> Result<ToolOutput, McpError> {
    let val = do_something()?;
    Ok(ToolOutput::text(val))
}
```

### Issue: "no method named `build` found for struct"

**Cause:** Typestate builder not in correct state.

**Solution:**
Ensure all required builder methods are called:
```rust
// Wrong: missing transport
let server = ServerBuilder::new("name", "1.0.0")
    .with_tool_handler(handler)
    .build();  // Error!

// Correct: all required fields set
let server = ServerBuilder::new("name", "1.0.0")
    .with_transport(transport)
    .with_tool_handler(handler)
    .build();
```

### Issue: "this function takes X arguments but Y arguments were supplied"

**Cause:** Tool parameter count mismatch with generated schema.

**Solution:**
Ensure tool parameters match the function signature:
```rust
#[tool(
    description = "Example",
    params(
        a(description = "First"),
        b(description = "Second"),  // Must match function params
    )
)]
async fn my_tool(&self, a: String, b: i32) -> ToolOutput {
    // ...
}
```

### Issue: "the trait `ToolHandler` is not implemented"

**Cause:** Tool handler not properly registered.

**Solution:**
Use the correct builder method:
```rust
// For #[mcp_server] macro:
ServerBuilder::new(MyServer)
    .with_tools(MyServer)  // Register the same type as tool handler
    .build()

// For manual handler:
ServerBuilder::new("name", "1.0.0")
    .with_transport(transport)
    .with_tool_handler(MyToolHandler::new())
    .build()
```

### Issue: "expected `&str`, found `String`"

**Cause:** Resource handler URI parameter type mismatch.

**Solution:**
```rust
// Correct: uri parameter should be &str
#[resource(uri_pattern = "file://{path}", name = "File")]
async fn get_file(&self, uri: &str) -> Result<ResourceContents, McpError> {
    // ...
}
```

### Issue: JSON Schema not generated correctly

**Cause:** Complex types without proper serde attributes.

**Solution:**
Use `#[derive(ToolInput)]` for complex input types:
```rust
use mcpkit_macros::ToolInput;
use serde::Deserialize;

#[derive(Deserialize, ToolInput)]
struct SearchInput {
    #[doc = "Search query"]
    query: String,

    #[doc = "Maximum results"]
    #[serde(default)]
    limit: Option<u32>,
}

#[tool(description = "Search")]
async fn search(&self, input: SearchInput) -> ToolOutput {
    // ...
}
```

## Debugging Techniques

### 1. Enable Debug Output

```rust
#[mcp_server(name = "test", version = "1.0", debug_expand = true)]
```

### 2. Check Compiler Errors First

Macro expansion happens before type checking. Ensure:
- All imports are present
- Types implement required traits
- Signatures are correct

### 3. Simplify and Isolate

When debugging, start with the simplest possible server:

```rust
struct MinimalServer;

#[mcp_server(name = "minimal", version = "1.0.0", debug_expand = true)]
impl MinimalServer {
    #[tool(description = "Test")]
    async fn test(&self) -> ToolOutput {
        ToolOutput::text("ok")
    }
}
```

Then gradually add complexity.

### 4. Check Generated Trait Implementations

The `#[mcp_server]` macro generates these implementations:

- `ServerHandler` - Core server info
- `ToolHandler` - Tool listing and calling (if tools defined)
- `ResourceHandler` - Resource listing and reading (if resources defined)
- `PromptHandler` - Prompt listing and getting (if prompts defined)

### 5. Verify JSON Schema

Test that tool schemas serialize correctly:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schema() {
        let server = MyServer::new();
        let tools = server.list_tools();

        for tool in tools {
            println!("Tool: {}", tool.name);
            println!("Schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap());
        }
    }
}
```

### 6. Use RUST_BACKTRACE

For panics during macro expansion:

```bash
RUST_BACKTRACE=1 cargo build
```

## Advanced Patterns

### Conditional Tool Availability

```rust
#[mcp_server(name = "conditional", version = "1.0.0")]
impl ConditionalServer {
    // This tool is always available
    #[tool(description = "Always available")]
    async fn always(&self) -> ToolOutput {
        ToolOutput::text("ok")
    }

    // For conditional tools, implement ToolHandler manually
}

// Custom implementation for conditional tools
#[async_trait]
impl ToolHandler for ConditionalServer {
    async fn list_tools(&self, ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        let mut tools = vec![
            Tool::new("always").description("Always available"),
        ];

        if self.advanced_mode {
            tools.push(Tool::new("advanced").description("Advanced only"));
        }

        Ok(tools)
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "always" => self.always().await,
            "advanced" if self.advanced_mode => self.advanced(args).await,
            _ => Err(McpError::method_not_found(name)),
        }
    }
}
```

### Dynamic Tool Registration

```rust
use std::collections::HashMap;
use std::sync::Arc;

type DynamicTool = Arc<dyn Fn(Value) -> BoxFuture<'static, ToolOutput> + Send + Sync>;

struct DynamicServer {
    tools: Arc<RwLock<HashMap<String, (Tool, DynamicTool)>>>,
}

impl DynamicServer {
    async fn register_tool<F, Fut>(&self, tool: Tool, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolOutput> + Send + 'static,
    {
        let wrapped: DynamicTool = Arc::new(move |args| Box::pin(handler(args)));
        self.tools.write().await.insert(tool.name.clone(), (tool, wrapped));
    }
}

#[async_trait]
impl ToolHandler for DynamicServer {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        let tools = self.tools.read().await;
        Ok(tools.values().map(|(t, _)| t.clone()).collect())
    }

    async fn call_tool(
        &self,
        name: &str,
        args: Value,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        let tools = self.tools.read().await;
        if let Some((_, handler)) = tools.get(name) {
            Ok(handler(args).await)
        } else {
            Err(McpError::method_not_found(name))
        }
    }
}
```

## Macro Expansion Examples

### Minimal Tool

Input:
```rust
#[mcp_server(name = "test", version = "1.0.0")]
impl MyServer {
    #[tool(description = "Say hello")]
    async fn hello(&self) -> ToolOutput {
        ToolOutput::text("Hello!")
    }
}
```

Expands to (simplified):
```rust
impl ServerHandler for MyServer {
    fn info(&self) -> ServerInfo {
        ServerInfo {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            instructions: None,
        }
    }
}

impl ToolHandler for MyServer {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "hello".to_string(),
                description: Some("Say hello".to_string()),
                input_schema: json!({"type": "object", "properties": {}}),
                annotations: None,
            }
        ])
    }

    async fn call_tool(
        &self,
        name: &str,
        _args: Value,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "hello" => Ok(self.hello().await),
            _ => Err(McpError::method_not_found(name)),
        }
    }
}
```

## Getting Help

If you're stuck:

1. Check this troubleshooting guide
2. Enable `debug_expand = true`
3. Use `cargo expand` for full expansion
4. Search [GitHub issues](https://github.com/praxiomlabs/mcpkit/issues)
5. Ask in the community forums
