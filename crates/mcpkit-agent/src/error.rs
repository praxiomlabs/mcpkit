//! Error types for agent operations.

use thiserror::Error;

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Provider error during LLM operation.
    #[error("provider error: {0}")]
    Provider(#[from] mcpkit_provider::ProviderError),

    /// Chain error during execution.
    #[error("chain error: {0}")]
    Chain(#[from] mcpkit_chain::ChainError),

    /// Memory error.
    #[error("memory error: {0}")]
    Memory(#[from] mcpkit_memory::MemoryError),

    /// Tool not found.
    #[error("tool not found: {name}")]
    ToolNotFound {
        /// The tool name that was not found.
        name: String,
    },

    /// Tool execution failed.
    #[error("tool '{name}' failed: {message}")]
    ToolFailed {
        /// The tool name.
        name: String,
        /// Error message.
        message: String,
    },

    /// Maximum iterations exceeded.
    #[error("maximum iterations exceeded ({max_iterations})")]
    MaxIterationsExceeded {
        /// The maximum number of iterations.
        max_iterations: usize,
    },

    /// Agent was stopped.
    #[error("agent stopped: {reason}")]
    Stopped {
        /// The reason for stopping.
        reason: String,
    },

    /// Parse error when extracting agent response.
    #[error("failed to parse agent response: {message}")]
    ParseError {
        /// Description of the parse error.
        message: String,
    },

    /// Invalid configuration.
    #[error("invalid configuration: {message}")]
    InvalidConfig {
        /// Description of the configuration error.
        message: String,
    },

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Custom error.
    #[error("{0}")]
    Custom(String),
}

impl AgentError {
    /// Create a custom error.
    pub fn custom(msg: impl Into<String>) -> Self {
        Self::Custom(msg.into())
    }

    /// Create a tool not found error.
    pub fn tool_not_found(name: impl Into<String>) -> Self {
        Self::ToolNotFound { name: name.into() }
    }

    /// Create a tool failed error.
    pub fn tool_failed(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ToolFailed {
            name: name.into(),
            message: message.into(),
        }
    }
}

/// Result type for agent operations.
pub type AgentResult<T> = Result<T, AgentError>;
