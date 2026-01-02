//! Agent executor that runs the agent loop with tools.

use std::sync::Arc;

use crate::agent::{Agent, AgentAction, AgentContext, AgentStep};
use crate::error::{AgentError, AgentResult};
use crate::tool::{Tool, ToolOutput, ToolRegistry};

/// Configuration for the agent executor.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum number of iterations before stopping.
    pub max_iterations: usize,
    /// Whether to include observations in the final output.
    pub include_observations: bool,
    /// Whether to log each step.
    pub verbose: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            include_observations: false,
            verbose: false,
        }
    }
}

impl ExecutorConfig {
    /// Create a new config with the given max iterations.
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            ..Default::default()
        }
    }

    /// Enable verbose logging.
    #[must_use]
    pub fn verbose(mut self) -> Self {
        self.verbose = true;
        self
    }

    /// Include observations in output.
    #[must_use]
    pub fn with_observations(mut self) -> Self {
        self.include_observations = true;
        self
    }
}

/// The result of running an agent.
#[derive(Debug, Clone)]
pub struct ExecutorOutput {
    /// The final output from the agent.
    pub output: String,
    /// All steps taken during execution.
    pub steps: Vec<AgentStep>,
    /// The number of iterations performed.
    pub iterations: usize,
}

impl ExecutorOutput {
    /// Get a step-by-step trace of the execution.
    pub fn trace(&self) -> String {
        let mut trace = String::new();
        for (i, step) in self.steps.iter().enumerate() {
            trace.push_str(&format!("\n--- Step {} ---\n", i + 1));

            match &step.action {
                AgentAction::Tool {
                    name,
                    arguments,
                    thought,
                } => {
                    if let Some(t) = thought {
                        trace.push_str(&format!("Thought: {t}\n"));
                    }
                    trace.push_str(&format!("Action: {name}\n"));
                    trace.push_str(&format!(
                        "Input: {}\n",
                        serde_json::to_string_pretty(arguments).unwrap_or_default()
                    ));
                    if let Some(obs) = &step.observation {
                        trace.push_str(&format!("Observation: {obs}\n"));
                    }
                }
                AgentAction::Finish { output, thought } => {
                    if let Some(t) = thought {
                        trace.push_str(&format!("Thought: {t}\n"));
                    }
                    trace.push_str(&format!("Final Answer: {output}\n"));
                }
            }
        }
        trace
    }
}

/// Executor that runs an agent with tools.
///
/// The executor handles the main agent loop:
/// 1. Ask agent for next action
/// 2. If finish, return the output
/// 3. If tool, execute it and get observation
/// 4. Add step to context and repeat
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_agent::{AgentExecutor, ReActAgent, ToolRegistry};
/// use mcpkit_provider::openai::OpenAiProvider;
///
/// let provider = OpenAiProvider::new(api_key)?;
/// let agent = ReActAgent::new(provider);
///
/// let mut executor = AgentExecutor::new(agent);
/// executor.register_tool(MyTool);
///
/// let result = executor.run("What is 2 + 2?").await?;
/// println!("{}", result.output);
/// ```
pub struct AgentExecutor<A: Agent> {
    agent: Arc<A>,
    tools: ToolRegistry,
    config: ExecutorConfig,
}

impl<A: Agent> AgentExecutor<A> {
    /// Create a new executor with the given agent.
    pub fn new(agent: A) -> Self {
        Self {
            agent: Arc::new(agent),
            tools: ToolRegistry::new(),
            config: ExecutorConfig::default(),
        }
    }

    /// Create from an Arc'd agent.
    pub fn from_arc(agent: Arc<A>) -> Self {
        Self {
            agent,
            tools: ToolRegistry::new(),
            config: ExecutorConfig::default(),
        }
    }

    /// Set the configuration.
    #[must_use]
    pub fn config(mut self, config: ExecutorConfig) -> Self {
        self.config = config;
        self
    }

    /// Set max iterations.
    #[must_use]
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.config.max_iterations = max;
        self
    }

    /// Enable verbose mode.
    #[must_use]
    pub fn verbose(mut self) -> Self {
        self.config.verbose = true;
        self
    }

    /// Register a tool.
    pub fn register_tool<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.register(tool);
    }

    /// Register multiple tools.
    pub fn register_tools<T: Tool + 'static>(&mut self, tools: impl IntoIterator<Item = T>) {
        for tool in tools {
            self.tools.register(tool);
        }
    }

    /// Run the agent with the given input.
    pub async fn run(&self, input: impl Into<String>) -> AgentResult<ExecutorOutput> {
        let input = input.into();
        let mut context = AgentContext::new(&input, self.tools.schemas());
        let mut iterations = 0;

        loop {
            iterations += 1;

            if iterations > self.config.max_iterations {
                return Err(AgentError::MaxIterationsExceeded {
                    max_iterations: self.config.max_iterations,
                });
            }

            if self.config.verbose {
                tracing::info!(
                    iteration = iterations,
                    step_count = context.step_count(),
                    "Agent iteration"
                );
            }

            // Get the next action from the agent
            let action = self.agent.decide(&context).await?;

            if self.config.verbose {
                tracing::info!(action = ?action, "Agent action");
            }

            match action {
                AgentAction::Finish { ref output, .. } => {
                    // Add final step
                    let output = output.clone();
                    context.add_step(AgentStep::new(action));

                    return Ok(ExecutorOutput {
                        output,
                        steps: context.steps,
                        iterations,
                    });
                }
                AgentAction::Tool { ref name, ref arguments, .. } => {
                    // Execute the tool
                    let observation = match self.tools.execute(name, arguments.clone()).await {
                        Ok(output) => format_tool_output(&output),
                        Err(e) => format!("Error: {e}"),
                    };

                    if self.config.verbose {
                        tracing::info!(tool = %name, observation = %observation, "Tool result");
                    }

                    // Add step with observation
                    context.add_step(AgentStep::new(action).with_observation(observation));
                }
            }
        }
    }

    /// Run the agent and return just the output string.
    pub async fn run_simple(&self, input: impl Into<String>) -> AgentResult<String> {
        let result = self.run(input).await?;
        Ok(result.output)
    }
}

/// Format a tool output for the observation.
fn format_tool_output(output: &ToolOutput) -> String {
    if output.success {
        match &output.data {
            Some(data) => format!(
                "{}\n\nData: {}",
                output.content,
                serde_json::to_string_pretty(data).unwrap_or_default()
            ),
            None => output.content.clone(),
        }
    } else {
        format!("Error: {}", output.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentAction;
    use crate::tool::{ToolOutput, ToolSchema};
    use async_trait::async_trait;

    struct MockAgent {
        responses: Vec<AgentAction>,
    }

    impl MockAgent {
        fn new(responses: Vec<AgentAction>) -> Self {
            Self { responses }
        }
    }

    #[async_trait]
    impl Agent for MockAgent {
        async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction> {
            let step = context.step_count();
            if step < self.responses.len() {
                Ok(self.responses[step].clone())
            } else {
                Ok(AgentAction::finish("fallback", None))
            }
        }
    }

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn schema(&self) -> ToolSchema {
            ToolSchema::new("echo", "Echo the input")
        }

        async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
            let msg = input
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            Ok(ToolOutput::success(format!("Echo: {msg}")))
        }

        fn name(&self) -> &str {
            "echo"
        }
    }

    #[tokio::test]
    async fn test_executor_simple_finish() {
        let agent = MockAgent::new(vec![AgentAction::finish("Done!", None)]);

        let executor = AgentExecutor::new(agent);
        let result = executor.run("test").await.unwrap();

        assert_eq!(result.output, "Done!");
        assert_eq!(result.iterations, 1);
    }

    #[tokio::test]
    async fn test_executor_with_tool() {
        let agent = MockAgent::new(vec![
            AgentAction::tool("echo", serde_json::json!({"message": "hello"}), None),
            AgentAction::finish("Got the echo", None),
        ]);

        let mut executor = AgentExecutor::new(agent);
        executor.register_tool(EchoTool);

        let result = executor.run("test").await.unwrap();

        assert_eq!(result.output, "Got the echo");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.steps.len(), 2);

        // First step should have observation
        assert_eq!(
            result.steps[0].observation,
            Some("Echo: hello".to_string())
        );
    }

    #[tokio::test]
    async fn test_executor_max_iterations() {
        // Agent that never finishes
        let agent = MockAgent::new(vec![
            AgentAction::tool("echo", serde_json::json!({}), None),
            AgentAction::tool("echo", serde_json::json!({}), None),
            AgentAction::tool("echo", serde_json::json!({}), None),
        ]);

        let mut executor = AgentExecutor::new(agent).max_iterations(2);
        executor.register_tool(EchoTool);

        let result = executor.run("test").await;

        assert!(matches!(
            result,
            Err(AgentError::MaxIterationsExceeded { max_iterations: 2 })
        ));
    }

    #[tokio::test]
    async fn test_executor_tool_not_found() {
        let agent = MockAgent::new(vec![
            AgentAction::tool("nonexistent", serde_json::json!({}), None),
            AgentAction::finish("done", None),
        ]);

        let executor = AgentExecutor::new(agent);
        let result = executor.run("test").await.unwrap();

        // Should continue with error observation
        assert_eq!(result.steps[0].observation.as_deref().unwrap().contains("Error"), true);
    }

    #[test]
    fn test_executor_output_trace() {
        let output = ExecutorOutput {
            output: "Final answer".to_string(),
            steps: vec![
                AgentStep::new(AgentAction::tool(
                    "search",
                    serde_json::json!({"q": "test"}),
                    Some("Need to search".to_string()),
                ))
                .with_observation("Found: result"),
                AgentStep::new(AgentAction::finish("Final answer", Some("I have the answer".to_string()))),
            ],
            iterations: 2,
        };

        let trace = output.trace();
        assert!(trace.contains("Step 1"));
        assert!(trace.contains("Thought: Need to search"));
        assert!(trace.contains("Action: search"));
        assert!(trace.contains("Found: result"));
        assert!(trace.contains("Step 2"));
        assert!(trace.contains("Final Answer: Final answer"));
    }
}
