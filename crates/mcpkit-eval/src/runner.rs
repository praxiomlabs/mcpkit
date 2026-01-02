//! Evaluation runner for batch processing test cases.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::EvalResult;
use crate::metric::{Metric, MetricResult};
use crate::test_case::{TestCase, TestDataset};

/// Configuration for the evaluation runner.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Whether to continue on metric errors.
    pub continue_on_error: bool,
    /// Whether to run metrics in parallel.
    pub parallel_metrics: bool,
    /// Pass threshold for metrics (default 0.5).
    pub pass_threshold: f64,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            continue_on_error: true,
            parallel_metrics: true,
            pass_threshold: 0.5,
        }
    }
}

impl RunnerConfig {
    /// Create a new configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to continue on errors.
    #[must_use]
    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    /// Set whether to run metrics in parallel.
    #[must_use]
    pub fn parallel_metrics(mut self, parallel: bool) -> Self {
        self.parallel_metrics = parallel;
        self
    }

    /// Set the pass threshold.
    #[must_use]
    pub fn pass_threshold(mut self, threshold: f64) -> Self {
        self.pass_threshold = threshold;
        self
    }
}

/// Result for a single test case across all metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseResult {
    /// The test case ID.
    pub test_id: String,
    /// Results for each metric.
    pub metric_results: HashMap<String, MetricResult>,
    /// Whether all metrics passed.
    pub passed: bool,
    /// Average score across all metrics.
    pub average_score: f64,
    /// Any errors encountered.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

impl TestCaseResult {
    /// Create a new test case result.
    fn new(test_id: String) -> Self {
        Self {
            test_id,
            metric_results: HashMap::new(),
            passed: true,
            average_score: 0.0,
            errors: Vec::new(),
        }
    }

    /// Add a metric result.
    fn add_result(&mut self, result: MetricResult) {
        if !result.passed {
            self.passed = false;
        }
        self.metric_results.insert(result.name.clone(), result);
    }

    /// Add an error.
    fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.passed = false;
    }

    /// Calculate the average score.
    fn calculate_average(&mut self) {
        if self.metric_results.is_empty() {
            self.average_score = 0.0;
        } else {
            let sum: f64 = self.metric_results.values().map(|r| r.score).sum();
            self.average_score = sum / self.metric_results.len() as f64;
        }
    }
}

/// Aggregated results from an evaluation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    /// Name of the evaluation.
    pub name: String,
    /// When the evaluation started.
    pub started_at: DateTime<Utc>,
    /// When the evaluation completed.
    pub completed_at: DateTime<Utc>,
    /// Total number of test cases.
    pub total_tests: usize,
    /// Number of passed tests.
    pub passed_tests: usize,
    /// Number of failed tests.
    pub failed_tests: usize,
    /// Pass rate (0.0 to 1.0).
    pub pass_rate: f64,
    /// Average score per metric.
    pub metric_averages: HashMap<String, f64>,
    /// Individual test case results.
    pub test_results: Vec<TestCaseResult>,
}

impl EvalReport {
    /// Create a new report.
    fn new(name: String, started_at: DateTime<Utc>) -> Self {
        Self {
            name,
            started_at,
            completed_at: Utc::now(),
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            pass_rate: 0.0,
            metric_averages: HashMap::new(),
            test_results: Vec::new(),
        }
    }

    /// Calculate summary statistics.
    fn finalize(&mut self) {
        self.completed_at = Utc::now();
        self.total_tests = self.test_results.len();
        self.passed_tests = self.test_results.iter().filter(|r| r.passed).count();
        self.failed_tests = self.total_tests - self.passed_tests;
        self.pass_rate = if self.total_tests > 0 {
            self.passed_tests as f64 / self.total_tests as f64
        } else {
            0.0
        };

        // Calculate per-metric averages
        let mut metric_sums: HashMap<String, (f64, usize)> = HashMap::new();
        for result in &self.test_results {
            for (name, metric_result) in &result.metric_results {
                let entry = metric_sums.entry(name.clone()).or_insert((0.0, 0));
                entry.0 += metric_result.score;
                entry.1 += 1;
            }
        }

        for (name, (sum, count)) in metric_sums {
            self.metric_averages.insert(name, sum / count as f64);
        }
    }

    /// Get a summary string.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "Evaluation: {}\n\
             Tests: {} total, {} passed, {} failed\n\
             Pass Rate: {:.1}%\n\
             Duration: {}ms\n\n\
             Metric Averages:\n",
            self.name,
            self.total_tests,
            self.passed_tests,
            self.failed_tests,
            self.pass_rate * 100.0,
            (self.completed_at - self.started_at).num_milliseconds()
        );

        for (metric, avg) in &self.metric_averages {
            summary.push_str(&format!("  {}: {:.3}\n", metric, avg));
        }

        summary
    }

    /// Save the report to a JSON file.
    pub async fn to_json_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }
}

/// The evaluation runner.
///
/// Runs a set of metrics against test cases and produces a report.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_eval::{EvalRunner, ExactMatchMetric, ContainsMetric, TestCase};
///
/// let mut runner = EvalRunner::new("QA Evaluation");
///
/// runner.add_metric(ExactMatchMetric::new());
/// runner.add_metric(ContainsMetric::new());
///
/// let tests = vec![
///     TestCase::new("Q1").with_expected_output("A1").with_actual_output("A1"),
///     TestCase::new("Q2").with_expected_output("B2").with_actual_output("Wrong"),
/// ];
///
/// let report = runner.run(&tests).await?;
/// println!("{}", report.summary());
/// ```
pub struct EvalRunner {
    name: String,
    metrics: Vec<Arc<dyn Metric>>,
    config: RunnerConfig,
}

impl EvalRunner {
    /// Create a new evaluation runner.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            metrics: Vec::new(),
            config: RunnerConfig::default(),
        }
    }

    /// Set the configuration.
    #[must_use]
    pub fn config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    /// Add a metric.
    pub fn add_metric<M: Metric + 'static>(&mut self, metric: M) {
        self.metrics.push(Arc::new(metric));
    }

    /// Add a metric (builder pattern).
    #[must_use]
    pub fn with_metric<M: Metric + 'static>(mut self, metric: M) -> Self {
        self.add_metric(metric);
        self
    }

    /// Run evaluation on test cases.
    pub async fn run(&self, test_cases: &[TestCase]) -> EvalResult<EvalReport> {
        let started_at = Utc::now();
        let mut report = EvalReport::new(self.name.clone(), started_at);

        for test_case in test_cases {
            let result = self.evaluate_test_case(test_case).await;
            report.test_results.push(result);
        }

        report.finalize();
        Ok(report)
    }

    /// Run evaluation on a dataset.
    pub async fn run_dataset(&self, dataset: &TestDataset) -> EvalResult<EvalReport> {
        self.run(&dataset.test_cases).await
    }

    /// Evaluate a single test case against all metrics.
    async fn evaluate_test_case(&self, test_case: &TestCase) -> TestCaseResult {
        let test_id = test_case.id_or_default();
        let mut result = TestCaseResult::new(test_id);

        if self.config.parallel_metrics {
            // Run metrics in parallel
            let futures: Vec<_> = self
                .metrics
                .iter()
                .map(|metric| {
                    let metric = Arc::clone(metric);
                    let test = test_case.clone();
                    async move { (metric.name().to_string(), metric.evaluate(&test).await) }
                })
                .collect();

            let results = futures::future::join_all(futures).await;

            for (name, metric_result) in results {
                match metric_result {
                    Ok(r) => result.add_result(r),
                    Err(e) => {
                        if self.config.continue_on_error {
                            result.add_error(format!("{}: {}", name, e));
                        } else {
                            result.add_error(format!("{}: {}", name, e));
                            break;
                        }
                    }
                }
            }
        } else {
            // Run metrics sequentially
            for metric in &self.metrics {
                match metric.evaluate(test_case).await {
                    Ok(r) => result.add_result(r),
                    Err(e) => {
                        if self.config.continue_on_error {
                            result.add_error(format!("{}: {}", metric.name(), e));
                        } else {
                            result.add_error(format!("{}: {}", metric.name(), e));
                            break;
                        }
                    }
                }
            }
        }

        result.calculate_average();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::{ContainsMetric, ExactMatchMetric};

    #[tokio::test]
    async fn test_runner_basic() {
        let runner = EvalRunner::new("Test Run")
            .with_metric(ExactMatchMetric::new())
            .with_metric(ContainsMetric::new());

        let tests = vec![
            TestCase::new("Q1")
                .with_id("test-1")
                .with_expected_output("Answer")
                .with_actual_output("Answer"),
            TestCase::new("Q2")
                .with_id("test-2")
                .with_expected_output("Hello")
                .with_actual_output("Hello World"),
        ];

        let report = runner.run(&tests).await.unwrap();

        assert_eq!(report.total_tests, 2);
        assert_eq!(report.name, "Test Run");
        assert!(!report.metric_averages.is_empty());
    }

    #[tokio::test]
    async fn test_runner_with_failures() {
        let runner = EvalRunner::new("Test Run").with_metric(ExactMatchMetric::new());

        let tests = vec![
            TestCase::new("Q1")
                .with_expected_output("A")
                .with_actual_output("A"),
            TestCase::new("Q2")
                .with_expected_output("B")
                .with_actual_output("Wrong"),
        ];

        let report = runner.run(&tests).await.unwrap();

        assert_eq!(report.passed_tests, 1);
        assert_eq!(report.failed_tests, 1);
        assert!((report.pass_rate - 0.5).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_test_case_result() {
        let mut result = TestCaseResult::new("test-1".to_string());
        result.add_result(MetricResult::new("metric1", 0.8));
        result.add_result(MetricResult::new("metric2", 0.6));
        result.calculate_average();

        assert!((result.average_score - 0.7).abs() < 0.001);
        assert_eq!(result.metric_results.len(), 2);
    }

    #[tokio::test]
    async fn test_report_summary() {
        let runner = EvalRunner::new("Summary Test").with_metric(ExactMatchMetric::new());

        let tests = vec![TestCase::new("Q1")
            .with_expected_output("A")
            .with_actual_output("A")];

        let report = runner.run(&tests).await.unwrap();
        let summary = report.summary();

        assert!(summary.contains("Summary Test"));
        assert!(summary.contains("100.0%"));
    }

    #[tokio::test]
    async fn test_runner_dataset() {
        let runner = EvalRunner::new("Dataset Test").with_metric(ExactMatchMetric::new());

        let dataset = TestDataset::new("Test Dataset")
            .add(
                TestCase::new("Q1")
                    .with_expected_output("A")
                    .with_actual_output("A"),
            )
            .add(
                TestCase::new("Q2")
                    .with_expected_output("B")
                    .with_actual_output("B"),
            );

        let report = runner.run_dataset(&dataset).await.unwrap();

        assert_eq!(report.total_tests, 2);
        assert_eq!(report.passed_tests, 2);
    }

    #[test]
    fn test_runner_config() {
        let config = RunnerConfig::new()
            .continue_on_error(false)
            .parallel_metrics(true)
            .pass_threshold(0.7);

        assert!(!config.continue_on_error);
        assert!(config.parallel_metrics);
        assert!((config.pass_threshold - 0.7).abs() < 0.001);
    }
}
