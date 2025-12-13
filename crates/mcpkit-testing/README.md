# mcpkit-testing

Testing utilities for the Model Context Protocol (MCP).

This crate provides mocks, fixtures, and assertions for testing MCP servers and clients.

## Features

- Mock servers and clients for unit testing
- Test fixtures with pre-configured tools/resources
- Custom assertions for MCP-specific scenarios

## Usage

### Mock Server

```rust
use mcpkit_testing::{MockServer, MockTool};
use mcpkit_core::types::ToolOutput;

let server = MockServer::new()
    .tool(MockTool::new("add")
        .description("Add two numbers")
        .handler(|args| Ok(ToolOutput::text("42"))))
    .build();

// Use in tests with MemoryTransport
```

### Test Fixtures

```rust
use mcpkit_testing::fixtures;

let tools = fixtures::sample_tools();
let resources = fixtures::sample_resources();
```

### Assertions

```rust
use mcpkit_testing::assert_tool_result;
use mcpkit_core::types::CallToolResult;

let result = CallToolResult::text("42");
assert_tool_result!(result, "42");
```

## Exports

| Export | Purpose |
|--------|---------|
| `MockServer` | Mock MCP server for testing |
| `MockServerBuilder` | Builder for mock servers |
| `MockTool` | Mock tool definition |
| `assert_tool_success` | Assert tool call succeeded |
| `assert_tool_error` | Assert tool call failed |
| `sample_tools` | Pre-configured test tools |
| `sample_resources` | Pre-configured test resources |

## Part of mcpkit

This crate is part of the [mcpkit](https://crates.io/crates/mcpkit) SDK. For most use cases, depend on `mcpkit` directly rather than this crate.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
