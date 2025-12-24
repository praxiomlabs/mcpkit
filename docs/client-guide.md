# Building MCP Clients with mcpkit

This guide walks you through building MCP clients using the Rust MCP SDK. MCP clients connect to MCP servers to invoke tools, read resources, and interact with AI capabilities.

## Prerequisites

- Rust 1.85 or later
- Basic familiarity with async Rust (Tokio)
- An MCP server to connect to (or use the examples in this repository)

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
mcpkit = "0.3"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

For more granular control, use individual crates:

```toml
[dependencies]
mcpkit-client = "0.3"
mcpkit-transport = "0.3"
mcpkit-core = "0.3"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

Here's a minimal example that connects to an MCP server and lists available tools:

```rust
use mcpkit::prelude::*;
use mcpkit::transport::SpawnedTransport;
use mcpkit::client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Spawn an MCP server and connect to it
    let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;

    // Build the client
    let client = ClientBuilder::new()
        .name("my-client")
        .version("1.0.0")
        .build(transport)
        .await?;

    // List available tools
    let tools = client.list_tools().await?;
    for tool in &tools {
        println!("Tool: {} - {}", tool.name, tool.description.as_deref().unwrap_or(""));
    }

    Ok(())
}
```

## Connecting to Servers

### Spawning a Server Process

The most common way to connect is by spawning an MCP server as a subprocess:

```rust
use mcpkit::transport::SpawnedTransport;

// Simple spawn
let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;

// With command-line arguments
let transport = SpawnedTransport::spawn("my-server", &["--verbose", "--port", "8080"]).await?;

// Using the builder for more control
let transport = SpawnedTransport::builder("my-mcp-server")
    .arg("--config")
    .arg("/path/to/config.json")
    .env("LOG_LEVEL", "debug")
    .working_dir("/path/to/server")
    .spawn()
    .await?;
```

### HTTP Transport

For HTTP-based MCP servers:

```rust
use mcpkit::transport::http::HttpTransport;

let transport = HttpTransport::connect("http://localhost:8080/mcp").await?;
```

### WebSocket Transport

For WebSocket connections:

```rust
use mcpkit::transport::websocket::WebSocketTransport;

let transport = WebSocketTransport::connect("ws://localhost:8080/mcp").await?;
```

## Building the Client

Use `ClientBuilder` to configure and create a client:

```rust
use mcpkit::client::ClientBuilder;

let client = ClientBuilder::new()
    .name("my-client")        // Client name (shown to servers)
    .version("1.0.0")         // Client version
    .with_sampling()          // Enable sampling capability
    .with_roots()             // Enable roots capability
    .with_elicitation()       // Enable elicitation capability
    .build(transport)
    .await?;
```

### Client Capabilities

Enable capabilities based on what your client supports:

| Method | Description |
|--------|-------------|
| `with_sampling()` | Client can handle LLM requests from servers |
| `with_elicitation()` | Client can handle user input requests |
| `with_roots()` | Client exposes filesystem roots |
| `with_roots_and_changes()` | Roots with change notifications |

## Querying Server Capabilities

After connecting, query what the server supports:

```rust
// Server information
let info = client.server_info();
println!("Connected to: {} v{}", info.name, info.version);

// Check capabilities
if client.has_tools() {
    println!("Server supports tools");
}
if client.has_resources() {
    println!("Server supports resources");
}
if client.has_prompts() {
    println!("Server supports prompts");
}
if client.has_tasks() {
    println!("Server supports long-running tasks");
}

// Server instructions (if provided)
if let Some(instructions) = client.instructions() {
    println!("Server instructions: {}", instructions);
}

// Protocol version
let version = client.protocol_version();
println!("Negotiated protocol: {}", version);
```

## Working with Tools

### Listing Tools

```rust
// List all tools
let tools = client.list_tools().await?;
for tool in &tools {
    println!("Tool: {}", tool.name);
    if let Some(desc) = &tool.description {
        println!("  Description: {}", desc);
    }
    if let Some(schema) = &tool.input_schema {
        println!("  Schema: {}", serde_json::to_string_pretty(schema)?);
    }
}

// With pagination (for servers with many tools)
let result = client.list_tools_paginated(None).await?;
for tool in &result.tools {
    println!("Tool: {}", tool.name);
}
if let Some(cursor) = result.next_cursor {
    // Fetch next page
    let next_page = client.list_tools_paginated(Some(&cursor)).await?;
}
```

### Calling Tools

```rust
use serde_json::json;

// Simple tool call
let result = client.call_tool("add", json!({
    "a": 10,
    "b": 20
})).await?;

// Process the result
for content in &result.content {
    match content {
        mcpkit_core::content::Content::Text(text) => {
            println!("Result: {}", text.text);
        }
        mcpkit_core::content::Content::Image(img) => {
            println!("Got image: {} bytes", img.data.len());
        }
        mcpkit_core::content::Content::Resource(res) => {
            println!("Got resource: {}", res.uri);
        }
    }
}

// Check for errors
if result.is_error.unwrap_or(false) {
    eprintln!("Tool reported an error");
}
```

## Working with Resources

### Listing Resources

```rust
// List all resources
let resources = client.list_resources().await?;
for resource in &resources {
    println!("Resource: {} ({})", resource.name, resource.uri);
    if let Some(mime) = &resource.mime_type {
        println!("  MIME type: {}", mime);
    }
}

// List resource templates
let templates = client.list_resource_templates().await?;
for template in &templates {
    println!("Template: {} ({})", template.name, template.uri_template);
}
```

### Reading Resources

```rust
// Read a resource by URI
let contents = client.read_resource("file:///path/to/file.txt").await?;

for content in &contents {
    println!("URI: {}", content.uri);
    if let Some(text) = &content.text {
        println!("Content: {}", text);
    }
    if let Some(blob) = &content.blob {
        println!("Binary data: {} bytes", blob.len());
    }
}
```

### Resource Subscriptions

Subscribe to resource changes:

```rust
// Subscribe to updates
client.subscribe_resource("file:///config.json").await?;

// Later, unsubscribe
client.unsubscribe_resource("file:///config.json").await?;
```

To receive notifications, implement `ClientHandler` (see below).

## Working with Prompts

### Listing Prompts

```rust
let prompts = client.list_prompts().await?;
for prompt in &prompts {
    println!("Prompt: {}", prompt.name);
    if let Some(desc) = &prompt.description {
        println!("  Description: {}", desc);
    }
    if let Some(args) = &prompt.arguments {
        for arg in args {
            println!("  Argument: {} (required: {})",
                arg.name,
                arg.required.unwrap_or(false));
        }
    }
}
```

### Getting Prompts

```rust
use serde_json::Map;

// Get a prompt with arguments
let mut args = Map::new();
args.insert("topic".to_string(), serde_json::json!("rust programming"));
args.insert("style".to_string(), serde_json::json!("technical"));

let result = client.get_prompt("explain_topic", Some(args)).await?;

// Process the messages
for message in &result.messages {
    println!("{}: {:?}", message.role, message.content);
}
```

## Working with Tasks

For servers that support long-running tasks:

```rust
// List all tasks
let tasks = client.list_tasks().await?;
for task in &tasks {
    println!("Task: {} - {:?}", task.id, task.status);
}

// Filter by status
use mcpkit_core::types::TaskStatus;
let running = client.list_tasks_filtered(
    Some(TaskStatus::Running),
    None
).await?;

// Get task details
let task = client.get_task("task-id-here").await?;
println!("Task status: {:?}", task.status);

// Cancel a running task
client.cancel_task("task-id-here").await?;
```

## Completions

Get argument completions for prompts or resources:

```rust
// Complete a prompt argument
let result = client.complete_prompt_argument(
    "my_prompt",
    "argument_name",
    "partial_value"
).await?;

for value in &result.completion.values {
    println!("Suggestion: {}", value);
}

// Complete a resource argument
let result = client.complete_resource_argument(
    "file:///{path}",
    "path",
    "/home/user/"
).await?;
```

## Handling Server-Initiated Requests

Servers can request actions from clients. You have two options:

### Option 1: Using the `#[mcp_client]` Macro (Recommended)

The `#[mcp_client]` macro provides a declarative way to implement handlers:

```rust
use mcpkit::prelude::*;
use mcpkit::client::handler::Root;
use mcpkit_core::types::{
    CreateMessageRequest, CreateMessageResult, Role,
    ElicitRequest, ElicitResult, ElicitAction,
};

struct MyHandler;

#[mcp_client]
impl MyHandler {
    /// Handle LLM sampling requests from servers.
    #[sampling]
    async fn handle_sampling(
        &self,
        request: CreateMessageRequest,
    ) -> Result<CreateMessageResult, McpError> {
        // Call your LLM here
        let response_text = request.messages
            .last()
            .map(|m| format!("Echo: {:?}", m.content))
            .unwrap_or_default();

        Ok(CreateMessageResult {
            role: Role::Assistant,
            content: Content::text(response_text),
            model: "echo-model".to_string(),
            stop_reason: Some("end_turn".to_string()),
        })
    }

    /// Handle elicitation (user input) requests.
    #[elicitation]
    async fn handle_elicitation(
        &self,
        request: ElicitRequest,
    ) -> Result<ElicitResult, McpError> {
        println!("Server asks: {}", request.message);
        Ok(ElicitResult {
            action: ElicitAction::Accept,
            content: Some(serde_json::json!({"answer": "user response"})),
        })
    }

    /// List filesystem roots.
    #[roots]
    fn get_roots(&self) -> Vec<Root> {
        vec![
            Root::new("file:///home/user/project").name("Project"),
            Root::new("file:///home/user/documents").name("Documents"),
        ]
    }

    /// Called when connected to a server.
    #[on_connected]
    async fn handle_connected(&self) {
        println!("Connected to server!");
    }

    /// Called when disconnected.
    #[on_disconnected]
    async fn handle_disconnected(&self) {
        println!("Disconnected from server");
    }

    /// Handle task progress notifications.
    #[on_task_progress]
    async fn handle_progress(&self, task_id: TaskId, progress: TaskProgress) {
        println!("Task {} progress: {}%", task_id, progress.progress * 100.0);
    }

    /// Handle resource update notifications.
    #[on_resource_updated]
    async fn handle_resource_updated(&self, uri: String) {
        println!("Resource updated: {}", uri);
    }

    /// Handle tools list change notifications.
    #[on_tools_list_changed]
    async fn handle_tools_changed(&self) {
        println!("Tools list changed - refresh cache");
    }
}

// Build client with handler - capabilities are automatically set!
let client = ClientBuilder::new()
    .name("my-client")
    .version("1.0.0")
    .build_with_handler(transport, MyHandler)
    .await?;
```

#### Handler Attributes

| Attribute | Description |
|-----------|-------------|
| `#[sampling]` | Handle `sampling/createMessage` requests |
| `#[elicitation]` | Handle `elicitation/elicit` requests |
| `#[roots]` | Handle `roots/list` requests |
| `#[on_connected]` | Called when connection is established |
| `#[on_disconnected]` | Called when connection is closed |
| `#[on_task_progress]` | Handle task progress notifications |
| `#[on_resource_updated]` | Handle resource update notifications |
| `#[on_tools_list_changed]` | Handle tools list change notifications |
| `#[on_resources_list_changed]` | Handle resources list change notifications |
| `#[on_prompts_list_changed]` | Handle prompts list change notifications |

### Option 2: Manual Implementation

You can also implement `ClientHandler` directly:

```rust
use mcpkit::client::{ClientBuilder, ClientHandler};
use mcpkit::client::handler::Root;

struct MyHandler;

impl ClientHandler for MyHandler {
    async fn create_message(
        &self,
        request: CreateMessageRequest,
    ) -> Result<CreateMessageResult, McpError> {
        // Implementation here
    }

    async fn list_roots(&self) -> Result<Vec<Root>, McpError> {
        Ok(vec![
            Root::new("file:///home/user/project").name("Project"),
        ])
    }

    async fn on_connected(&self) {
        println!("Connected!");
    }
}

// Build client with explicit capabilities
let client = ClientBuilder::new()
    .name("my-client")
    .version("1.0.0")
    .with_sampling()
    .with_roots()
    .build_with_handler(transport, MyHandler)
    .await?;
```

## Server Discovery

Find and manage MCP servers:

```rust
use mcpkit::client::{ServerDiscovery, DiscoveredServer};

// Create discovery instance
let mut discovery = ServerDiscovery::new();

// Add custom config path
let discovery = discovery.add_config_path("/custom/servers.json");

// Register servers manually
let discovery = discovery
    .register(DiscoveredServer::stdio("calculator", "calculator-server"))
    .register(DiscoveredServer::http("api", "http://localhost:8080")
        .description("API server")
        .env("API_KEY", "secret"));

// Discover from config files
discovery.discover()?;

// List discovered servers
for server in discovery.servers() {
    println!("Server: {} ({:?})", server.name, server.transport);
}

// Get a specific server
if let Some(server) = discovery.get("calculator") {
    println!("Found calculator server");
}
```

### Server Configuration Format

Create a `servers.json` file:

```json
{
  "servers": [
    {
      "name": "calculator",
      "transport": {
        "type": "stdio",
        "command": "/usr/local/bin/calculator-server",
        "args": ["--verbose"]
      },
      "description": "Math operations",
      "env": {
        "LOG_LEVEL": "debug"
      }
    },
    {
      "name": "web-api",
      "transport": {
        "type": "http",
        "url": "https://api.example.com/mcp"
      }
    }
  ]
}
```

Standard config locations:
- Linux: `~/.config/mcp/servers.json` or `~/.mcp/servers.json`
- macOS: `~/Library/Application Support/mcp/servers.json`
- Windows: `%APPDATA%\mcp\servers.json`

## Connection Pooling

Manage multiple client connections efficiently:

```rust
use mcpkit::client::{ClientPool, ClientPoolBuilder, PoolConfig};

// Build a connection pool
let pool = ClientPoolBuilder::new()
    .max_connections(10)
    .idle_timeout(Duration::from_secs(300))
    .build();

// Get a client from the pool
let client = pool.get("calculator").await?;

// Use the client
let result = client.call_tool("add", json!({"a": 1, "b": 2})).await?;

// Client is returned to pool when dropped

// Pool statistics
let stats = pool.stats();
println!("Active: {}, Idle: {}", stats.active, stats.idle);
```

## Connection Lifecycle

### Checking Connection State

```rust
// Check if connected
if client.is_connected() {
    println!("Client is connected");
}

// Ping the server
client.ping().await?;
```

### Closing Connections

```rust
// Graceful close
client.close().await?;

// For SpawnedTransport, you can also:
// - Wait for the process to exit
// - Kill it forcefully if needed
```

## Error Handling

```rust
use mcpkit_core::error::McpError;

match client.call_tool("unknown_tool", json!({})).await {
    Ok(result) => {
        println!("Success: {:?}", result);
    }
    Err(McpError::CapabilityNotSupported { capability, available }) => {
        eprintln!("Server doesn't support {}", capability);
        eprintln!("Available: {:?}", available);
    }
    Err(McpError::Transport(details)) => {
        eprintln!("Transport error: {}", details.message);
    }
    Err(McpError::Internal { message, .. }) => {
        eprintln!("Internal error: {}", message);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Best Practices

### 1. Check Capabilities Before Use

```rust
// Always check before using optional features
if client.has_tools() {
    let tools = client.list_tools().await?;
}
```

### 2. Handle Reconnection

```rust
async fn with_reconnect<F, T>(
    mut connect: impl FnMut() -> F,
    mut operation: impl FnMut(&Client) -> T,
) -> Result<T::Output, McpError>
where
    F: Future<Output = Result<Client, McpError>>,
    T: Future<Output = Result<_, McpError>>,
{
    let mut retries = 3;
    loop {
        let client = connect().await?;
        match operation(&client).await {
            Ok(result) => return Ok(result),
            Err(McpError::Transport(_)) if retries > 0 => {
                retries -= 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}
```

### 3. Use Timeouts

```rust
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(30),
    client.call_tool("slow_operation", json!({}))
).await??;
```

### 4. Cache Server Information

```rust
// Cache tool list to avoid repeated queries
let tools = client.list_tools().await?;
let tool_map: HashMap<String, Tool> = tools
    .into_iter()
    .map(|t| (t.name.clone(), t))
    .collect();

// Use cached data
if tool_map.contains_key("add") {
    client.call_tool("add", json!({"a": 1, "b": 2})).await?;
}
```

### 5. Handle List Change Notifications

```rust
impl ClientHandler for MyHandler {
    async fn on_tools_list_changed(&self) {
        // Invalidate cache and refresh
        self.tool_cache.clear().await;
        // Re-fetch tools on next access
    }
}
```

## Complete Example

See the full client example in the repository:

```bash
cargo run -p client-example
```

This example:
1. Builds and spawns the filesystem-server
2. Connects to it
3. Lists and calls tools
4. Demonstrates error handling
5. Shows resource and prompt operations

## Next Steps

- [Transport Options](./transports.md) - Different ways to connect
- [Error Handling](./error-handling.md) - Comprehensive error handling
- [Production Deployment](./production-deployment.md) - Deploy clients in production
- [Troubleshooting](./troubleshooting.md) - Common issues and solutions

## See Also

- [API Reference](https://docs.rs/mcpkit-client)
- [MCP Specification](https://modelcontextprotocol.io)
- [Example Client](../examples/client-example/)
