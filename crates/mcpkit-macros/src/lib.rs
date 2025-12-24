//! Procedural macros for the MCP SDK.
//!
//! This crate provides the unified `#[mcp_server]` macro that simplifies
//! MCP server development.
//!
//! # Overview
//!
//! The macro system provides:
//!
//! - `#[mcp_server]` - Transform an impl block into a full MCP server
//! - `#[tool]` - Mark a method as an MCP tool
//! - `#[resource]` - Mark a method as an MCP resource handler
//! - `#[prompt]` - Mark a method as an MCP prompt handler
//!
//! # Example
//!
//! ```ignore
//! use mcpkit::prelude::*;
//! use mcpkit::transport::stdio::StdioTransport;
//!
//! struct Calculator;
//!
//! #[mcp_server(name = "calculator", version = "1.0.0")]
//! impl Calculator {
//!     /// Add two numbers together
//!     #[tool(description = "Add two numbers")]
//!     async fn add(&self, a: f64, b: f64) -> ToolOutput {
//!         ToolOutput::text((a + b).to_string())
//!     }
//!
//!     /// Multiply two numbers
//!     #[tool(description = "Multiply two numbers")]
//!     async fn multiply(&self, a: f64, b: f64) -> ToolOutput {
//!         ToolOutput::text((a * b).to_string())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), McpError> {
//!     let transport = StdioTransport::new();
//!     let server = ServerBuilder::new(Calculator)
//!         .with_tools(Calculator)
//!         .build();
//!     server.serve(transport).await
//! }
//! ```
//!
//! # Code Reduction
//!
//! This single macro replaces 4 separate macros:
//! - `#[derive(Clone)]` with manual router field
//! - `#[tool_router]`
//! - `#[tool_handler]`
//! - Manual `new()` constructor
//!
//! **Result: Reduced boilerplate code.**

#![deny(missing_docs)]

mod attrs;
mod client;
mod codegen;
mod derive;
mod error;
mod prompt;
mod resource;
mod server;
mod tool;

use proc_macro::TokenStream;

/// The unified MCP server macro.
///
/// This macro transforms an impl block into a full MCP server implementation,
/// automatically generating all the necessary trait implementations and routing.
///
/// # Attributes
///
/// - `name` - Server name (required)
/// - `version` - Server version (required, can use `env!("CARGO_PKG_VERSION")`)
/// - `instructions` - Optional usage instructions sent to clients
/// - `capabilities` - Optional list of capabilities to advertise
/// - `debug_expand` - Set to `true` to print generated code (default: false)
///
/// # Example
///
/// ```ignore
/// #[mcp_server(name = "my-server", version = "1.0.0")]
/// impl MyServer {
///     #[tool(description = "Do something")]
///     async fn my_tool(&self, input: String) -> ToolOutput {
///         ToolOutput::text(format!("Got: {}", input))
///     }
/// }
/// ```
///
/// # Generated Code
///
/// The macro generates:
///
/// 1. `impl ServerHandler` with `server_info()` and `capabilities()`
/// 2. `impl ToolHandler` with `list_tools()` and `call_tool()` (if any `#[tool]` methods)
/// 3. `impl ResourceHandler` (if any `#[resource]` methods)
/// 4. `impl PromptHandler` (if any `#[prompt]` methods)
///
/// To serve the MCP server, use `ServerBuilder` with your preferred transport:
///
/// ```ignore
/// let server = ServerBuilder::new(MyServer).with_tools(MyServer).build();
/// server.serve(StdioTransport::new()).await?;
/// ```
#[proc_macro_attribute]
pub fn mcp_server(attr: TokenStream, item: TokenStream) -> TokenStream {
    server::expand_mcp_server(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Mark a method as an MCP tool.
///
/// This attribute is used inside an `#[mcp_server]` impl block to designate
/// a method as an MCP tool that AI assistants can call.
///
/// # Attributes
///
/// - `description` - Required description of what the tool does
/// - `name` - Override the tool name (defaults to the method name)
///
/// ## Tool Annotations (Hints for AI Assistants)
///
/// These attributes provide hints to AI assistants about the tool's behavior.
/// They appear in the tool's JSON schema as `annotations`:
///
/// - `destructive = true` - The tool may cause irreversible changes (e.g., delete files,
///   drop tables, send emails). AI assistants may ask for confirmation before calling.
///
/// - `idempotent = true` - Calling the tool multiple times with the same arguments
///   produces the same result (safe to retry on failure).
///
/// - `read_only = true` - The tool only reads data and has no side effects.
///   AI assistants may call these tools more freely.
///
/// ```ignore
/// // A destructive tool - deletes data
/// #[tool(description = "Delete a user account", destructive = true)]
/// async fn delete_user(&self, user_id: String) -> ToolOutput { ... }
///
/// // A read-only tool - safe to call repeatedly
/// #[tool(description = "Get user profile", read_only = true)]
/// async fn get_user(&self, user_id: String) -> ToolOutput { ... }
///
/// // An idempotent tool - safe to retry
/// #[tool(description = "Set user email", idempotent = true)]
/// async fn set_email(&self, user_id: String, email: String) -> ToolOutput { ... }
/// ```
///
/// # Parameter Extraction
///
/// Tool parameters are extracted directly from the function signature:
///
/// ```ignore
/// #[tool(description = "Search for items")]
/// async fn search(
///     &self,
///     /// The search query  (becomes JSON Schema description)
///     query: String,
///     /// Maximum results to return
///     #[mcp(default = 10)]
///     limit: usize,
///     /// Optional category filter
///     category: Option<String>,
/// ) -> ToolOutput {
///     // ...
/// }
/// ```
///
/// # Return Types
///
/// Tools can return either `ToolOutput` or `Result<ToolOutput, McpError>`:
///
/// ## Using `ToolOutput` directly
///
/// Use this when you want to handle errors as recoverable user-facing messages:
///
/// ```ignore
/// #[tool(description = "Divide two numbers")]
/// async fn divide(&self, a: f64, b: f64) -> ToolOutput {
///     if b == 0.0 {
///         // User sees this as a tool error they can recover from
///         return ToolOutput::error("Cannot divide by zero");
///     }
///     ToolOutput::text(format!("{}", a / b))
/// }
/// ```
///
/// ## Using `Result<ToolOutput, McpError>`
///
/// Use this for errors that should propagate as JSON-RPC errors (e.g., invalid
/// parameters, resource not found, permission denied):
///
/// ```ignore
/// #[tool(description = "Read a file")]
/// async fn read_file(&self, path: String) -> Result<ToolOutput, McpError> {
///     // Parameter validation - returns JSON-RPC error
///     if path.contains("..") {
///         return Err(McpError::invalid_params("read_file", "Path traversal not allowed"));
///     }
///
///     // Resource access - returns JSON-RPC error
///     let content = std::fs::read_to_string(&path)
///         .map_err(|e| McpError::resource_not_found(&path))?;
///
///     Ok(ToolOutput::text(content))
/// }
/// ```
///
/// ## When to use which
///
/// | Scenario | Return Type | Example |
/// |----------|-------------|---------|
/// | User input can be corrected | `ToolOutput::error()` | "Please provide a valid email" |
/// | Invalid parameters | `Err(McpError::invalid_params())` | Missing required field |
/// | Resource not found | `Err(McpError::resource_not_found())` | File doesn't exist |
/// | Permission denied | `Err(McpError::resource_access_denied())` | No read access |
/// | Internal server error | `Err(McpError::internal())` | Database connection failed |
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::expand_tool(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Mark a method as an MCP resource handler.
///
/// This attribute designates a method that provides access to resources
/// that AI assistants can read.
///
/// # Attributes
///
/// - `uri_pattern` - The URI pattern for this resource (e.g., `"myserver://data/{id}"`)
/// - `name` - Human-readable name for the resource
/// - `description` - Description of the resource
/// - `mime_type` - MIME type of the resource content
///
/// # Example
///
/// ```ignore
/// #[resource(
///     uri_pattern = "config://app/{key}",
///     name = "App Configuration",
///     description = "Application configuration values",
///     mime_type = "application/json"
/// )]
/// async fn get_config(&self, key: String) -> ResourceContents {
///     // ...
/// }
/// ```
#[proc_macro_attribute]
pub fn resource(attr: TokenStream, item: TokenStream) -> TokenStream {
    resource::expand_resource(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Mark a method as an MCP prompt handler.
///
/// This attribute designates a method that provides prompt templates
/// that AI assistants can use.
///
/// # Attributes
///
/// - `description` - Description of what the prompt does
/// - `name` - Override the prompt name (defaults to the method name)
///
/// # Example
///
/// ```ignore
/// #[prompt(description = "Generate a greeting message")]
/// async fn greeting(&self, name: String) -> GetPromptResult {
///     GetPromptResult {
///         description: Some("A friendly greeting".to_string()),
///         messages: vec![
///             PromptMessage::user(format!("Hello, {}!", name))
///         ],
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn prompt(attr: TokenStream, item: TokenStream) -> TokenStream {
    prompt::expand_prompt(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for tool input types.
///
/// This derive macro generates JSON Schema information for complex
/// tool input types.
///
/// # Example
///
/// ```ignore
/// #[derive(ToolInput)]
/// struct SearchInput {
///     /// The search query
///     query: String,
///     /// Maximum results (1-100)
///     #[mcp(default = 10, range(1, 100))]
///     limit: usize,
///     /// Optional filters
///     filters: Option<Vec<String>>,
/// }
/// ```
#[proc_macro_derive(ToolInput, attributes(mcp))]
pub fn derive_tool_input(input: TokenStream) -> TokenStream {
    derive::expand_tool_input(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

// =============================================================================
// Client Macros
// =============================================================================

/// The unified MCP client macro.
///
/// This macro transforms an impl block into a `ClientHandler` implementation,
/// automatically generating all the necessary trait implementations.
///
/// # Example
///
/// ```ignore
/// use mcpkit::prelude::*;
///
/// struct MyClient;
///
/// #[mcp_client]
/// impl MyClient {
///     /// Handle LLM sampling requests from servers.
///     #[sampling]
///     async fn handle_sampling(
///         &self,
///         request: CreateMessageRequest,
///     ) -> Result<CreateMessageResult, McpError> {
///         // Call your LLM here
///         Ok(CreateMessageResult {
///             role: Role::Assistant,
///             content: Content::text("Response"),
///             model: "my-model".to_string(),
///             stop_reason: Some("end_turn".to_string()),
///         })
///     }
///
///     /// Provide filesystem roots to servers.
///     #[roots]
///     fn get_roots(&self) -> Vec<Root> {
///         vec![Root::new("file:///home/user/project").name("Project")]
///     }
/// }
/// ```
///
/// # Handler Methods
///
/// The following attributes mark methods as handlers:
///
/// - `#[sampling]` - Handle `sampling/createMessage` requests
/// - `#[elicitation]` - Handle `elicitation/elicit` requests
/// - `#[roots]` - Handle `roots/list` requests
/// - `#[on_connected]` - Called when connection is established
/// - `#[on_disconnected]` - Called when connection is closed
/// - `#[on_task_progress]` - Handle task progress notifications
/// - `#[on_resource_updated]` - Handle resource update notifications
/// - `#[on_tools_list_changed]` - Handle tools list change notifications
/// - `#[on_resources_list_changed]` - Handle resources list change notifications
/// - `#[on_prompts_list_changed]` - Handle prompts list change notifications
///
/// # Generated Code
///
/// The macro generates:
///
/// 1. `impl ClientHandler` with all handler methods delegating to your implementations
/// 2. A `capabilities()` method returning the appropriate `ClientCapabilities`
#[proc_macro_attribute]
pub fn mcp_client(attr: TokenStream, item: TokenStream) -> TokenStream {
    client::expand_mcp_client(attr.into(), item.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Mark a method as a sampling handler.
///
/// This handler is called when servers request LLM completions.
/// The method should accept a `CreateMessageRequest` and return
/// `Result<CreateMessageResult, McpError>` or `CreateMessageResult`.
///
/// # Example
///
/// ```ignore
/// #[sampling]
/// async fn handle_sampling(
///     &self,
///     request: CreateMessageRequest,
/// ) -> Result<CreateMessageResult, McpError> {
///     // Process the request and generate a response
/// }
/// ```
#[proc_macro_attribute]
pub fn sampling(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as an elicitation handler.
///
/// This handler is called when servers request user input.
/// The method should accept an `ElicitRequest` and return
/// `Result<ElicitResult, McpError>` or `ElicitResult`.
///
/// # Example
///
/// ```ignore
/// #[elicitation]
/// async fn handle_elicitation(
///     &self,
///     request: ElicitRequest,
/// ) -> Result<ElicitResult, McpError> {
///     // Present the request to the user and return their response
/// }
/// ```
#[proc_macro_attribute]
pub fn elicitation(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a roots handler.
///
/// This handler is called when servers request the list of filesystem roots.
/// The method should return `Vec<Root>` or `Result<Vec<Root>, McpError>`.
///
/// # Example
///
/// ```ignore
/// #[roots]
/// fn get_roots(&self) -> Vec<Root> {
///     vec![
///         Root::new("file:///home/user/project").name("Project"),
///         Root::new("file:///home/user/docs").name("Documents"),
///     ]
/// }
/// ```
#[proc_macro_attribute]
pub fn roots(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as the connection established handler.
///
/// This handler is called when the client connects to a server.
///
/// # Example
///
/// ```ignore
/// #[on_connected]
/// async fn handle_connected(&self) {
///     println!("Connected to server!");
/// }
/// ```
#[proc_macro_attribute]
pub fn on_connected(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as the disconnection handler.
///
/// This handler is called when the client disconnects from a server.
///
/// # Example
///
/// ```ignore
/// #[on_disconnected]
/// async fn handle_disconnected(&self) {
///     println!("Disconnected from server");
/// }
/// ```
#[proc_macro_attribute]
pub fn on_disconnected(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a task progress notification handler.
///
/// This handler is called when the server reports progress on a task.
/// The method should accept a `TaskId` and `TaskProgress`.
///
/// # Example
///
/// ```ignore
/// #[on_task_progress]
/// async fn handle_progress(&self, task_id: TaskId, progress: TaskProgress) {
///     println!("Task {} is {}% complete", task_id, progress.progress * 100.0);
/// }
/// ```
#[proc_macro_attribute]
pub fn on_task_progress(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a resource update notification handler.
///
/// This handler is called when a subscribed resource is updated.
/// The method should accept a `String` (the resource URI).
///
/// # Example
///
/// ```ignore
/// #[on_resource_updated]
/// async fn handle_resource_updated(&self, uri: String) {
///     println!("Resource updated: {}", uri);
///     // Invalidate cache, refresh data, etc.
/// }
/// ```
#[proc_macro_attribute]
pub fn on_resource_updated(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a tools list change notification handler.
///
/// This handler is called when the server's tool list changes.
///
/// # Example
///
/// ```ignore
/// #[on_tools_list_changed]
/// async fn handle_tools_changed(&self) {
///     println!("Tools list changed - refreshing cache");
/// }
/// ```
#[proc_macro_attribute]
pub fn on_tools_list_changed(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a resources list change notification handler.
///
/// This handler is called when the server's resource list changes.
///
/// # Example
///
/// ```ignore
/// #[on_resources_list_changed]
/// async fn handle_resources_changed(&self) {
///     println!("Resources list changed - refreshing cache");
/// }
/// ```
#[proc_macro_attribute]
pub fn on_resources_list_changed(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}

/// Mark a method as a prompts list change notification handler.
///
/// This handler is called when the server's prompt list changes.
///
/// # Example
///
/// ```ignore
/// #[on_prompts_list_changed]
/// async fn handle_prompts_changed(&self) {
///     println!("Prompts list changed - refreshing cache");
/// }
/// ```
#[proc_macro_attribute]
pub fn on_prompts_list_changed(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // This is a marker attribute - just pass through the item unchanged
    item
}
