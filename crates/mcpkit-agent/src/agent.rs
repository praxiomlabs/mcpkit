//! Agent trait and base implementations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AgentResult;
use crate::tool::ToolSchema;

/// The action an agent decides to take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    /// Use a tool with the given arguments.
    Tool {
        /// The name of the tool to use.
        name: String,
        /// The arguments to pass to the tool.
        arguments: serde_json::Value,
        /// Optional reasoning for this action.
        #[serde(skip_serializing_if = "Option::is_none")]
        thought: Option<String>,
    },
    /// Provide the final answer.
    Finish {
        /// The final answer/output.
        output: String,
        /// Optional reasoning.
        #[serde(skip_serializing_if = "Option::is_none")]
        thought: Option<String>,
    },
}

impl AgentAction {
    /// Create a tool action.
    pub fn tool(
        name: impl Into<String>,
        arguments: serde_json::Value,
        thought: Option<String>,
    ) -> Self {
        Self::Tool {
            name: name.into(),
            arguments,
            thought,
        }
    }

    /// Create a finish action.
    pub fn finish(output: impl Into<String>, thought: Option<String>) -> Self {
        Self::Finish {
            output: output.into(),
            thought,
        }
    }

    /// Check if this is a finish action.
    #[must_use]
    pub fn is_finish(&self) -> bool {
        matches!(self, Self::Finish { .. })
    }

    /// Get the thought/reasoning if present.
    #[must_use]
    pub fn thought(&self) -> Option<&str> {
        match self {
            Self::Tool { thought, .. } | Self::Finish { thought, .. } => thought.as_deref(),
        }
    }
}

/// A single step in the agent's execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    /// The action taken.
    pub action: AgentAction,
    /// The observation/result from the action (for tool calls).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<String>,
}

impl AgentStep {
    /// Create a new step with an action.
    pub fn new(action: AgentAction) -> Self {
        Self {
            action,
            observation: None,
        }
    }

    /// Set the observation.
    #[must_use]
    pub fn with_observation(mut self, observation: impl Into<String>) -> Self {
        self.observation = Some(observation.into());
        self
    }
}

/// Context provided to the agent for decision-making.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// The user's input/query.
    pub input: String,
    /// Previous steps taken in this execution.
    pub steps: Vec<AgentStep>,
    /// Available tools.
    pub tools: Vec<ToolSchema>,
}

impl AgentContext {
    /// Create a new context.
    pub fn new(input: impl Into<String>, tools: Vec<ToolSchema>) -> Self {
        Self {
            input: input.into(),
            steps: Vec::new(),
            tools,
        }
    }

    /// Add a step to the context.
    pub fn add_step(&mut self, step: AgentStep) {
        self.steps.push(step);
    }

    /// Get the last observation if any.
    #[must_use]
    pub fn last_observation(&self) -> Option<&str> {
        self.steps
            .last()
            .and_then(|s| s.observation.as_deref())
    }

    /// Get the number of steps taken.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

/// Trait for agents that can make decisions.
///
/// An agent observes the current context and decides what action to take.
/// The executor handles tool execution and iteration.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_agent::{Agent, AgentContext, AgentAction, AgentResult};
/// use async_trait::async_trait;
///
/// struct SimpleAgent;
///
/// #[async_trait]
/// impl Agent for SimpleAgent {
///     async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction> {
///         // If we have no steps, use a tool
///         if context.steps.is_empty() {
///             return Ok(AgentAction::tool("search", json!({"query": &context.input}), None));
///         }
///
///         // Otherwise, finish with the last observation
///         let answer = context.last_observation().unwrap_or("No result");
///         Ok(AgentAction::finish(answer, None))
///     }
/// }
/// ```
#[async_trait]
pub trait Agent: Send + Sync {
    /// Decide the next action based on the current context.
    async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction>;

    /// Get the agent's name.
    fn name(&self) -> &str {
        "Agent"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_action_tool() {
        let action = AgentAction::tool("search", serde_json::json!({"q": "test"}), Some("I need to search".to_string()));

        assert!(!action.is_finish());
        assert_eq!(action.thought(), Some("I need to search"));
    }

    #[test]
    fn test_agent_action_finish() {
        let action = AgentAction::finish("The answer is 42", None);

        assert!(action.is_finish());
        assert_eq!(action.thought(), None);
    }

    #[test]
    fn test_agent_step() {
        let step = AgentStep::new(AgentAction::tool("echo", serde_json::json!({}), None))
            .with_observation("Hello!");

        assert_eq!(step.observation, Some("Hello!".to_string()));
    }

    #[test]
    fn test_agent_context() {
        let tools = vec![ToolSchema::new("test", "A test tool")];
        let mut context = AgentContext::new("What is 2+2?", tools);

        assert_eq!(context.input, "What is 2+2?");
        assert_eq!(context.step_count(), 0);
        assert!(context.last_observation().is_none());

        context.add_step(
            AgentStep::new(AgentAction::tool("calc", serde_json::json!({}), None))
                .with_observation("4"),
        );

        assert_eq!(context.step_count(), 1);
        assert_eq!(context.last_observation(), Some("4"));
    }
}
