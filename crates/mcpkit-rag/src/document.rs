//! Document types for RAG pipelines.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A document in a RAG pipeline.
///
/// Documents represent text content with associated metadata. They can be
/// loaded from various sources, split into chunks, and indexed for retrieval.
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::Document;
///
/// let doc = Document::new("This is the document content")
///     .with_id("doc-1")
///     .with_metadata("source", "user_manual.pdf")
///     .with_metadata("page", 42);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// The text content of the document.
    pub content: String,
    /// Optional unique identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Metadata associated with the document.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Document {
    /// Create a new document with the given content.
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the document ID.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Generate a random UUID as the document ID.
    #[must_use]
    pub fn with_generated_id(mut self) -> Self {
        self.id = Some(uuid::Uuid::new_v4().to_string());
        self
    }

    /// Add a metadata field.
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
        self
    }

    /// Get a metadata value by key.
    #[must_use]
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Get a metadata value as a specific type.
    pub fn get_metadata_as<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.metadata
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the length of the content in characters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if the document content is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get the ID or generate a default one.
    #[must_use]
    pub fn id_or_default(&self) -> String {
        self.id
            .clone()
            .unwrap_or_else(|| format!("doc-{}", uuid::Uuid::new_v4()))
    }
}

impl From<&str> for Document {
    fn from(s: &str) -> Self {
        Document::new(s)
    }
}

impl From<String> for Document {
    fn from(s: String) -> Self {
        Document::new(s)
    }
}

/// A retrieved document with its similarity score.
///
/// This is returned from retrieval operations and includes the document
/// content along with a score indicating how relevant it is to the query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedDocument {
    /// The document content.
    pub document: Document,
    /// The similarity score.
    ///
    /// Higher scores indicate more relevant documents.
    pub score: f32,
}

impl RetrievedDocument {
    /// Create a new retrieved document.
    #[must_use]
    pub fn new(document: Document, score: f32) -> Self {
        Self { document, score }
    }

    /// Get the document content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.document.content
    }

    /// Get the document ID if present.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.document.id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new("Hello, world!");
        assert_eq!(doc.content, "Hello, world!");
        assert!(doc.id.is_none());
        assert!(doc.metadata.is_empty());
    }

    #[test]
    fn test_document_with_id() {
        let doc = Document::new("Content").with_id("doc-1");
        assert_eq!(doc.id, Some("doc-1".to_string()));
    }

    #[test]
    fn test_document_with_metadata() {
        let doc = Document::new("Content")
            .with_metadata("source", "test.txt")
            .with_metadata("page", 42);

        assert_eq!(
            doc.get_metadata("source"),
            Some(&serde_json::json!("test.txt"))
        );
        assert_eq!(doc.get_metadata_as::<i32>("page"), Some(42));
    }

    #[test]
    fn test_document_from_str() {
        let doc: Document = "Hello".into();
        assert_eq!(doc.content, "Hello");
    }

    #[test]
    fn test_retrieved_document() {
        let doc = Document::new("Test content").with_id("doc-1");
        let retrieved = RetrievedDocument::new(doc, 0.95);

        assert_eq!(retrieved.content(), "Test content");
        assert_eq!(retrieved.id(), Some("doc-1"));
        assert!((retrieved.score - 0.95).abs() < 0.001);
    }
}
