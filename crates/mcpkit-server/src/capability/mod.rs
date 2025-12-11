//! Capability handlers for MCP servers.
//!
//! This module provides default implementations and utilities for
//! handling various MCP capabilities.
//!
//! # Capability Modules
//!
//! - [`tools`]: Tool discovery and execution
//! - [`resources`]: Resource discovery and reading
//! - [`prompts`]: Prompt discovery and rendering
//! - [`tasks`]: Long-running task management
//! - [`sampling`]: Sampling/LLM inference requests
//! - [`elicitation`]: User input elicitation
//! - [`completions`]: Argument completion support

pub mod completions;
pub mod elicitation;
pub mod prompts;
pub mod resources;
pub mod sampling;
pub mod tasks;
pub mod tools;

// Re-export commonly used types
pub use completions::CompletionService;
pub use elicitation::ElicitationService;
pub use prompts::PromptService;
pub use resources::ResourceService;
pub use sampling::SamplingService;
pub use tasks::TaskService;
pub use tools::ToolService;
