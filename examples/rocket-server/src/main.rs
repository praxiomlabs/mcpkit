//! MCP server using Rocket framework.
//!
//! This example demonstrates how to create an MCP server using the Rocket web framework.
//! It provides tools for greeting users and performing basic calculations.
//!
//! ## Running the server
//!
//! ```bash
//! cargo run -p rocket-server-example
//! ```
//!
//! The server will start on `http://localhost:8000`.
//!
//! ## Endpoints
//!
//! - `POST /mcp` - JSON-RPC endpoint for MCP requests
//! - `GET /mcp/sse` - Server-Sent Events endpoint for streaming

use mcpkit_core::capability::{ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{GetPromptResult, Prompt, PromptArgument, PromptMessage, Resource, ResourceContents, Tool, ToolOutput};
use mcpkit_rocket::{McpRouter, McpState, create_mcp_routes};
use mcpkit_server::context::Context;
use mcpkit_server::handler::{PromptHandler, ResourceHandler, ToolHandler};
use mcpkit_server::ServerHandler;
use serde_json::json;
use tracing::info;

/// MCP server handler implementing tools, resources, and prompts.
struct RocketHandler;

impl ServerHandler for RocketHandler {
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new("rocket-mcp-server", env!("CARGO_PKG_VERSION"))
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new()
            .with_tools()
            .with_resources()
            .with_prompts()
    }
}

impl ToolHandler for RocketHandler {
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
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("World");
                Ok(ToolOutput::text(format!("Hello, {name}! Welcome to the Rocket MCP server.")))
            }
            "calculate" => {
                let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("add");
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
                    _ => return Err(McpError::tool_error("calculate", format!("Unknown operation: {op}"))),
                };

                Ok(ToolOutput::text(format!("{a} {op} {b} = {result}")))
            }
            _ => Err(McpError::tool_error(name, "Tool not found")),
        }
    }
}

impl ResourceHandler for RocketHandler {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(vec![
            Resource::new("rocket://info", "Server Info")
                .description("Information about this Rocket MCP server")
                .mime_type("application/json"),
            Resource::new("rocket://readme", "README")
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
            "rocket://info" => {
                let info = json!({
                    "name": "Rocket MCP Server",
                    "version": env!("CARGO_PKG_VERSION"),
                    "framework": "Rocket 0.5",
                    "features": ["tools", "resources", "prompts"]
                });
                Ok(vec![ResourceContents::text(uri, serde_json::to_string_pretty(&info).unwrap())])
            }
            "rocket://readme" => {
                let readme = r#"
# Rocket MCP Server

This is an example MCP server built with the Rocket web framework.

## Available Tools

- `greet`: Greet a user by name
- `calculate`: Perform basic arithmetic (add, subtract, multiply, divide)

## Available Prompts

- `welcome`: Generate a welcome message

## Usage

Send JSON-RPC requests to POST /mcp with the `mcp-protocol-version` header.
"#;
                Ok(vec![ResourceContents::text(uri, readme.trim())])
            }
            _ => Err(McpError::resource_not_found(uri)),
        }
    }
}

impl PromptHandler for RocketHandler {
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
                         who is trying out the Rocket MCP server."
                    ))],
                })
            }
            _ => Err(McpError::method_not_found(format!("prompts/get:{name}"))),
        }
    }
}

// Generate the MCP routes for RocketHandler
create_mcp_routes!(RocketHandler);

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rocket_server_example=info".parse().unwrap())
                .add_directive("mcpkit=debug".parse().unwrap()),
        )
        .init();

    info!("Starting Rocket MCP server...");

    // Create the MCP state
    let state: McpState<RocketHandler> = McpRouter::new(RocketHandler).into_state();

    // Build and launch Rocket
    let _ = rocket::build()
        .manage(state)
        .mount("/", rocket::routes![mcp_post, mcp_sse])
        .launch()
        .await?;

    Ok(())
}
