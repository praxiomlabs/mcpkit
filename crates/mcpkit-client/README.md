# mcpkit-client

Client implementation for the Model Context Protocol (MCP).

This crate provides the client-side implementation including a fluent client API, server discovery, and connection pooling.

## Overview

The MCP client allows AI applications to:

- Connect to MCP servers via various transports
- Discover and invoke tools
- Read resources
- Get prompts
- Track long-running tasks
- Handle server-initiated requests (sampling, elicitation)

## Usage

```rust
use mcpkit_client::ClientBuilder;
use mcpkit_transport::SpawnedTransport;

#[tokio::main]
async fn main() -> Result<(), mcpkit_core::error::McpError> {
    // Spawn an MCP server as a subprocess
    let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;

    let client = ClientBuilder::new()
        .name("my-client")
        .version("1.0.0")
        .build(transport)
        .await?;

    // List available tools
    let tools = client.list_tools().await?;

    // Call a tool
    let result = client.call_tool("add", serde_json::json!({
        "a": 1,
        "b": 2
    })).await?;

    Ok(())
}
```

## Features

### Server Discovery

Discover MCP servers from configuration:

```rust
use mcpkit_client::ServerDiscovery;

let discovery = ServerDiscovery::new();
discovery.register("my-server", config);
let server = discovery.get("my-server");
```

### Connection Pooling

Manage multiple client connections:

```rust
use mcpkit_client::{ClientPool, PoolConfig};

let pool = ClientPool::builder()
    .max_connections(10)
    .build();
```

### Client Handler

Handle server-initiated requests by implementing `ClientHandler`:

```rust
use mcpkit_client::ClientHandler;

struct MyHandler;

impl ClientHandler for MyHandler {
    // Handle sampling requests, elicitation, etc.
}
```

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
