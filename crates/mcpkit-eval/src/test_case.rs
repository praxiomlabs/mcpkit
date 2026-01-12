//! Test case types for LLM evaluation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A test case for LLM evaluation.
///
/// Test cases contain the inputs and expected outputs for evaluation.
/// They can be used for both RAG evaluation (with contexts) and
/// general LLM evaluation.
///
/// # Example
///
/// ```rust
/// use mcpkit_eval::TestCase;
///
/// let test = TestCase::new("What is the capital of France?")
///     .with_expected_output("Paris")
///     .with_context("France is a country in Western Europe. Its capital is Paris.");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Unique identifier for the test case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The input query or prompt.
    pub input: String,
    /// The expected output (ground truth).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output: Option<String>,
    /// The actual output from the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_output: Option<String>,
    /// Retrieved contexts (for RAG evaluation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<String>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl TestCase {
    /// Create a new test case with the given input.
    #[must_use]
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            id: None,
            input: input.into(),
            expected_output: None,
            actual_output: None,
            contexts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set the test case ID.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Generate a random UUID as the ID.
    #[must_use]
    pub fn with_generated_id(mut self) -> Self {
        self.id = Some(uuid::Uuid::new_v4().to_string());
        self
    }

    /// Set the expected output (ground truth).
    #[must_use]
    pub fn with_expected_output(mut self, output: impl Into<String>) -> Self {
        self.expected_output = Some(output.into());
        self
    }

    /// Set the actual output from the LLM.
    #[must_use]
    pub fn with_actual_output(mut self, output: impl Into<String>) -> Self {
        self.actual_output = Some(output.into());
        self
    }

    /// Add a single context.
    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.contexts.push(context.into());
        self
    }

    /// Set all contexts.
    #[must_use]
    pub fn with_contexts(mut self, contexts: Vec<String>) -> Self {
        self.contexts = contexts;
        self
    }

    /// Add metadata.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
        self
    }

    /// Check if this is a RAG test case (has contexts).
    #[must_use]
    pub fn is_rag_test(&self) -> bool {
        !self.contexts.is_empty()
    }

    /// Check if this test case has expected output.
    #[must_use]
    pub fn has_expected(&self) -> bool {
        self.expected_output.is_some()
    }

    /// Check if this test case has actual output.
    #[must_use]
    pub fn has_actual(&self) -> bool {
        self.actual_output.is_some()
    }

    /// Get the ID or a default value.
    #[must_use]
    pub fn id_or_default(&self) -> String {
        self.id
            .clone()
            .unwrap_or_else(|| format!("test-{}", uuid::Uuid::new_v4()))
    }
}

/// A dataset of test cases.
///
/// Datasets group test cases for batch evaluation.
///
/// # Example
///
/// ```rust
/// use mcpkit_eval::{TestCase, TestDataset};
///
/// let dataset = TestDataset::new("QA Benchmark")
///     .add_case(TestCase::new("Q1").with_expected_output("A1"))
///     .add_case(TestCase::new("Q2").with_expected_output("A2"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDataset {
    /// Name of the dataset.
    pub name: String,
    /// Description of the dataset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The test cases.
    pub test_cases: Vec<TestCase>,
    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl TestDataset {
    /// Create a new dataset with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            test_cases: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a test case.
    #[must_use]
    pub fn add_case(mut self, test_case: TestCase) -> Self {
        self.test_cases.push(test_case);
        self
    }

    /// Add multiple test cases.
    #[must_use]
    pub fn add_all(mut self, test_cases: impl IntoIterator<Item = TestCase>) -> Self {
        self.test_cases.extend(test_cases);
        self
    }

    /// Get the number of test cases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.test_cases.len()
    }

    /// Check if the dataset is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.test_cases.is_empty()
    }

    /// Iterate over test cases.
    pub fn iter(&self) -> impl Iterator<Item = &TestCase> {
        self.test_cases.iter()
    }

    /// Load a dataset from a JSON file.
    pub async fn from_json_file(path: impl AsRef<std::path::Path>) -> Result<Self, std::io::Error> {
        let content = tokio::fs::read_to_string(path).await?;
        let dataset: TestDataset = serde_json::from_str(&content)?;
        Ok(dataset)
    }

    /// Save the dataset to a JSON file.
    pub async fn to_json_file(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<(), std::io::Error> {
        let content = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }
}

impl IntoIterator for TestDataset {
    type Item = TestCase;
    type IntoIter = std::vec::IntoIter<TestCase>;

    fn into_iter(self) -> Self::IntoIter {
        self.test_cases.into_iter()
    }
}

impl<'a> IntoIterator for &'a TestDataset {
    type Item = &'a TestCase;
    type IntoIter = std::slice::Iter<'a, TestCase>;

    fn into_iter(self) -> Self::IntoIter {
        self.test_cases.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_case_creation() {
        let test = TestCase::new("What is Rust?");
        assert_eq!(test.input, "What is Rust?");
        assert!(test.expected_output.is_none());
        assert!(test.contexts.is_empty());
    }

    #[test]
    fn test_test_case_with_expected() {
        let test = TestCase::new("Question")
            .with_expected_output("Answer")
            .with_actual_output("Model answer");

        assert!(test.has_expected());
        assert!(test.has_actual());
        assert_eq!(test.expected_output, Some("Answer".to_string()));
    }

    #[test]
    fn test_test_case_with_contexts() {
        let test = TestCase::new("Question")
            .with_context("Context 1")
            .with_context("Context 2");

        assert!(test.is_rag_test());
        assert_eq!(test.contexts.len(), 2);
    }

    #[test]
    fn test_test_case_metadata() {
        let test = TestCase::new("Question")
            .with_metadata("category", "science")
            .with_metadata("difficulty", 3);

        assert_eq!(
            test.metadata.get("category"),
            Some(&serde_json::json!("science"))
        );
        assert_eq!(test.metadata.get("difficulty"), Some(&serde_json::json!(3)));
    }

    #[test]
    fn test_dataset_creation() {
        let dataset = TestDataset::new("Test Suite")
            .description("A test dataset")
            .add_case(TestCase::new("Q1"))
            .add_case(TestCase::new("Q2"));

        assert_eq!(dataset.name, "Test Suite");
        assert_eq!(dataset.len(), 2);
    }

    #[test]
    fn test_dataset_iteration() {
        let dataset = TestDataset::new("Suite")
            .add_case(TestCase::new("Q1"))
            .add_case(TestCase::new("Q2"));

        let inputs: Vec<_> = dataset.iter().map(|t| t.input.as_str()).collect();
        assert_eq!(inputs, vec!["Q1", "Q2"]);
    }
}
