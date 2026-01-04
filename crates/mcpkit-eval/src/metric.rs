//! Evaluation metrics for LLM outputs.
//!
//! Metrics evaluate the quality of LLM outputs against various criteria.
//! They can be traditional (exact match, F1) or LLM-based (using an LLM
//! as a judge).

use std::sync::Arc;

use async_trait::async_trait;

use mcpkit_provider::{CompletionRequest, Message, Provider};

use crate::error::{EvalError, EvalResult};
use crate::test_case::TestCase;

/// The result of evaluating a metric.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricResult {
    /// The metric name.
    pub name: String,
    /// The score (typically 0.0 to 1.0).
    pub score: f64,
    /// Optional explanation of the score.
    pub reason: Option<String>,
    /// Whether evaluation passed (score above threshold).
    pub passed: bool,
}

impl MetricResult {
    /// Create a new metric result.
    #[must_use]
    pub fn new(name: impl Into<String>, score: f64) -> Self {
        Self {
            name: name.into(),
            score,
            reason: None,
            passed: score >= 0.5,
        }
    }

    /// Add a reason for the score.
    #[must_use]
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set a custom pass threshold.
    #[must_use]
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.passed = self.score >= threshold;
        self
    }
}

/// Trait for evaluation metrics.
///
/// Metrics take a test case and return a score indicating how well
/// the actual output matches expectations.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_eval::{Metric, ExactMatchMetric, TestCase};
///
/// let metric = ExactMatchMetric::new();
/// let test = TestCase::new("Question")
///     .with_expected_output("Answer")
///     .with_actual_output("Answer");
///
/// let result = metric.evaluate(&test).await?;
/// assert!(result.passed);
/// ```
#[async_trait]
pub trait Metric: Send + Sync {
    /// Evaluate the test case.
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult>;

    /// Get the metric name.
    fn name(&self) -> &str;

    /// Get the metric description.
    fn description(&self) -> &'static str {
        "Evaluation metric"
    }
}

/// Exact match metric - checks if actual equals expected.
///
/// Scores 1.0 if the outputs match exactly, 0.0 otherwise.
/// Case sensitivity and whitespace handling are configurable.
#[derive(Debug, Clone)]
pub struct ExactMatchMetric {
    case_sensitive: bool,
    normalize_whitespace: bool,
}

impl Default for ExactMatchMetric {
    fn default() -> Self {
        Self::new()
    }
}

impl ExactMatchMetric {
    /// Create a new exact match metric.
    #[must_use]
    pub fn new() -> Self {
        Self {
            case_sensitive: false,
            normalize_whitespace: true,
        }
    }

    /// Set case sensitivity.
    #[must_use]
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }

    /// Set whitespace normalization.
    #[must_use]
    pub fn normalize_whitespace(mut self, normalize: bool) -> Self {
        self.normalize_whitespace = normalize;
        self
    }

    fn normalize(&self, s: &str) -> String {
        let mut result = if self.case_sensitive {
            s.to_string()
        } else {
            s.to_lowercase()
        };

        if self.normalize_whitespace {
            result = result.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        result.trim().to_string()
    }
}

#[async_trait]
impl Metric for ExactMatchMetric {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let expected = test_case.expected_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("ExactMatch requires expected_output")
        })?;

        let actual = test_case.actual_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("ExactMatch requires actual_output")
        })?;

        let expected_norm = self.normalize(expected);
        let actual_norm = self.normalize(actual);

        let score = if expected_norm == actual_norm { 1.0 } else { 0.0 };

        Ok(MetricResult::new("exact_match", score).with_reason(if score == 1.0 {
            "Outputs match exactly".to_string()
        } else {
            format!(
                "Outputs differ: expected '{expected_norm}', got '{actual_norm}'"
            )
        }))
    }

    fn name(&self) -> &'static str {
        "exact_match"
    }

    fn description(&self) -> &'static str {
        "Checks if actual output exactly matches expected output"
    }
}

/// Contains metric - checks if actual contains expected.
///
/// Scores 1.0 if actual output contains the expected text.
#[derive(Debug, Clone)]
pub struct ContainsMetric {
    case_sensitive: bool,
}

impl Default for ContainsMetric {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainsMetric {
    /// Create a new contains metric.
    #[must_use]
    pub fn new() -> Self {
        Self {
            case_sensitive: false,
        }
    }

    /// Set case sensitivity.
    #[must_use]
    pub fn case_sensitive(mut self, sensitive: bool) -> Self {
        self.case_sensitive = sensitive;
        self
    }
}

#[async_trait]
impl Metric for ContainsMetric {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let expected = test_case.expected_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("Contains requires expected_output")
        })?;

        let actual = test_case.actual_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("Contains requires actual_output")
        })?;

        let (expected_cmp, actual_cmp) = if self.case_sensitive {
            (expected.clone(), actual.clone())
        } else {
            (expected.to_lowercase(), actual.to_lowercase())
        };

        let score = if actual_cmp.contains(&expected_cmp) {
            1.0
        } else {
            0.0
        };

        Ok(MetricResult::new("contains", score).with_reason(if score == 1.0 {
            "Actual output contains expected text".to_string()
        } else {
            format!("Actual output does not contain '{expected}'")
        }))
    }

    fn name(&self) -> &'static str {
        "contains"
    }

    fn description(&self) -> &'static str {
        "Checks if actual output contains expected text"
    }
}

/// Regex match metric - checks if actual matches a regex pattern.
#[derive(Debug, Clone)]
pub struct RegexMatchMetric {
    pattern: String,
}

impl RegexMatchMetric {
    /// Create a new regex match metric.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
        }
    }
}

#[async_trait]
impl Metric for RegexMatchMetric {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let actual = test_case.actual_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("RegexMatch requires actual_output")
        })?;

        let re = regex::Regex::new(&self.pattern).map_err(|e| {
            EvalError::parse(format!("Invalid regex pattern: {e}"))
        })?;

        let score = if re.is_match(actual) { 1.0 } else { 0.0 };

        Ok(MetricResult::new("regex_match", score).with_reason(if score == 1.0 {
            "Output matches pattern".to_string()
        } else {
            format!("Output does not match pattern '{}'", self.pattern)
        }))
    }

    fn name(&self) -> &'static str {
        "regex_match"
    }

    fn description(&self) -> &'static str {
        "Checks if actual output matches a regex pattern"
    }
}

/// LLM-as-judge metric for faithfulness evaluation.
///
/// Evaluates whether the answer is faithful to the provided context
/// (doesn't hallucinate information not in the context).
pub struct FaithfulnessMetric<P: Provider> {
    provider: Arc<P>,
    model: Option<String>,
}

impl<P: Provider + 'static> FaithfulnessMetric<P> {
    /// Create a new faithfulness metric.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            model: None,
        }
    }

    /// Create from an Arc'd provider.
    pub fn from_arc(provider: Arc<P>) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    /// Set the model to use for evaluation.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

#[async_trait]
impl<P: Provider + 'static> Metric for FaithfulnessMetric<P> {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let actual = test_case.actual_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("Faithfulness requires actual_output")
        })?;

        if test_case.contexts.is_empty() {
            return Err(EvalError::invalid_test_case(
                "Faithfulness requires contexts for RAG evaluation",
            ));
        }

        let context = test_case.contexts.join("\n\n");

        let prompt = format!(
            r#"You are evaluating the faithfulness of an AI-generated answer.

Context:
{context}

Answer:
{actual}

Evaluate whether the answer is faithful to the context. An answer is faithful if:
1. All claims in the answer can be verified from the context
2. The answer does not introduce information not present in the context
3. The answer does not contradict the context

Respond with a JSON object:
{{"score": <0.0-1.0>, "reason": "<explanation>"}}

Where score is:
- 1.0: Completely faithful, all claims supported by context
- 0.5-0.9: Mostly faithful with minor unsupported claims
- 0.1-0.4: Partially faithful with significant unsupported claims
- 0.0: Not faithful, major hallucinations or contradictions"#
        );

        let mut request = CompletionRequest::new()
            .message(Message::user(prompt))
            .temperature(0.0);

        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        let response = self.provider.complete(request).await?;
        let text = response.text().unwrap_or_default();

        // Parse the JSON response
        parse_llm_judge_response("faithfulness", &text)
    }

    fn name(&self) -> &'static str {
        "faithfulness"
    }

    fn description(&self) -> &'static str {
        "LLM-as-judge evaluation of answer faithfulness to context"
    }
}

/// LLM-as-judge metric for answer relevancy.
///
/// Evaluates whether the answer is relevant to the question.
pub struct AnswerRelevancyMetric<P: Provider> {
    provider: Arc<P>,
    model: Option<String>,
}

impl<P: Provider + 'static> AnswerRelevancyMetric<P> {
    /// Create a new answer relevancy metric.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            model: None,
        }
    }

    /// Create from an Arc'd provider.
    pub fn from_arc(provider: Arc<P>) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    /// Set the model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

#[async_trait]
impl<P: Provider + 'static> Metric for AnswerRelevancyMetric<P> {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        let actual = test_case.actual_output.as_ref().ok_or_else(|| {
            EvalError::invalid_test_case("AnswerRelevancy requires actual_output")
        })?;

        let prompt = format!(
            r#"You are evaluating the relevancy of an AI-generated answer to a question.

Question:
{}

Answer:
{}

Evaluate whether the answer is relevant to the question. An answer is relevant if:
1. It directly addresses the question asked
2. It provides information that helps answer the question
3. It stays on topic and doesn't include irrelevant information

Respond with a JSON object:
{{"score": <0.0-1.0>, "reason": "<explanation>"}}

Where score is:
- 1.0: Highly relevant, directly answers the question
- 0.5-0.9: Mostly relevant with some off-topic content
- 0.1-0.4: Partially relevant, misses key aspects of the question
- 0.0: Not relevant, doesn't address the question"#,
            test_case.input, actual
        );

        let mut request = CompletionRequest::new()
            .message(Message::user(prompt))
            .temperature(0.0);

        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        let response = self.provider.complete(request).await?;
        let text = response.text().unwrap_or_default();

        parse_llm_judge_response("answer_relevancy", &text)
    }

    fn name(&self) -> &'static str {
        "answer_relevancy"
    }

    fn description(&self) -> &'static str {
        "LLM-as-judge evaluation of answer relevancy to the question"
    }
}

/// LLM-as-judge metric for context precision.
///
/// Evaluates whether the retrieved contexts are precise (relevant to the question).
pub struct ContextPrecisionMetric<P: Provider> {
    provider: Arc<P>,
    model: Option<String>,
}

impl<P: Provider + 'static> ContextPrecisionMetric<P> {
    /// Create a new context precision metric.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            model: None,
        }
    }

    /// Create from an Arc'd provider.
    pub fn from_arc(provider: Arc<P>) -> Self {
        Self {
            provider,
            model: None,
        }
    }

    /// Set the model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

#[async_trait]
impl<P: Provider + 'static> Metric for ContextPrecisionMetric<P> {
    async fn evaluate(&self, test_case: &TestCase) -> EvalResult<MetricResult> {
        if test_case.contexts.is_empty() {
            return Err(EvalError::invalid_test_case(
                "ContextPrecision requires contexts",
            ));
        }

        let contexts = test_case
            .contexts
            .iter()
            .enumerate()
            .map(|(i, c)| format!("[Context {}]\n{}", i + 1, c))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            r#"You are evaluating the precision of retrieved contexts for answering a question.

Question:
{}

Retrieved Contexts:
{}

For each context, determine if it is relevant to answering the question.
Context precision measures what proportion of retrieved contexts are actually useful.

Respond with a JSON object:
{{"score": <0.0-1.0>, "reason": "<explanation>"}}

Where score is the proportion of relevant contexts (e.g., 2 relevant out of 4 contexts = 0.5)"#,
            test_case.input, contexts
        );

        let mut request = CompletionRequest::new()
            .message(Message::user(prompt))
            .temperature(0.0);

        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        let response = self.provider.complete(request).await?;
        let text = response.text().unwrap_or_default();

        parse_llm_judge_response("context_precision", &text)
    }

    fn name(&self) -> &'static str {
        "context_precision"
    }

    fn description(&self) -> &'static str {
        "LLM-as-judge evaluation of context precision (relevance of retrieved contexts)"
    }
}

/// Parse the JSON response from an LLM judge.
fn parse_llm_judge_response(metric_name: &str, response: &str) -> EvalResult<MetricResult> {
    // Try to extract JSON from the response
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    // Parse as JSON
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        EvalError::parse(format!(
            "Failed to parse LLM judge response as JSON: {e}. Response: {response}"
        ))
    })?;

    let score = parsed
        .get("score")
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| EvalError::parse("Missing 'score' field in LLM judge response"))?;

    let reason = parsed
        .get("reason")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);

    let mut result = MetricResult::new(metric_name, score);
    if let Some(r) = reason {
        result = result.with_reason(r);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exact_match_metric() {
        let metric = ExactMatchMetric::new();

        let test = TestCase::new("Question")
            .with_expected_output("Answer")
            .with_actual_output("answer");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 1.0); // Case insensitive by default
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_exact_match_case_sensitive() {
        let metric = ExactMatchMetric::new().case_sensitive(true);

        let test = TestCase::new("Question")
            .with_expected_output("Answer")
            .with_actual_output("answer");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 0.0);
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_exact_match_whitespace_normalization() {
        let metric = ExactMatchMetric::new();

        let test = TestCase::new("Question")
            .with_expected_output("Hello World")
            .with_actual_output("  hello   world  ");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 1.0);
    }

    #[tokio::test]
    async fn test_contains_metric() {
        let metric = ContainsMetric::new();

        let test = TestCase::new("Question")
            .with_expected_output("Rust")
            .with_actual_output("Rust is a programming language");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_contains_metric_not_found() {
        let metric = ContainsMetric::new();

        let test = TestCase::new("Question")
            .with_expected_output("Python")
            .with_actual_output("Rust is a programming language");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 0.0);
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn test_regex_match_metric() {
        let metric = RegexMatchMetric::new(r"\d{3}-\d{4}");

        let test = TestCase::new("Question")
            .with_actual_output("Call 555-1234 for help");

        let result = metric.evaluate(&test).await.unwrap();
        assert_eq!(result.score, 1.0);
    }

    #[test]
    fn test_metric_result() {
        let result = MetricResult::new("test", 0.75)
            .with_reason("Good score")
            .with_threshold(0.7);

        assert_eq!(result.name, "test");
        assert_eq!(result.score, 0.75);
        assert!(result.passed);
        assert_eq!(result.reason, Some("Good score".to_string()));
    }

    #[test]
    fn test_parse_llm_judge_response() {
        let response = r#"{"score": 0.85, "reason": "The answer is mostly correct"}"#;
        let result = parse_llm_judge_response("test", response).unwrap();

        assert_eq!(result.score, 0.85);
        assert_eq!(
            result.reason,
            Some("The answer is mostly correct".to_string())
        );
    }

    #[test]
    fn test_parse_llm_judge_response_with_text() {
        let response = r#"Here is my evaluation: {"score": 0.9, "reason": "Good"} That's my assessment."#;
        let result = parse_llm_judge_response("test", response).unwrap();

        assert_eq!(result.score, 0.9);
    }
}
