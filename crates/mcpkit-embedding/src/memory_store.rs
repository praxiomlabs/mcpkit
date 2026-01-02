//! In-memory vector store implementation.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::distance::DistanceMetric;
use crate::error::{EmbeddingError, EmbeddingResult};
use crate::store::{SearchOptions, SearchResult, StoredEmbedding, VectorStore};

/// A simple in-memory vector store.
///
/// `InMemoryStore` provides fast vector similarity search using a linear
/// scan with optimized distance calculations. It's suitable for:
///
/// - Development and testing
/// - Small to medium datasets (up to ~100k vectors)
/// - Applications where simplicity is preferred
///
/// For larger datasets, consider using a dedicated vector database
/// like Qdrant or Pinecone.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_embedding::{InMemoryStore, StoredEmbedding, SearchOptions, DistanceMetric};
///
/// // Create a store with cosine similarity
/// let mut store = InMemoryStore::new();
///
/// // Insert embeddings
/// store.insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0])).await?;
/// store.insert(StoredEmbedding::new("doc2", vec![0.707, 0.707, 0.0])).await?;
///
/// // Search for similar vectors
/// let results = store.search(&[1.0, 0.0, 0.0], SearchOptions::top_k(5)).await?;
/// assert_eq!(results[0].id, "doc1"); // Most similar
/// ```
#[derive(Debug, Clone)]
pub struct InMemoryStore {
    /// Stored embeddings by ID.
    embeddings: HashMap<String, StoredEmbedding>,
    /// Distance metric to use.
    metric: DistanceMetric,
    /// Expected dimensions (set from first insertion).
    dimensions: Option<usize>,
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStore {
    /// Create a new empty in-memory store with cosine similarity.
    #[must_use]
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            metric: DistanceMetric::Cosine,
            dimensions: None,
        }
    }

    /// Create a store with a specific distance metric.
    #[must_use]
    pub fn with_metric(metric: DistanceMetric) -> Self {
        Self {
            embeddings: HashMap::new(),
            metric,
            dimensions: None,
        }
    }

    /// Create a store with fixed dimensions.
    ///
    /// All inserted embeddings must have exactly this number of dimensions.
    #[must_use]
    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            embeddings: HashMap::new(),
            metric: DistanceMetric::Cosine,
            dimensions: Some(dimensions),
        }
    }

    /// Create a store with a specific metric and fixed dimensions.
    #[must_use]
    pub fn with_metric_and_dimensions(metric: DistanceMetric, dimensions: usize) -> Self {
        Self {
            embeddings: HashMap::new(),
            metric,
            dimensions: Some(dimensions),
        }
    }

    /// Check if an ID exists in the store.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.embeddings.contains_key(id)
    }

    /// Get all IDs in the store.
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.embeddings.keys().map(String::as_str).collect()
    }

    /// Validate embedding dimensions.
    fn validate_dimensions(&self, embedding: &StoredEmbedding) -> EmbeddingResult<()> {
        if let Some(expected) = self.dimensions {
            if embedding.embedding.len() != expected {
                return Err(EmbeddingError::DimensionMismatch {
                    expected,
                    actual: embedding.embedding.len(),
                });
            }
        }
        Ok(())
    }
}

#[async_trait]
impl VectorStore for InMemoryStore {
    async fn insert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()> {
        // Check for duplicate
        if self.embeddings.contains_key(&item.id) {
            return Err(EmbeddingError::DuplicateId { id: item.id });
        }

        // Validate or set dimensions
        if self.dimensions.is_none() && !item.embedding.is_empty() {
            self.dimensions = Some(item.embedding.len());
        }
        self.validate_dimensions(&item)?;

        self.embeddings.insert(item.id.clone(), item);
        Ok(())
    }

    async fn upsert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()> {
        // Validate or set dimensions
        if self.dimensions.is_none() && !item.embedding.is_empty() {
            self.dimensions = Some(item.embedding.len());
        }
        self.validate_dimensions(&item)?;

        self.embeddings.insert(item.id.clone(), item);
        Ok(())
    }

    async fn get(&self, id: &str) -> EmbeddingResult<Option<StoredEmbedding>> {
        Ok(self.embeddings.get(id).cloned())
    }

    async fn delete(&mut self, id: &str) -> EmbeddingResult<bool> {
        Ok(self.embeddings.remove(id).is_some())
    }

    async fn search(
        &self,
        query: &[f32],
        options: SearchOptions,
    ) -> EmbeddingResult<Vec<SearchResult>> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }

        // Validate query dimensions
        if let Some(expected) = self.dimensions {
            if query.len() != expected {
                return Err(EmbeddingError::DimensionMismatch {
                    expected,
                    actual: query.len(),
                });
            }
        }

        // Calculate scores for all embeddings
        let mut scored: Vec<(String, f32, &StoredEmbedding)> = self
            .embeddings
            .values()
            .filter_map(|stored| {
                let score = self.metric.calculate(query, &stored.embedding)?;

                // Apply threshold filter
                if let Some(threshold) = options.threshold {
                    if self.metric.higher_is_better() {
                        if score < threshold {
                            return None;
                        }
                    } else if score > threshold {
                        return None;
                    }
                }

                Some((stored.id.clone(), score, stored))
            })
            .collect();

        // Sort by score
        if self.metric.higher_is_better() {
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        }

        // Take top-k and build results
        let results: Vec<SearchResult> = scored
            .into_iter()
            .take(options.k)
            .map(|(id, score, stored)| {
                let mut result = SearchResult::new(id, score);

                if options.include_embeddings {
                    result = result.with_embedding(stored.embedding.clone());
                }

                if options.include_metadata && !stored.metadata.is_empty() {
                    result = result.with_metadata(stored.metadata.clone());
                }

                result
            })
            .collect();

        Ok(results)
    }

    fn len(&self) -> usize {
        self.embeddings.len()
    }

    async fn clear(&mut self) {
        self.embeddings.clear();
        // Note: we keep dimensions since they're part of store configuration
    }

    fn metric(&self) -> DistanceMetric {
        self.metric
    }

    fn dimensions(&self) -> Option<usize> {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_get() {
        let mut store = InMemoryStore::new();

        let item = StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]);
        store.insert(item).await.unwrap();

        let retrieved = store.get("doc1").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "doc1");

        let missing = store.get("nonexistent").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_duplicate_insert() {
        let mut store = InMemoryStore::new();

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0]))
            .await
            .unwrap();

        let result = store
            .insert(StoredEmbedding::new("doc1", vec![0.0, 1.0]))
            .await;

        assert!(matches!(result, Err(EmbeddingError::DuplicateId { .. })));
    }

    #[tokio::test]
    async fn test_upsert() {
        let mut store = InMemoryStore::new();

        store
            .upsert(StoredEmbedding::new("doc1", vec![1.0, 0.0]))
            .await
            .unwrap();

        store
            .upsert(StoredEmbedding::new("doc1", vec![0.0, 1.0]))
            .await
            .unwrap();

        let item = store.get("doc1").await.unwrap().unwrap();
        assert_eq!(item.embedding, vec![0.0, 1.0]);
    }

    #[tokio::test]
    async fn test_delete() {
        let mut store = InMemoryStore::new();

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0]))
            .await
            .unwrap();

        assert!(store.delete("doc1").await.unwrap());
        assert!(!store.delete("doc1").await.unwrap());
        assert!(store.get("doc1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_search_cosine() {
        let mut store = InMemoryStore::new();

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("doc2", vec![0.707, 0.707, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("doc3", vec![0.0, 1.0, 0.0]))
            .await
            .unwrap();

        let results = store
            .search(&[1.0, 0.0, 0.0], SearchOptions::top_k(2))
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc1"); // Exact match
        assert_eq!(results[1].id, "doc2"); // 45 degree angle
    }

    #[tokio::test]
    async fn test_search_with_threshold() {
        let mut store = InMemoryStore::new();

        store
            .insert(StoredEmbedding::new("similar", vec![1.0, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("different", vec![0.0, 1.0]))
            .await
            .unwrap();

        let results = store
            .search(&[1.0, 0.0], SearchOptions::top_k(10).threshold(0.5))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "similar");
    }

    #[tokio::test]
    async fn test_search_euclidean() {
        let mut store = InMemoryStore::with_metric(DistanceMetric::Euclidean);

        store
            .insert(StoredEmbedding::new("close", vec![1.0, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("far", vec![10.0, 10.0]))
            .await
            .unwrap();

        let results = store
            .search(&[0.0, 0.0], SearchOptions::top_k(2))
            .await
            .unwrap();

        assert_eq!(results[0].id, "close"); // Distance ~1.0
        assert_eq!(results[1].id, "far"); // Distance ~14.14
    }

    #[tokio::test]
    async fn test_dimension_validation() {
        let mut store = InMemoryStore::with_dimensions(3);

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();

        let result = store
            .insert(StoredEmbedding::new("doc2", vec![1.0, 0.0]))
            .await;

        assert!(matches!(
            result,
            Err(EmbeddingError::DimensionMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn test_search_with_metadata() {
        let mut store = InMemoryStore::new();

        let mut item = StoredEmbedding::new("doc1", vec![1.0, 0.0]);
        item.insert_metadata("title", "Hello World");
        item.insert_metadata("score", 42);
        store.insert(item).await.unwrap();

        let results = store
            .search(&[1.0, 0.0], SearchOptions::top_k(1))
            .await
            .unwrap();

        assert_eq!(results[0].metadata.get("title").unwrap(), "Hello World");
        assert_eq!(results[0].metadata.get("score").unwrap(), 42);
    }

    #[tokio::test]
    async fn test_clear() {
        let mut store = InMemoryStore::new();

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("doc2", vec![2.0]))
            .await
            .unwrap();

        assert_eq!(store.len(), 2);
        store.clear().await;
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn test_empty_search() {
        let store = InMemoryStore::new();
        let results = store
            .search(&[1.0, 0.0], SearchOptions::top_k(5))
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
