//! LLM evaluation and testing framework for mcpkit-forge.
//!
//! `mcpkit-eval` provides tools for evaluating LLM outputs and RAG pipelines.
//! It supports both traditional metrics (exact match, contains) and LLM-as-judge
//! metrics (faithfulness, answer relevancy, context precision).
//!
//! # Features
//!
//! - **Test cases**: Structured test data with inputs, expected outputs, and contexts
//! - **Metrics**: Pluggable evaluation metrics (traditional and LLM-based)
//! - **Runner**: Batch evaluation with aggregated reporting
//! - **Datasets**: Organize and persist test cases
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use mcpkit_eval::{
//!     EvalRunner, ExactMatchMetric, ContainsMetric,
//!     TestCase, TestDataset, RunnerConfig,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create test cases
//!     let tests = vec![
//!         TestCase::new("What is 2+2?")
//!             .with_expected_output("4")
//!             .with_actual_output("The answer is 4"),
//!         TestCase::new("What is the capital of France?")
//!             .with_expected_output("Paris")
//!             .with_actual_output("Paris is the capital"),
//!     ];
//!
//!     // Set up runner with metrics
//!     let runner = EvalRunner::new("QA Evaluation")
//!         .with_metric(ExactMatchMetric::new())
//!         .with_metric(ContainsMetric::new())
//!         .config(RunnerConfig::new().pass_threshold(0.5));
//!
//!     // Run evaluation
//!     let report = runner.run(&tests).await?;
//!     println!("{}", report.summary());
//!
//!     Ok(())
//! }
//! ```
//!
//! # RAG Evaluation
//!
//! For RAG pipelines, use LLM-as-judge metrics:
//!
//! ```rust,ignore
//! use mcpkit_eval::{
//!     EvalRunner, FaithfulnessMetric, AnswerRelevancyMetric,
//!     ContextPrecisionMetric, TestCase,
//! };
//! use mcpkit_provider::openai::OpenAiProvider;
//!
//! let provider = OpenAiProvider::new(api_key)?;
//!
//! let runner = EvalRunner::new("RAG Evaluation")
//!     .with_metric(FaithfulnessMetric::new(provider.clone()))
//!     .with_metric(AnswerRelevancyMetric::new(provider.clone()))
//!     .with_metric(ContextPrecisionMetric::new(provider));
//!
//! let test = TestCase::new("What is Rust?")
//!     .with_context("Rust is a systems programming language.")
//!     .with_actual_output("Rust is a programming language for systems.");
//!
//! let report = runner.run(&[test]).await?;
//! ```
//!
//! # Available Metrics
//!
//! ## Traditional Metrics
//!
//! | Metric | Description |
//! |--------|-------------|
//! | `ExactMatchMetric` | Exact string match (configurable case/whitespace) |
//! | `ContainsMetric` | Checks if output contains expected text |
//! | `RegexMatchMetric` | Matches output against a regex pattern |
//!
//! ## LLM-as-Judge Metrics
//!
//! | Metric | Description |
//! |--------|-------------|
//! | `FaithfulnessMetric` | Is the answer faithful to the context? |
//! | `AnswerRelevancyMetric` | Is the answer relevant to the question? |
//! | `ContextPrecisionMetric` | Are the retrieved contexts relevant? |
//!
//! # Test Datasets
//!
//! Organize test cases into datasets for reuse:
//!
//! ```rust
//! use mcpkit_eval::{TestCase, TestDataset};
//!
//! let dataset = TestDataset::new("QA Benchmark")
//!     .description("Common question-answering tests")
//!     .add_case(TestCase::new("Q1").with_expected_output("A1"))
//!     .add_case(TestCase::new("Q2").with_expected_output("A2"));
//!
//! // Datasets can be saved/loaded from JSON
//! // dataset.to_json_file("qa_benchmark.json").await?;
//! // let loaded = TestDataset::from_json_file("qa_benchmark.json").await?;
//! ```
//!
//! # Custom Metrics
//!
//! Implement the `Metric` trait for custom evaluation logic:
//!
//! ```rust,ignore
//! use mcpkit_eval::{Metric, MetricResult, TestCase, EvalResult};
//! use async_trait::async_trait;
//!
//! struct LengthMetric {
//!     max_length: usize,
//! }
//!
//! #[async_trait]
//! impl Metric for LengthMetric {
//!     async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
//!         let actual = test_case.actual_output.as_deref().unwrap_or("");
//!         let score = if actual.len() <= self.max_length { 1.0 } else { 0.0 };
//!         Ok(MetricResult::new("length", score))
//!     }
//!
//!     fn name(&self) -> &str {
//!         "length"
//!     }
//! }
//! ```

#![warn(missing_docs)]

mod error;
mod metric;
mod runner;
mod test_case;

// Re-exports
pub use error::{EvalError, EvalResult};
pub use metric::{
    AnswerRelevancyMetric, ContainsMetric, ContextPrecisionMetric, ExactMatchMetric,
    FaithfulnessMetric, Metric, MetricResult, RegexMatchMetric,
};
pub use runner::{EvalReport, EvalRunner, RunnerConfig, TestCaseResult};
pub use test_case::{TestCase, TestDataset};
