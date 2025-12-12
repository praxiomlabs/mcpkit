# mcpkit-transport

Transport abstractions for the Model Context Protocol (MCP).

This crate provides transport layer implementations that handle the low-level details of sending and receiving JSON-RPC messages between MCP clients and servers.

## Overview

The transport layer is responsible for:

- Serializing and deserializing JSON-RPC messages
- Managing connection lifecycle
- Providing different transport implementations

## Available Transports

| Transport | Description |
|-----------|-------------|
| `StdioTransport` | Standard I/O for subprocess communication |
| `SpawnedTransport` | Spawn and connect to a subprocess |
| `MemoryTransport` | In-memory transport for testing |
| `HttpTransport` | HTTP/SSE transport (with `http` feature) |
| `WebSocketTransport` | WebSocket transport (with `websocket` feature) |
| `UnixTransport` | Unix domain sockets (Unix only) |

## Usage

```rust
use mcpkit_transport::{Transport, SpawnedTransport};

#[tokio::main]
async fn main() -> Result<(), mcpkit_transport::TransportError> {
    // Spawn an MCP server as a subprocess
    let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;

    // Receive messages
    while let Some(msg) = transport.recv().await? {
        // Handle the message
    }

    transport.close().await?;
    Ok(())
}
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `tokio-runtime` (default) | Use Tokio for async I/O |
| `async-std-runtime` | Use async-std for async I/O |
| `http` | Enable HTTP/SSE transport |
| `websocket` | Enable WebSocket transport |

## Middleware

The transport layer supports middleware for:

- Logging and telemetry
- Message transformation
- Connection pooling

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
