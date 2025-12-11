//! Full MCP server example.
//!
//! This example demonstrates a complete MCP server with tools, resources, and prompts
//! using the unified `#[mcp_server]` macro.
//!
//! ## Running
//!
//! ```bash
//! cargo run -p full-server
//! ```
//!
//! ## What This Demonstrates
//!
//! - Tools: Calculator functions and note management
//! - Resources: Static configuration and dynamic data
//! - Prompts: Code review and summarization prompts
//! - All generated from a single `#[mcp_server]` macro

use mcpkit::prelude::*;
use mcpkit_core::types::{GetPromptResult, PromptMessage, ResourceContents};
use mcpkit_server::{Context, PromptHandler, ResourceHandler, ServerHandler, ToolHandler};
use std::collections::HashMap;
use std::sync::RwLock;

/// A full-featured MCP server demonstrating tools, resources, and prompts.
struct FullServer {
    /// In-memory notes storage
    notes: RwLock<HashMap<String, String>>,
    /// Configuration data
    config: HashMap<String, String>,
}

impl FullServer {
    fn new() -> Self {
        let mut config = HashMap::new();
        config.insert("app_name".to_string(), "FullServer".to_string());
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("debug".to_string(), "true".to_string());

        Self {
            notes: RwLock::new(HashMap::new()),
            config,
        }
    }
}

#[mcp_server(
    name = "full-server",
    version = "1.0.0",
    instructions = "A full-featured MCP server with tools, resources, and prompts."
)]
impl FullServer {
    // ========== TOOLS ==========

    /// Add two numbers together.
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a + b))
    }

    /// Multiply two numbers.
    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
        ToolOutput::text(format!("{}", a * b))
    }

    /// Save a note with a given key.
    #[tool(description = "Save a note with a given key")]
    async fn save_note(&self, key: String, content: String) -> ToolOutput {
        let mut notes = self.notes.write().unwrap();
        notes.insert(key.clone(), content);
        ToolOutput::text(format!("Note '{}' saved", key))
    }

    /// Get a note by key.
    #[tool(description = "Get a note by its key")]
    async fn get_note(&self, key: String) -> ToolOutput {
        let notes = self.notes.read().unwrap();
        match notes.get(&key) {
            Some(content) => ToolOutput::text(content.clone()),
            None => ToolOutput::error(format!("Note '{}' not found", key)),
        }
    }

    /// List all saved notes.
    #[tool(description = "List all saved note keys", read_only = true)]
    async fn list_notes(&self) -> ToolOutput {
        let notes = self.notes.read().unwrap();
        let keys: Vec<&String> = notes.keys().collect();
        ToolOutput::text(format!("{:?}", keys))
    }

    // ========== RESOURCES ==========

    /// Get the application configuration.
    #[resource(
        uri_pattern = "config://app",
        name = "App Configuration",
        description = "Application configuration settings",
        mime_type = "application/json"
    )]
    async fn get_config(&self, _uri: &str) -> ResourceContents {
        let json = serde_json::to_string_pretty(&self.config).unwrap_or_default();
        ResourceContents::text("config://app", json)
    }

    /// Get a specific configuration value.
    #[resource(
        uri_pattern = "config://app/{key}",
        name = "Config Value",
        description = "Get a specific configuration value",
        mime_type = "text/plain"
    )]
    async fn get_config_value(&self, uri: &str) -> ResourceContents {
        // Extract key from URI like "config://app/debug"
        let key = uri.strip_prefix("config://app/").unwrap_or("");
        let value = self
            .config
            .get(key)
            .map(|s| s.as_str())
            .unwrap_or("not found");
        ResourceContents::text(uri, value)
    }

    // ========== PROMPTS ==========

    /// Generate a code review prompt.
    #[prompt(description = "Generate a code review prompt for the given code")]
    async fn code_review(&self, code: String, language: Option<String>) -> GetPromptResult {
        let lang = language.unwrap_or_else(|| "unknown".to_string());
        GetPromptResult {
            description: Some("Code review prompt".to_string()),
            messages: vec![PromptMessage::user(format!(
                "Please review the following {} code for:\n\
                 - Bugs and potential issues\n\
                 - Performance improvements\n\
                 - Code style and best practices\n\
                 - Security concerns\n\n\
                 ```{}\n{}\n```",
                lang, lang, code
            ))],
        }
    }

    /// Generate a summarization prompt.
    #[prompt(description = "Generate a prompt to summarize text")]
    async fn summarize(&self, text: String, max_words: Option<u32>) -> GetPromptResult {
        let length_instruction = max_words
            .map(|w| format!("Keep the summary under {} words.", w))
            .unwrap_or_else(|| "Keep the summary concise.".to_string());

        GetPromptResult {
            description: Some("Summarization prompt".to_string()),
            messages: vec![PromptMessage::user(format!(
                "Please summarize the following text. {}\n\n{}",
                length_instruction, text
            ))],
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Initialize the server
    let server = FullServer::new();

    println!("Full MCP server initialized!");
    println!();

    // Demonstrate server info
    let info = <FullServer as ServerHandler>::server_info(&server);
    println!("Server: {} v{}", info.name, info.version);

    let caps = <FullServer as ServerHandler>::capabilities(&server);
    println!("Capabilities:");
    println!("  - Tools: {}", caps.has_tools());
    println!("  - Resources: {}", caps.has_resources());
    println!("  - Prompts: {}", caps.has_prompts());

    // Set up test context
    use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
    use mcpkit_core::protocol::RequestId;
    use mcpkit_core::protocol_version::ProtocolVersion;
    use mcpkit_server::NoOpPeer;

    let request_id = RequestId::Number(1);
    let client_caps = ClientCapabilities::default();
    let server_caps = ServerCapabilities::default();
    let peer = NoOpPeer;

    let ctx = Context::new(&request_id, None, &client_caps, &server_caps, ProtocolVersion::LATEST, &peer);

    // ========== TEST TOOLS ==========
    println!("\n=== Tools ===");
    let tools = <FullServer as ToolHandler>::list_tools(&server, &ctx).await?;
    println!("Available tools:");
    for tool in &tools {
        println!(
            "  - {} : {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        );
    }

    // Test add tool
    println!("\nCalling add(2, 3)...");
    let args = serde_json::json!({"a": 2.0, "b": 3.0});
    let result = <FullServer as ToolHandler>::call_tool(&server, "add", args, &ctx).await?;
    print_tool_result(&result);

    // Test save_note tool
    println!("\nCalling save_note('greeting', 'Hello, World!')...");
    let args = serde_json::json!({"key": "greeting", "content": "Hello, World!"});
    let result = <FullServer as ToolHandler>::call_tool(&server, "save_note", args, &ctx).await?;
    print_tool_result(&result);

    // Test get_note tool
    println!("\nCalling get_note('greeting')...");
    let args = serde_json::json!({"key": "greeting"});
    let result = <FullServer as ToolHandler>::call_tool(&server, "get_note", args, &ctx).await?;
    print_tool_result(&result);

    // ========== TEST RESOURCES ==========
    println!("\n=== Resources ===");
    let resources = <FullServer as ResourceHandler>::list_resources(&server, &ctx).await?;
    println!("Available resources:");
    for resource in &resources {
        println!("  - {} ({})", resource.uri, resource.name);
    }

    // Test reading config resource
    println!("\nReading config://app...");
    let contents =
        <FullServer as ResourceHandler>::read_resource(&server, "config://app", &ctx).await?;
    for content in &contents {
        if let Some(text) = &content.text {
            println!("Content:\n{}", text);
        }
    }

    // Test reading config value
    println!("\nReading config://app/debug...");
    let contents =
        <FullServer as ResourceHandler>::read_resource(&server, "config://app/debug", &ctx).await?;
    for content in &contents {
        if let Some(text) = &content.text {
            println!("Content: {}", text);
        }
    }

    // ========== TEST PROMPTS ==========
    println!("\n=== Prompts ===");
    let prompts = <FullServer as PromptHandler>::list_prompts(&server, &ctx).await?;
    println!("Available prompts:");
    for prompt in &prompts {
        let args_str = prompt
            .arguments
            .as_ref()
            .map(|args| {
                args.iter()
                    .map(|a| {
                        let req = if a.required.unwrap_or(false) { "*" } else { "" };
                        format!("{}{}", a.name, req)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        println!(
            "  - {}({}) : {}",
            prompt.name,
            args_str,
            prompt.description.as_deref().unwrap_or("")
        );
    }

    // Test code_review prompt
    println!("\nGetting code_review prompt...");
    let args =
        serde_json::json!({"code": "fn main() { println!(\"Hello\"); }", "language": "rust"});
    let args_map: serde_json::Map<String, serde_json::Value> = args.as_object().unwrap().clone();
    let result =
        <FullServer as PromptHandler>::get_prompt(&server, "code_review", Some(args_map), &ctx)
            .await?;
    println!("Description: {:?}", result.description);
    println!("Messages:");
    for msg in &result.messages {
        let role_str = match msg.role {
            mcpkit_core::types::Role::User => "user",
            mcpkit_core::types::Role::Assistant => "assistant",
        };
        // Get first 80 chars of content for display
        let content_preview: String = format!("{:?}", msg.content).chars().take(80).collect();
        println!("  [{}]: {}...", role_str, content_preview);
    }

    // Test summarize prompt
    println!("\nGetting summarize prompt...");
    let args = serde_json::json!({"text": "The quick brown fox jumps over the lazy dog.", "max_words": 10});
    let args_map: serde_json::Map<String, serde_json::Value> = args.as_object().unwrap().clone();
    let result =
        <FullServer as PromptHandler>::get_prompt(&server, "summarize", Some(args_map), &ctx)
            .await?;
    println!("Description: {:?}", result.description);
    for msg in &result.messages {
        println!("  [user]: {:?}", msg.content);
    }

    println!("\n✓ All tests passed!");

    Ok(())
}

fn print_tool_result(result: &ToolOutput) {
    match result {
        ToolOutput::Success(r) => {
            for content in &r.content {
                if let Content::Text(tc) = content {
                    println!("Result: {}", tc.text);
                }
            }
        }
        ToolOutput::RecoverableError { message, .. } => {
            println!("Error: {}", message);
        }
    }
}
