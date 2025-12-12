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
//! use mcp::prelude::*;
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
//!     Calculator.serve_stdio().await
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
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::unwrap_used)]

mod attrs;
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
/// 5. A `serve_stdio()` convenience method
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
/// - `destructive` - Hint that the tool may cause destructive changes (default: false)
/// - `idempotent` - Hint that calling the tool multiple times has same effect (default: false)
/// - `read_only` - Hint that the tool only reads data (default: false)
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
/// Tools can return:
/// - `ToolOutput` - The standard output type
/// - `Result<ToolOutput, McpError>` - For fallible operations
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
