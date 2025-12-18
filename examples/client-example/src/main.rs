//! MCP Client Example
//!
//! This example demonstrates how to use the MCP client to connect to an MCP server,
//! perform initialization, discover capabilities, and call tools.
//!
//! # Running
//!
//! This client automatically builds and spawns the filesystem-server:
//!
//! ```bash
//! cargo run -p client-example
//! ```
//!
//! You can also test with any other MCP server that runs on stdio.

use mcpkit_core::protocol::{Message, Notification, Request, RequestId};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::Command as AsyncCommand,
};
use tracing::info;

/// MCP Protocol version.
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// Simple MCP client for demonstration purposes.
struct McpClient {
    /// Request ID counter.
    request_id: AtomicU64,
    /// Child process stdin.
    stdin: tokio::process::ChildStdin,
    /// Child process stdout reader.
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
    /// Server capabilities received during initialization.
    server_capabilities: Option<Value>,
    /// Server info received during initialization.
    server_info: Option<Value>,
}

impl McpClient {
    /// Spawn a server process and create a client connected to it.
    async fn spawn(command: &str, args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        info!(command = %command, "Spawning MCP server");

        let mut child = AsyncCommand::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stdout = tokio::io::BufReader::new(stdout);

        Ok(Self {
            request_id: AtomicU64::new(0),
            stdin,
            stdout,
            server_capabilities: None,
            server_info: None,
        })
    }

    /// Get the next request ID.
    fn next_id(&self) -> RequestId {
        RequestId::Number(self.request_id.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// Send a JSON-RPC request and wait for response.
    async fn request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let id = self.next_id();
        let method_owned = method.to_string();
        let request = if let Some(p) = params {
            Request::with_params(method_owned, id.clone(), p)
        } else {
            Request::new(method_owned, id.clone())
        };

        let msg = Message::Request(request);
        let json = serde_json::to_string(&msg)?;

        info!(method = %method, id = ?id, "Sending request");

        // Send request
        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        // Read response
        let mut line = String::new();
        self.stdout.read_line(&mut line).await?;

        let response: Message = serde_json::from_str(&line)?;

        match response {
            Message::Response(resp) => {
                if let Some(error) = resp.error {
                    Err(format!("RPC Error {}: {}", error.code, error.message).into())
                } else {
                    Ok(resp.result.unwrap_or(Value::Null))
                }
            }
            _ => Err("Expected response, got something else".into()),
        }
    }

    /// Send a notification (no response expected).
    async fn notify(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let method_owned = method.to_string();
        let notification = if let Some(p) = params {
            Notification::with_params(method_owned, p)
        } else {
            Notification::new(method_owned)
        };

        let msg = Message::Notification(notification);
        let json = serde_json::to_string(&msg)?;

        info!(method = %method, "Sending notification");

        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        Ok(())
    }

    /// Initialize the connection with the server.
    async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Initializing connection");

        let result = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "clientInfo": {
                        "name": "mcp-client-example",
                        "version": "1.0.0",
                    },
                    "capabilities": {},
                })),
            )
            .await?;

        self.server_info = result.get("serverInfo").cloned();
        self.server_capabilities = result.get("capabilities").cloned();

        info!(
            server_info = ?self.server_info,
            "Received initialize response"
        );

        // Send initialized notification
        self.notify("notifications/initialized", None).await?;

        info!("Initialization complete");
        Ok(())
    }

    /// List available tools.
    async fn list_tools(&mut self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let result = self.request("tools/list", None).await?;
        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(tools)
    }

    /// Call a tool.
    async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let result = self
            .request(
                "tools/call",
                Some(json!({
                    "name": name,
                    "arguments": arguments,
                })),
            )
            .await?;

        Ok(result)
    }

    /// List available resources.
    async fn list_resources(&mut self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let result = self.request("resources/list", None).await?;
        let resources = result
            .get("resources")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(resources)
    }

    /// Read a resource.
    #[allow(dead_code)]
    async fn read_resource(&mut self, uri: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let result = self
            .request(
                "resources/read",
                Some(json!({
                    "uri": uri,
                })),
            )
            .await?;

        Ok(result)
    }

    /// List available prompts.
    async fn list_prompts(&mut self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let result = self.request("prompts/list", None).await?;
        let prompts = result
            .get("prompts")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(prompts)
    }

    /// Get a prompt.
    #[allow(dead_code)]
    async fn get_prompt(
        &mut self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let mut params = json!({ "name": name });
        if let Some(args) = arguments {
            params["arguments"] = args;
        }

        let result = self.request("prompts/get", Some(params)).await?;
        Ok(result)
    }

    /// Send a ping request.
    async fn ping(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let _result = self.request("ping", None).await?;
        info!("Ping successful");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("client_example=info".parse().unwrap()),
        )
        .init();

    println!("MCP Client Example");
    println!("==================");
    println!();

    // Get the path to the minimal-server example
    // In a real application, you'd specify the actual server path
    let cargo_manifest = std::env::var("CARGO_MANIFEST_DIR")?;
    let workspace_root = std::path::Path::new(&cargo_manifest)
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to get workspace root");

    // Build the filesystem-server first (a real MCP server that serves on stdio)
    println!("Building filesystem-server example...");
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("filesystem-server")
        .current_dir(workspace_root)
        .status()?;

    if !status.success() {
        return Err("Failed to build filesystem-server".into());
    }

    // Find the built binary
    let server_binary = workspace_root
        .join("target")
        .join("debug")
        .join("filesystem-server");

    println!("Starting MCP server: {:?}", server_binary);
    println!();

    // Spawn the server and connect
    let mut client = McpClient::spawn(server_binary.to_str().unwrap(), &[]).await?;

    // Initialize connection
    client.initialize().await?;
    println!("Connected to server!");
    println!();

    // Ping the server
    println!("--- Ping ---");
    client.ping().await?;
    println!();

    // List and display tools
    println!("--- Available Tools ---");
    let tools = client.list_tools().await?;
    for tool in &tools {
        let name = tool["name"].as_str().unwrap_or("unknown");
        let description = tool["description"].as_str().unwrap_or("no description");
        println!("  {}: {}", name, description);
    }
    println!();

    // Call some tools
    println!("--- Calling Tools ---");

    // Get the sandbox root
    let result = client.call_tool("get_root", json!({})).await?;
    println!("get_root() = {:?}", extract_text_content(&result));

    // List directory
    let result = client.call_tool("list_directory", json!({})).await?;
    println!("list_directory() = {:?}", extract_text_content(&result));

    // Search for Rust files
    let result = client
        .call_tool(
            "search_files",
            json!({ "pattern": "*.rs", "max_results": 5 }),
        )
        .await?;
    println!("search_files(*.rs) = {:?}", extract_text_content(&result));

    // Test error handling - read non-existent file
    println!();
    println!("--- Testing Error Handling ---");
    let result = client
        .call_tool("read_file", json!({ "path": "nonexistent_file_12345.txt" }))
        .await?;
    let is_error = result["isError"].as_bool().unwrap_or(false);
    println!(
        "read_file(nonexistent) = {:?} (isError: {})",
        extract_text_content(&result),
        is_error
    );

    // List resources (if supported by the server)
    println!();
    println!("--- Available Resources ---");
    match client.list_resources().await {
        Ok(resources) => {
            if resources.is_empty() {
                println!("  (no resources available)");
            } else {
                for resource in &resources {
                    let uri = resource["uri"].as_str().unwrap_or("unknown");
                    let name = resource["name"].as_str().unwrap_or("unknown");
                    println!("  {}: {}", uri, name);
                }
            }
        }
        Err(_) => {
            println!("  (resources not supported by this server)");
        }
    }

    // List prompts (if supported by the server)
    println!();
    println!("--- Available Prompts ---");
    match client.list_prompts().await {
        Ok(prompts) => {
            if prompts.is_empty() {
                println!("  (no prompts available)");
            } else {
                for prompt in &prompts {
                    let name = prompt["name"].as_str().unwrap_or("unknown");
                    let description = prompt["description"].as_str().unwrap_or("no description");
                    println!("  {}: {}", name, description);
                }
            }
        }
        Err(_) => {
            println!("  (prompts not supported by this server)");
        }
    }

    println!();
    println!("Client example completed successfully!");

    Ok(())
}

/// Extract text content from a tool result.
fn extract_text_content(result: &Value) -> String {
    result["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .unwrap_or("(no content)")
        .to_string()
}
