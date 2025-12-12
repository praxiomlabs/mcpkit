# mcpkit-server

Server implementation for the Model Context Protocol (MCP).

This crate provides the server-side implementation including composable handler traits, a fluent builder API, and request routing.

## Overview

Building an MCP server involves:

1. Implementing the `ServerHandler` trait (required)
2. Implementing optional capability traits (`ToolHandler`, `ResourceHandler`, etc.)
3. Using `ServerBuilder` to create a configured server
4. Running the server with a transport

## Usage

```rust
use mcpkit_server::{ServerBuilder, ServerHandler};
use mcpkit_core::capability::{ServerInfo, ServerCapabilities};

struct MyServer;

impl ServerHandler for MyServer {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("my-server", "1.0.0")
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new().with_tools()
    }
}

let server = ServerBuilder::new(MyServer).build();
```

## Handler Traits

The server uses composable handler traits:

| Trait | Purpose |
|-------|---------|
| `ServerHandler` | Core trait required for all servers |
| `ToolHandler` | Handle tool discovery and execution |
| `ResourceHandler` | Handle resource discovery and reading |
| `PromptHandler` | Handle prompt discovery and rendering |
| `TaskHandler` | Handle long-running task operations |
| `SamplingHandler` | Handle server-initiated LLM requests |
| `ElicitationHandler` | Handle structured user input requests |
| `CompletionHandler` | Handle argument completion requests |
| `LoggingHandler` | Handle log level changes |

## Context

Handlers receive a `Context` that provides:

- Request metadata (ID, progress token)
- Client and server capabilities
- Protocol version for feature detection
- Cancellation checking
- Progress reporting
- Notification sending

## Metrics

Built-in request metrics collection via `ServerMetrics`:

- Request counts by method
- Success/error rates
- Duration tracking

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
