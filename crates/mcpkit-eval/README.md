# mcpkit-eval

LLM evaluation and testing framework for mcpkit-forge.

## Overview

`mcpkit-eval` provides tools for evaluating LLM outputs and RAG pipelines. It supports both traditional metrics (exact match, contains) and LLM-as-judge metrics (faithfulness, answer relevancy, context precision).

## Features

- **Test cases**: Structured test data with inputs, expected outputs, and contexts
- **Metrics**: Pluggable evaluation metrics (traditional and LLM-based)
- **Runner**: Batch evaluation with aggregated reporting
- **Datasets**: Organize and persist test cases

## Quick Start

```rust
use mcpkit_eval::{
    EvalRunner, ExactMatchMetric, ContainsMetric,
    TestCase, TestDataset, RunnerConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create test cases
    let tests = vec![
        TestCase::new("What is 2+2?")
            .with_expected_output("4")
            .with_actual_output("The answer is 4"),
        TestCase::new("What is the capital of France?")
            .with_expected_output("Paris")
            .with_actual_output("Paris is the capital"),
    ];

    // Set up runner with metrics
    let runner = EvalRunner::new("QA Evaluation")
        .with_metric(ExactMatchMetric::new())
        .with_metric(ContainsMetric::new())
        .config(RunnerConfig::new().pass_threshold(0.5));

    // Run evaluation
    let report = runner.run(&tests).await?;
    println!("{}", report.summary());

    Ok(())
}
```

## Test Cases

Structure test data with inputs, expected outputs, and optional context:

```rust
use mcpkit_eval::TestCase;

let test = TestCase::new("What is Rust?")
    .with_expected_output("A systems programming language")
    .with_actual_output("Rust is a systems programming language")
    .with_context("Rust is a systems programming language focused on safety.")
    .with_metadata("category", "programming");
```

## Available Metrics

### Traditional Metrics

| Metric | Description |
|--------|-------------|
| `ExactMatchMetric` | Exact string match (configurable case/whitespace) |
| `ContainsMetric` | Checks if output contains expected text |
| `RegexMatchMetric` | Matches output against a regex pattern |

```rust
use mcpkit_eval::{ExactMatchMetric, ContainsMetric, RegexMatchMetric};

// Case-insensitive exact match
let metric = ExactMatchMetric::new().case_insensitive(true);

// Check if output contains expected
let metric = ContainsMetric::new();

// Regex pattern match
let metric = RegexMatchMetric::new(r"\d{4}-\d{2}-\d{2}");  // Date pattern
```

### LLM-as-Judge Metrics

For RAG pipelines, use LLM-based evaluation:

| Metric | Description |
|--------|-------------|
| `FaithfulnessMetric` | Is the answer faithful to the context? |
| `AnswerRelevancyMetric` | Is the answer relevant to the question? |
| `ContextPrecisionMetric` | Are the retrieved contexts relevant? |

```rust
use mcpkit_eval::{FaithfulnessMetric, AnswerRelevancyMetric, ContextPrecisionMetric};
use mcpkit_provider::openai::OpenAiProvider;

let provider = OpenAiProvider::new(api_key)?;

let runner = EvalRunner::new("RAG Evaluation")
    .with_metric(FaithfulnessMetric::new(provider.clone()))
    .with_metric(AnswerRelevancyMetric::new(provider.clone()))
    .with_metric(ContextPrecisionMetric::new(provider));

let test = TestCase::new("What is Rust?")
    .with_context("Rust is a systems programming language.")
    .with_actual_output("Rust is a programming language for systems.");

let report = runner.run(&[test]).await?;
```

## Test Datasets

Organize test cases into datasets for reuse:

```rust
use mcpkit_eval::{TestCase, TestDataset};

let dataset = TestDataset::new("QA Benchmark")
    .description("Common question-answering tests")
    .add_case(TestCase::new("Q1").with_expected_output("A1"))
    .add_case(TestCase::new("Q2").with_expected_output("A2"));

// Datasets can be saved/loaded from JSON
dataset.to_json_file("qa_benchmark.json").await?;
let loaded = TestDataset::from_json_file("qa_benchmark.json").await?;
```

## Runner Configuration

Configure evaluation behavior:

```rust
use mcpkit_eval::{EvalRunner, RunnerConfig};

let runner = EvalRunner::new("My Evaluation")
    .config(RunnerConfig::new()
        .pass_threshold(0.8)      // Minimum score to pass
        .fail_fast(true)          // Stop on first failure
        .parallel(4));            // Run tests in parallel
```

## Evaluation Reports

Get detailed results:

```rust
let report = runner.run(&tests).await?;

// Summary statistics
println!("{}", report.summary());

// Individual results
for result in &report.results {
    println!("Test: {}", result.test_case.input);
    for metric_result in &result.metric_results {
        println!("  {}: {:.2}", metric_result.name, metric_result.score);
    }
}

// Overall metrics
println!("Pass rate: {:.1}%", report.pass_rate() * 100.0);
println!("Average score: {:.2}", report.average_score());
```

## Custom Metrics

Implement the `Metric` trait for custom evaluation logic:

```rust
use mcpkit_eval::{Metric, MetricResult, TestCase, EvalResult};
use async_trait::async_trait;

struct LengthMetric {
    max_length: usize,
}

#[async_trait]
impl Metric for LengthMetric {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let actual = test_case.actual_output.as_deref().unwrap_or("");
        let score = if actual.len() <= self.max_length { 1.0 } else { 0.0 };
        Ok(MetricResult::new("length", score))
    }

    fn name(&self) -> &str {
        "length"
    }
}
```

## Continuous Evaluation

Integrate with CI/CD pipelines:

```rust
let report = runner.run(&tests).await?;

// Exit with error if evaluation fails
if report.pass_rate() < 0.9 {
    eprintln!("Evaluation failed: {:.1}% pass rate", report.pass_rate() * 100.0);
    std::process::exit(1);
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
