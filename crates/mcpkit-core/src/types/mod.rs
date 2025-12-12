//! MCP-specific types for tools, resources, prompts, tasks, and content.
//!
//! This module contains all the domain-specific types defined by the
//! Model Context Protocol specification (version 2025-11-25).
//!
//! # Overview
//!
//! The MCP protocol defines several core capabilities:
//!
//! - **Tools**: Callable functions that servers expose for AI assistants
//! - **Resources**: Data that servers expose (files, database entries, etc.)
//! - **Prompts**: Templated messages with arguments
//! - **Tasks**: Long-running operations with progress tracking
//! - **Sampling**: Requesting LLM completions from the client
//! - **Elicitation**: Requesting structured input from the user
//! - **Content**: Polymorphic content types (text, images, audio, resources)

pub mod completion;
pub mod content;
pub mod elicitation;
pub mod prompt;
pub mod resource;
pub mod sampling;
pub mod task;
pub mod tool;

// Re-export all public types at the module level
pub use completion::*;
pub use content::*;
pub use elicitation::*;
pub use prompt::*;
pub use resource::*;
pub use sampling::*;
pub use task::*;
pub use tool::*;
