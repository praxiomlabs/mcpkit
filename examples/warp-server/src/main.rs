//! MCP server using Warp framework.
//!
//! This example demonstrates how to create an MCP server using the Warp web framework.
//! It provides tools for greeting users and performing basic calculations.
//!
//! ## Running the server
//!
//! ```bash
//! cargo run -p warp-server-example
//! ```
//!
//! The server will start on `http://0.0.0.0:3000`.
//!
//! ## Endpoints
//!
//! - `POST /mcp` - JSON-RPC endpoint for MCP requests
//! - `GET /mcp/sse` - Server-Sent Events endpoint for streaming

use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    GetPromptResult, Prompt, PromptArgument, PromptMessage, Resource, ResourceContents, Tool,
    ToolOutput,
};
use mcpkit_server::ServerHandler;
use mcpkit_server::context::Context;
use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};
use mcpkit_warp::McpRouter;
use serde_json::json;
use tracing::info;

/// MCP server handler implementing tools, resources, and prompts.
struct WarpHandler;

impl ServerHandler for WarpHandler {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("warp-mcp-server", env!("CARGO_PKG_VERSION"))
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new()
            .with_tools()
            .with_resources()
            .with_prompts()
    }
}

impl ToolHandler for WarpHandler {
    async fn list_tools(&self, _ctx: &Context<'_>) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool::new("greet")
                .description("Greet a user by name")
                .input_schema(json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the person to greet"
                        }
                    },
                    "required": ["name"]
                })),
            Tool::new("calculate")
                .description("Perform basic arithmetic")
                .input_schema(json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["add", "subtract", "multiply", "divide"],
                            "description": "The operation to perform"
                        },
                        "a": {
                            "type": "number",
                            "description": "First operand"
                        },
                        "b": {
                            "type": "number",
                            "description": "Second operand"
                        }
                    },
                    "required": ["operation", "a", "b"]
                })),
            Tool::new("echo")
                .description("Echo back the input")
                .input_schema(json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to echo"
                        }
                    },
                    "required": ["message"]
                })),
        ])
    }

    async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        _ctx: &Context<'_>,
    ) -> Result<ToolOutput, McpError> {
        match name {
            "greet" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
                Ok(ToolOutput::text(format!(
                    "Hello, {name}! Welcome to the Warp MCP server."
                )))
            }
            "calculate" => {
                let op = args
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("add");
                let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);

                let result = match op {
                    "add" => a + b,
                    "subtract" => a - b,
                    "multiply" => a * b,
                    "divide" => {
                        if b == 0.0 {
                            return Err(McpError::tool_error("calculate", "Division by zero"));
                        }
                        a / b
                    }
                    _ => {
                        return Err(McpError::tool_error(
                            "calculate",
                            format!("Unknown operation: {op}"),
                        ));
                    }
                };

                Ok(ToolOutput::text(format!("{a} {op} {b} = {result}")))
            }
            "echo" => {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(empty)");
                Ok(ToolOutput::text(format!("Echo: {message}")))
            }
            _ => Err(McpError::tool_error(name, "Tool not found")),
        }
    }
}

impl ResourceHandler for WarpHandler {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(vec![
            Resource::new("warp://info", "Server Info")
                .description("Information about this Warp MCP server")
                .mime_type("application/json"),
            Resource::new("warp://readme", "README")
                .description("Server documentation")
                .mime_type("text/plain"),
        ])
    }

    async fn read_resource(
        &self,
        uri: &str,
        _ctx: &Context<'_>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        match uri {
            "warp://info" => {
                let info = json!({
                    "name": "Warp MCP Server",
                    "version": env!("CARGO_PKG_VERSION"),
                    "framework": "Warp 0.3",
                    "features": ["tools", "resources", "prompts", "cors"]
                });
                Ok(vec![ResourceContents::text(
                    uri,
                    serde_json::to_string_pretty(&info).unwrap(),
                )])
            }
            "warp://readme" => {
                let readme = r#"
# Warp MCP Server

This is an example MCP server built with the Warp web framework.

## Available Tools

- `greet`: Greet a user by name
- `calculate`: Perform basic arithmetic (add, subtract, multiply, divide)
- `echo`: Echo back the input message

## Available Prompts

- `welcome`: Generate a welcome message

## Usage

Send JSON-RPC requests to POST /mcp with the `mcp-protocol-version` header.

### Example Request

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "mcp-protocol-version: 2025-11-25" \
  -d '{"jsonrpc":"2.0","method":"tools/list","id":1}'
```
"#;
                Ok(vec![ResourceContents::text(uri, readme.trim())])
            }
            _ => Err(McpError::resource_not_found(uri)),
        }
    }
}

impl PromptHandler for WarpHandler {
    async fn list_prompts(&self, _ctx: &Context<'_>) -> Result<Vec<Prompt>, McpError> {
        Ok(vec![
            Prompt::new("welcome")
                .description("Generate a welcome message for a user")
                .argument(PromptArgument::required("name", "The user's name")),
        ])
    }

    async fn get_prompt(
        &self,
        name: &str,
        args: Option<serde_json::Map<String, serde_json::Value>>,
        _ctx: &Context<'_>,
    ) -> Result<GetPromptResult, McpError> {
        match name {
            "welcome" => {
                let user_name = args
                    .as_ref()
                    .and_then(|a| a.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("User");

                Ok(GetPromptResult {
                    description: Some(format!("Welcome message for {user_name}")),
                    messages: vec![PromptMessage::user(format!(
                        "Please write a warm and friendly welcome message for {user_name} \
                         who is trying out the Warp MCP server."
                    ))],
                })
            }
            _ => Err(McpError::method_not_found(format!("prompts/get:{name}"))),
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("warp_server_example=info".parse().unwrap())
                .add_directive("mcpkit=debug".parse().unwrap()),
        )
        .init();

    info!("Starting Warp MCP server on http://0.0.0.0:3000");

    // Create and serve the MCP router
    McpRouter::new(WarpHandler)
        .with_cors()
        .serve(([0, 0, 0, 0], 3000))
        .await;
}
