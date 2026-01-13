# mcpkit-chain

Composable chain primitives for LLM orchestration workflows.

## Overview

`mcpkit-chain` provides a set of composable building blocks for creating LLM pipelines. Inspired by [LangChain Expression Language (LCEL)](https://python.langchain.com/docs/concepts/lcel/), it enables declarative composition of LLM operations.

## Core Concepts

- **`Runnable`**: The base trait for all composable operations
- **`ChainValue`**: Dynamic value type for passing data between steps
- **Sequential**: Chain runnables with `.then()` for step-by-step execution
- **Parallel**: Run multiple runnables concurrently with `RunnableParallel`
- **Branching**: Conditional execution with `RunnableBranch`

## Quick Start

```rust
use mcpkit_chain::{Runnable, ChainValue, PromptRunnable, LlmRunnable};
use mcpkit_provider::openai::OpenAiProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OpenAiProvider::new(std::env::var("OPENAI_API_KEY")?)?;

    // Create a simple chain: prompt -> LLM
    let chain = PromptRunnable::new("Summarize: {input}")
        .then(LlmRunnable::new(provider).model("gpt-4o"));

    let result = chain.invoke(ChainValue::from("Some long text...")).await?;
    println!("{}", result);
    Ok(())
}
```

## Parallel Execution

Run multiple operations concurrently:

```rust
use mcpkit_chain::{RunnableParallel, PromptRunnable, LlmRunnable, ChainValue};

// Generate title and summary in parallel
let mut parallel = RunnableParallel::new(vec![]);
parallel.add("title", PromptRunnable::new("Generate a title for: {input}")
    .then(llm.clone()));
parallel.add("summary", PromptRunnable::new("Summarize: {input}")
    .then(llm.clone()));

let result = parallel.invoke(ChainValue::from("Article content...")).await?;
let title = result.get("title").unwrap();
let summary = result.get("summary").unwrap();
```

## Conditional Branching

Route inputs based on conditions:

```rust
use mcpkit_chain::{RunnableBranch, RunnableConst, ChainValue};

let router = RunnableBranch::new()
    .when(
        |v| v.get("type").and_then(|t| t.as_str()) == Some("question"),
        question_handler,
    )
    .when(
        |v| v.get("type").and_then(|t| t.as_str()) == Some("command"),
        command_handler,
    )
    .otherwise(default_handler);
```

## Built-in Runnables

| Runnable | Description |
|----------|-------------|
| `RunnableFn` | Create from async function |
| `RunnableSequence` | Sequential execution |
| `RunnableParallel` | Concurrent execution |
| `RunnableBranch` | Conditional routing |
| `RunnablePassthrough` | Pass input unchanged |
| `RunnableConst` | Return constant value |
| `RunnablePick` | Extract field from object |
| `RunnableAssign` | Add fields to object |
| `RunnableRetry` | Retry on failure |
| `PromptRunnable` | Format prompt template |
| `LlmRunnable` | Call LLM provider |
| `JsonParseRunnable` | Parse JSON from string |

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
