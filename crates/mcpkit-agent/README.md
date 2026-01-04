# mcpkit-agent

Agent patterns and tool execution for LLM-powered autonomous agents.

## Overview

`mcpkit-agent` provides the building blocks for creating autonomous agents that can reason, use tools, and accomplish complex tasks. Inspired by [ReAct](https://www.promptingguide.ai/techniques/react) and other modern agent patterns.

## Core Concepts

- **`Agent`**: Trait for decision-making agents
- **`Tool`**: Trait for executable tools/capabilities
- **`AgentExecutor`**: Runs the agent loop with tools
- **`ReActAgent`**: LLM-based agent using Reasoning + Acting

## Quick Start

```rust
use mcpkit_agent::{ReActAgent, AgentExecutor, Tool, ToolSchema, ToolOutput, AgentResult};
use mcpkit_provider::openai::OpenAiProvider;
use async_trait::async_trait;

// Define a simple tool
struct SearchTool;

#[async_trait]
impl Tool for SearchTool {
    fn schema(&self) -> ToolSchema {
        ToolSchema::new("search", "Search the web for information")
            .add_parameter("query", "string", "The search query", true)
    }

    async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
        let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
        // Perform actual search...
        Ok(ToolOutput::success(format!("Results for: {query}")))
    }

    fn name(&self) -> &str {
        "search"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAiProvider::new(std::env::var("OPENAI_API_KEY")?)?;

    // Create a ReAct agent
    let agent = ReActAgent::new(provider).model("gpt-4o");

    // Set up executor with tools
    let mut executor = AgentExecutor::new(agent);
    executor.register_tool(SearchTool);

    // Run the agent
    let result = executor.run("What is the population of Tokyo?").await?;
    println!("{}", result.output);

    // Get execution trace
    println!("\n{}", result.trace());

    Ok(())
}
```

## The ReAct Pattern

ReAct (Reasoning + Acting) is an agent pattern where the LLM explicitly reasons about what to do before taking actions:

```
Thought: I need to search for Tokyo's population
Action: search
Action Input: {"query": "Tokyo population 2024"}
Observation: Tokyo has a population of approximately 14 million...
Thought: I now have the answer
Final Answer: Tokyo has a population of approximately 14 million people.
```

```rust
use mcpkit_agent::{ReActAgent, AgentExecutor};

let agent = ReActAgent::new(provider)
    .model("gpt-4o")
    .max_iterations(10)  // Limit reasoning loops
    .system_prompt("You are a helpful research assistant.");

let mut executor = AgentExecutor::new(agent);
```

## Defining Tools

Tools are capabilities the agent can use:

```rust
use mcpkit_agent::{Tool, ToolSchema, ToolOutput, AgentResult};
use async_trait::async_trait;

struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str {
        "calculator"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema::new("calculator", "Perform mathematical calculations")
            .add_parameter("expression", "string", "Math expression to evaluate", true)
    }

    async fn execute(&self, input: serde_json::Value) -> AgentResult<ToolOutput> {
        let expr = input.get("expression")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Evaluate expression...
        Ok(ToolOutput::success("42"))
    }
}
```

### Function Tools

For simple tools, use `FnTool`:

```rust
use mcpkit_agent::{FnTool, ToolSchema};

let weather_tool = FnTool::new(
    ToolSchema::new("weather", "Get current weather")
        .add_parameter("city", "string", "City name", true),
    |input| async move {
        let city = input.get("city").and_then(|v| v.as_str()).unwrap_or("");
        Ok(ToolOutput::success(format!("Sunny in {city}")))
    }
);
```

## Custom Agents

Implement the `Agent` trait for custom decision logic:

```rust
use mcpkit_agent::{Agent, AgentContext, AgentAction, AgentResult};
use async_trait::async_trait;

struct MyAgent;

#[async_trait]
impl Agent for MyAgent {
    async fn decide(&self, context: &AgentContext) -> AgentResult<AgentAction> {
        // Custom decision logic
        if context.steps.is_empty() {
            // First step: use a tool
            Ok(AgentAction::tool("search", json!({"query": &context.input}), None))
        } else {
            // Got result: finish
            let answer = context.last_observation().unwrap_or("No result");
            Ok(AgentAction::finish(answer, None))
        }
    }
}
```

## Executor Configuration

Configure the agent executor:

```rust
use mcpkit_agent::{AgentExecutor, ExecutorConfig};

let config = ExecutorConfig::new()
    .max_iterations(20)       // Maximum reasoning steps
    .timeout(Duration::from_secs(120));  // Overall timeout

let mut executor = AgentExecutor::new(agent)
    .config(config);
```

## Execution Trace

Get detailed execution history:

```rust
let result = executor.run("Calculate 15% tip on $47.50").await?;

// Access individual steps
for step in &result.steps {
    println!("Action: {:?}", step.action);
    println!("Observation: {}", step.observation);
}

// Or get formatted trace
println!("{}", result.trace());
```

## Built-in Patterns

| Pattern | Description |
|---------|-------------|
| `ReActAgent` | Reasoning + Acting with chain-of-thought |

Future patterns planned:
- Plan-and-Execute
- Multi-agent collaboration
- Hierarchical agents

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
