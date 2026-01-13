//! Agent patterns and tool execution for LLM-powered autonomous agents.
//!
//! `mcpkit-agent` provides the building blocks for creating autonomous agents
//! that can reason, use tools, and accomplish complex tasks. Inspired by
//! [ReAct](https://www.promptingguide.ai/techniques/react) and other modern
//! agent patterns.
//!
//! # Core Concepts
//!
//! - **`Agent`**: Trait for decision-making agents
//! - **`Tool`**: Trait for executable tools/capabilities
//! - **`AgentExecutor`**: Runs the agent loop with tools
//! - **`ReActAgent`**: LLM-based agent using Reasoning + Acting
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use mcpkit_agent::{ReActAgent, AgentExecutor, Tool, ToolSchema, ToolOutput, AgentResult};
//! use mcpkit_provider::openai::OpenAiProvider;
//! use async_trait::async_trait;
//!
//! // Define a simple tool
//! struct SearchTool;
//!
//! #[async_trait]
//! impl Tool for SearchTool {
//!     fn schema(&self) -> ToolSchema {
//!         ToolSchema::new("search", "Search the web for information")
//!             .add_parameter("query", "string", "The search query", true)
//!     }
//!
//!     async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
//!         let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
//!         // Perform actual search...
//!         Ok(ToolOutput::success(format!("Results for: {query}")))
//!     }
//!
//!     fn name(&self) -> &str {
//!         "search"
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let provider = OpenAiProvider::new(std::env::var("OPENAI_API_KEY")?)?;
//!
//!     // Create a ReAct agent
//!     let agent = ReActAgent::new(provider).model("gpt-4o");
//!
//!     // Set up executor with tools
//!     let mut executor = AgentExecutor::new(agent);
//!     executor.register_tool(SearchTool);
//!
//!     // Run the agent
//!     let result = executor.run("What is the population of Tokyo?").await?;
//!     println!("{}", result.output);
//!
//!     // Get execution trace
//!     println!("\n{}", result.trace());
//!
//!     Ok(())
//! }
//! ```
//!
//! # Custom Agents
//!
//! Implement the `Agent` trait for custom decision logic:
//!
//! ```rust,ignore
//! use mcpkit_agent::{Agent, AgentContext, AgentAction, AgentResult};
//! use async_trait::async_trait;
//!
//! struct MyAgent;
//!
//! #[async_trait]
//! impl Agent for MyAgent {
//!     async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction> {
//!         // Custom decision logic
//!         if context.steps.is_empty() {
//!             // First step: use a tool
//!             Ok(AgentAction::tool("search", json!({"query": &context.input}), None))
//!         } else {
//!             // Got result: finish
//!             let answer = context.last_observation().unwrap_or("No result");
//!             Ok(AgentAction::finish(answer, None))
//!         }
//!     }
//! }
//! ```
//!
//! # Tool Registry
//!
//! Register multiple tools with the executor:
//!
//! ```rust,ignore
//! use mcpkit_agent::{AgentExecutor, ToolRegistry};
//!
//! let mut executor = AgentExecutor::new(agent);
//! executor.register_tool(SearchTool);
//! executor.register_tool(CalculatorTool);
//! executor.register_tool(WeatherTool);
//! ```
//!
//! # Built-in Patterns
//!
//! | Pattern | Description |
//! |---------|-------------|
//! | `ReActAgent` | Reasoning + Acting with chain-of-thought |
//!
//! Future patterns planned:
//! - Plan-and-Execute
//! - Multi-agent collaboration
//! - Hierarchical agents

#![warn(missing_docs)]

mod agent;
mod error;
mod executor;
mod react;
mod tool;

// Re-exports
pub use agent::{Agent, AgentAction, AgentContext, AgentStep};
pub use error::{AgentError, AgentResult};
pub use executor::{AgentExecutor, ExecutorConfig, ExecutorOutput};
pub use react::ReActAgent;
pub use tool::{FnTool, Tool, ToolOutput, ToolRegistry, ToolSchema};
