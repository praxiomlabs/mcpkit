//! Core vector store trait and types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::distance::DistanceMetric;
use crate::error::EmbeddingResult;

/// A stored embedding with its ID and optional metadata.
///
/// This represents a single vector in the store, associated with a unique
/// identifier and optional JSON metadata for filtering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEmbedding {
    /// Unique identifier for this embedding.
    pub id: String,
    /// The embedding vector.
    pub embedding: Vec<f32>,
    /// Optional metadata for filtering and retrieval.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl StoredEmbedding {
    /// Create a new stored embedding without metadata.
    #[must_use]
    pub fn new(id: impl Into<String>, embedding: Vec<f32>) -> Self {
        Self {
            id: id.into(),
            embedding,
            metadata: HashMap::new(),
        }
    }

    /// Create a stored embedding with metadata.
    #[must_use]
    pub fn with_metadata(
        id: impl Into<String>,
        embedding: Vec<f32>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            id: id.into(),
            embedding,
            metadata,
        }
    }

    /// Get the dimensionality of the embedding.
    #[must_use]
    pub fn dimensions(&self) -> usize {
        self.embedding.len()
    }

    /// Add a metadata field.
    pub fn insert_metadata(&mut self, key: impl Into<String>, value: impl Serialize) {
        if let Ok(v) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), v);
        }
    }

    /// Get a metadata value by key.
    #[must_use]
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}

/// A search result from the vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The ID of the matching embedding.
    pub id: String,
    /// The similarity/distance score.
    ///
    /// Interpretation depends on the metric:
    /// - Cosine: -1.0 to 1.0 (higher = more similar)
    /// - `DotProduct`: higher = more similar
    /// - Euclidean: >= 0 (lower = more similar)
    pub score: f32,
    /// The matching embedding vector (if requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// The metadata (if present).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SearchResult {
    /// Create a new search result.
    #[must_use]
    pub fn new(id: String, score: f32) -> Self {
        Self {
            id,
            score,
            embedding: None,
            metadata: HashMap::new(),
        }
    }

    /// Add the embedding vector to the result.
    #[must_use]
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Add metadata to the result.
    #[must_use]
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Options for searching the vector store.
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Number of results to return.
    pub k: usize,
    /// Whether to include embedding vectors in results.
    pub include_embeddings: bool,
    /// Whether to include metadata in results.
    pub include_metadata: bool,
    /// Optional minimum similarity threshold.
    ///
    /// For cosine/dot: minimum score to include.
    /// For euclidean: maximum distance to include.
    pub threshold: Option<f32>,
}

impl SearchOptions {
    /// Create search options for top-k results.
    #[must_use]
    pub fn top_k(k: usize) -> Self {
        Self {
            k,
            include_embeddings: false,
            include_metadata: true,
            threshold: None,
        }
    }

    /// Include embedding vectors in results.
    #[must_use]
    pub fn with_embeddings(mut self) -> Self {
        self.include_embeddings = true;
        self
    }

    /// Exclude metadata from results.
    #[must_use]
    pub fn without_metadata(mut self) -> Self {
        self.include_metadata = false;
        self
    }

    /// Set a similarity threshold.
    #[must_use]
    pub fn threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }
}

/// Trait for vector storage implementations.
///
/// This trait defines the core interface for storing and searching embeddings.
/// Implementations can range from simple in-memory stores to distributed
/// vector databases.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_embedding::{VectorStore, InMemoryStore, StoredEmbedding, SearchOptions};
///
/// let mut store = InMemoryStore::new();
///
/// // Insert embeddings
/// store.insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0])).await?;
/// store.insert(StoredEmbedding::new("doc2", vec![0.9, 0.1, 0.0])).await?;
///
/// // Search
/// let query = vec![1.0, 0.0, 0.0];
/// let results = store.search(&query, SearchOptions::top_k(5)).await?;
/// ```
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert a single embedding into the store.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The ID already exists (use `upsert` for update-or-insert)
    /// - The dimensions don't match the store's configuration
    async fn insert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()>;

    /// Insert multiple embeddings in a batch.
    ///
    /// Default implementation calls `insert` for each item.
    async fn insert_batch(&mut self, items: Vec<StoredEmbedding>) -> EmbeddingResult<()> {
        for item in items {
            self.insert(item).await?;
        }
        Ok(())
    }

    /// Update or insert an embedding.
    ///
    /// If the ID exists, updates the embedding. Otherwise, inserts it.
    async fn upsert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()>;

    /// Get an embedding by its ID.
    async fn get(&self, id: &str) -> EmbeddingResult<Option<StoredEmbedding>>;

    /// Delete an embedding by its ID.
    ///
    /// Returns `Ok(true)` if the item was deleted, `Ok(false)` if it didn't exist.
    async fn delete(&mut self, id: &str) -> EmbeddingResult<bool>;

    /// Search for similar embeddings.
    ///
    /// Returns results ordered by similarity (most similar first).
    async fn search(
        &self,
        query: &[f32],
        options: SearchOptions,
    ) -> EmbeddingResult<Vec<SearchResult>>;

    /// Get the number of embeddings in the store.
    fn len(&self) -> usize;

    /// Check if the store is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all embeddings from the store.
    async fn clear(&mut self);

    /// Get the distance metric used by this store.
    fn metric(&self) -> DistanceMetric;

    /// Get the expected dimensions for embeddings (if fixed).
    fn dimensions(&self) -> Option<usize>;
}
