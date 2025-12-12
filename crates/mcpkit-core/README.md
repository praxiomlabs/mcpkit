# mcpkit-core

Core types and traits for the Model Context Protocol (MCP).

This crate provides the foundational building blocks for the MCP SDK:

- **Protocol types**: JSON-RPC 2.0 request/response/notification types
- **MCP types**: Tools, resources, prompts, tasks, content, sampling, elicitation
- **Capability negotiation**: Client and server capabilities
- **Error handling**: Unified `McpError` type with rich diagnostics
- **Typestate connection**: Compile-time enforced connection lifecycle
- **OAuth 2.1 / PKCE**: Authentication primitives

## Runtime Agnostic

This crate does not depend on any async runtime. It can be used with Tokio, async-std, smol, or any other executor.

## Protocol Version

Implements MCP protocol version **2025-11-25** with backward compatibility for **2024-11-05**.

## Usage

```rust
use mcpkit_core::{
    types::{Tool, ToolOutput, Content},
    capability::{ServerCapabilities, ServerInfo},
};

// Create a tool definition
let tool = Tool::new("search")
    .description("Search the database")
    .input_schema(serde_json::json!({
        "type": "object",
        "properties": {
            "query": { "type": "string" }
        },
        "required": ["query"]
    }));

// Create server capabilities
let caps = ServerCapabilities::new()
    .with_tools()
    .with_resources()
    .with_tasks();

// Create server info
let info = ServerInfo::new("my-server", "1.0.0");
```

## Feature Flags

- `fancy-errors` - Enable miette's fancy error reporting with terminal colors

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
