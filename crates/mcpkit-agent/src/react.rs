//! ReAct (Reasoning and Acting) agent implementation.

use async_trait::async_trait;
use std::sync::Arc;

use mcpkit_provider::{CompletionRequest, Message, Provider};

use crate::agent::{Agent, AgentAction, AgentContext};
use crate::error::{AgentError, AgentResult};

/// A ReAct agent that uses chain-of-thought reasoning with tool use.
///
/// The ReAct pattern (Reasoning and Acting) interleaves reasoning traces
/// with actions, allowing the agent to plan, execute, and observe in a loop.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_agent::{ReActAgent, AgentExecutor, ToolRegistry};
/// use mcpkit_provider::openai::OpenAiProvider;
///
/// let provider = OpenAiProvider::new(api_key)?;
/// let agent = ReActAgent::new(provider).model("gpt-4o");
///
/// let mut executor = AgentExecutor::new(agent);
/// executor.register_tool(SearchTool);
///
/// let result = executor.run("What is the capital of France?").await?;
/// println!("{}", result);
/// ```
pub struct ReActAgent<P: Provider> {
    provider: Arc<P>,
    model: Option<String>,
    system_prompt: String,
    temperature: Option<f32>,
}

impl<P: Provider> ReActAgent<P> {
    /// Create a new ReAct agent with the given provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            model: None,
            system_prompt: DEFAULT_REACT_SYSTEM_PROMPT.to_string(),
            temperature: Some(0.0),
        }
    }

    /// Create from an Arc'd provider.
    pub fn from_arc(provider: Arc<P>) -> Self {
        Self {
            provider,
            model: None,
            system_prompt: DEFAULT_REACT_SYSTEM_PROMPT.to_string(),
            temperature: Some(0.0),
        }
    }

    /// Set the model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set a custom system prompt.
    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Set the temperature.
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Build the prompt for the LLM.
    fn build_prompt(&self, context: &AgentContext) -> String {
        let mut prompt = String::new();

        // Add tool descriptions
        prompt.push_str("You have access to the following tools:\n\n");
        for tool in &context.tools {
            prompt.push_str(&format!("**{}**: {}\n", tool.name, tool.description));
            prompt.push_str(&format!(
                "Parameters: {}\n\n",
                serde_json::to_string_pretty(&tool.parameters).unwrap_or_default()
            ));
        }

        // Add the question
        prompt.push_str(&format!("\nQuestion: {}\n\n", context.input));

        // Add previous steps
        for step in &context.steps {
            match &step.action {
                AgentAction::Tool {
                    name,
                    arguments,
                    thought,
                } => {
                    if let Some(t) = thought {
                        prompt.push_str(&format!("Thought: {t}\n"));
                    }
                    prompt.push_str(&format!("Action: {name}\n"));
                    prompt.push_str(&format!(
                        "Action Input: {}\n",
                        serde_json::to_string(arguments).unwrap_or_default()
                    ));
                    if let Some(obs) = &step.observation {
                        prompt.push_str(&format!("Observation: {obs}\n"));
                    }
                }
                AgentAction::Finish { output, thought } => {
                    if let Some(t) = thought {
                        prompt.push_str(&format!("Thought: {t}\n"));
                    }
                    prompt.push_str(&format!("Final Answer: {output}\n"));
                }
            }
        }

        prompt.push_str("\nNow decide your next action. ");
        prompt.push_str(
            "Respond with a Thought, then either an Action/Action Input or a Final Answer.\n",
        );

        prompt
    }

    /// Parse the LLM response into an action.
    fn parse_response(&self, response: &str) -> AgentResult<AgentAction> {
        let response = response.trim();

        // Extract thought
        let thought = extract_field(response, "Thought:");

        // Check for final answer
        if let Some(answer) = extract_field(response, "Final Answer:") {
            return Ok(AgentAction::finish(answer, thought));
        }

        // Extract action and input
        let action_name =
            extract_field(response, "Action:").ok_or_else(|| AgentError::ParseError {
                message: "No Action or Final Answer found in response".to_string(),
            })?;

        let action_input = extract_field(response, "Action Input:").unwrap_or_default();

        // Parse action input as JSON
        let arguments: serde_json::Value = if action_input.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&action_input).unwrap_or_else(|_| {
                // If not valid JSON, wrap as a string
                serde_json::json!({"input": action_input})
            })
        };

        Ok(AgentAction::tool(action_name, arguments, thought))
    }
}

/// Extract a field value from text (e.g., "Thought: some text" -> "some text").
fn extract_field(text: &str, prefix: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

#[async_trait]
impl<P: Provider + 'static> Agent for ReActAgent<P> {
    async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction> {
        let prompt = self.build_prompt(context);

        let mut request = CompletionRequest::new()
            .message(Message::system(&self.system_prompt))
            .message(Message::user(prompt));

        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        if let Some(temp) = self.temperature {
            request = request.temperature(temp);
        }

        let response = self.provider.complete(request).await?;
        let text = response.text().ok_or_else(|| AgentError::ParseError {
            message: "Empty response from LLM".to_string(),
        })?;

        tracing::debug!(response = %text, "ReAct agent response");

        self.parse_response(&text)
    }

    fn name(&self) -> &str {
        "ReActAgent"
    }
}

/// Default system prompt for ReAct agents.
const DEFAULT_REACT_SYSTEM_PROMPT: &str = r"You are a helpful AI assistant that uses a systematic approach to solve problems.

When given a question or task, you should:
1. Think about what you need to do
2. Use available tools to gather information or perform actions
3. Observe the results
4. Repeat until you have enough information to provide a final answer

Always structure your response using this format:

Thought: [Your reasoning about what to do next]
Action: [The tool name to use]
Action Input: [The JSON input for the tool]

Or when you have the final answer:

Thought: [Your final reasoning]
Final Answer: [Your complete answer to the question]

Be precise and thorough. Only provide a Final Answer when you are confident in your response.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_field() {
        let text = "Thought: I need to search\nAction: search\nAction Input: {\"q\": \"test\"}";

        assert_eq!(
            extract_field(text, "Thought:"),
            Some("I need to search".to_string())
        );
        assert_eq!(extract_field(text, "Action:"), Some("search".to_string()));
        assert_eq!(
            extract_field(text, "Action Input:"),
            Some("{\"q\": \"test\"}".to_string())
        );
        assert_eq!(extract_field(text, "Final Answer:"), None);
    }

    #[test]
    fn test_extract_field_with_colon() {
        let text = "Final Answer: The answer is: 42";
        assert_eq!(
            extract_field(text, "Final Answer:"),
            Some("The answer is: 42".to_string())
        );
    }
}
