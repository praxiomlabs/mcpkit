//! SQLite-based vector store using sqlite-vec extension.
//!
//! This module provides a persistent vector store backed by `SQLite` with the
//! sqlite-vec extension for efficient vector similarity search.
//!
//! # Safety
//!
//! This module uses `unsafe` code to load the sqlite-vec extension via FFI.
//! The unsafe block is minimal and well-contained - it only registers the
//! extension initialization function with `SQLite`'s auto-extension mechanism.
//! This is required because `SQLite` extension loading inherently involves
//! FFI calls that cannot be verified by Rust's borrow checker.

#![allow(unsafe_code)]

//! # Features
//!
//! - **Persistent storage**: Vectors survive process restarts
//! - **File-based**: Single file database, easy to deploy
//! - **Cross-platform**: Works anywhere `SQLite` works
//! - **Efficient**: Uses sqlite-vec's optimized brute-force search
//!
//! # Example
//!
//! ```rust,ignore
//! use mcpkit_embedding::{SqliteVecStore, VectorStore, StoredEmbedding, SearchOptions};
//!
//! // Create or open a database
//! let mut store = SqliteVecStore::open("vectors.db", 384)?;
//!
//! // Insert embeddings
//! store.insert(StoredEmbedding::new("doc1", vec![0.1; 384])).await?;
//!
//! // Search
//! let results = store.search(&query_vec, SearchOptions::top_k(5)).await?;
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use rusqlite::{Connection, params};

use crate::distance::DistanceMetric;
use crate::error::{EmbeddingError, EmbeddingResult};
use crate::store::{SearchOptions, SearchResult, StoredEmbedding, VectorStore};

/// A persistent vector store backed by `SQLite` with sqlite-vec extension.
///
/// This store uses the sqlite-vec extension to provide efficient vector
/// similarity search with persistent storage. It supports cosine similarity
/// via normalized vectors.
pub struct SqliteVecStore {
    conn: Mutex<Connection>,
    dimensions: usize,
    metric: DistanceMetric,
    count: Mutex<usize>,
}

impl SqliteVecStore {
    /// Create a new `SQLite` vector store, creating the database if it doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `SQLite` database file
    /// * `dimensions` - Expected dimensionality of vectors
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or sqlite-vec cannot be loaded.
    pub fn open(path: impl AsRef<Path>, dimensions: usize) -> EmbeddingResult<Self> {
        Self::open_with_metric(path, dimensions, DistanceMetric::Cosine)
    }

    /// Create a new `SQLite` vector store with a specific distance metric.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the `SQLite` database file
    /// * `dimensions` - Expected dimensionality of vectors
    /// * `metric` - Distance metric to use for similarity search
    pub fn open_with_metric(
        path: impl AsRef<Path>,
        dimensions: usize,
        metric: DistanceMetric,
    ) -> EmbeddingResult<Self> {
        // Load the sqlite-vec extension
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open(path.as_ref()).map_err(|e| EmbeddingError::Storage {
            message: format!("Failed to open SQLite database: {e}"),
        })?;

        // Create tables
        conn.execute_batch(&format!(
            r"
            CREATE TABLE IF NOT EXISTS embeddings (
                id TEXT PRIMARY KEY,
                metadata TEXT DEFAULT '{{}}'
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS vec_embeddings USING vec0(
                embedding float[{dimensions}]
            );
            "
        ))
        .map_err(|e| EmbeddingError::Storage {
            message: format!("Failed to create tables: {e}"),
        })?;

        // Count existing embeddings
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
            .unwrap_or(0);

        Ok(Self {
            conn: Mutex::new(conn),
            dimensions,
            metric,
            count: Mutex::new(count),
        })
    }

    /// Create an in-memory `SQLite` vector store (useful for testing).
    ///
    /// # Arguments
    ///
    /// * `dimensions` - Expected dimensionality of vectors
    pub fn in_memory(dimensions: usize) -> EmbeddingResult<Self> {
        Self::open(":memory:", dimensions)
    }

    /// Convert a vector to bytes for sqlite-vec.
    fn vec_to_bytes(vec: &[f32]) -> Vec<u8> {
        vec.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Normalize a vector for cosine similarity.
    fn normalize(vec: &[f32]) -> Vec<f32> {
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            vec.iter().map(|x| x / magnitude).collect()
        } else {
            vec.to_vec()
        }
    }
}

impl std::fmt::Debug for SqliteVecStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteVecStore")
            .field("dimensions", &self.dimensions)
            .field("metric", &self.metric)
            .field("count", &*self.count.lock().unwrap())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl VectorStore for SqliteVecStore {
    async fn insert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()> {
        if item.dimensions() != self.dimensions {
            return Err(EmbeddingError::DimensionMismatch {
                expected: self.dimensions,
                actual: item.dimensions(),
            });
        }

        let conn = self.conn.lock().unwrap();

        // Check if ID already exists
        let exists: bool = conn
            .query_row("SELECT 1 FROM embeddings WHERE id = ?", [&item.id], |_| {
                Ok(true)
            })
            .unwrap_or(false);

        if exists {
            return Err(EmbeddingError::DuplicateId { id: item.id });
        }

        // Normalize for cosine similarity
        let embedding = if self.metric == DistanceMetric::Cosine {
            Self::normalize(&item.embedding)
        } else {
            item.embedding.clone()
        };

        let metadata_json = serde_json::to_string(&item.metadata).unwrap_or_else(|_| "{}".into());
        let embedding_bytes = Self::vec_to_bytes(&embedding);

        // Get the next rowid for vec_embeddings
        let next_rowid: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(rowid), 0) + 1 FROM vec_embeddings",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Insert into both tables
        conn.execute(
            "INSERT INTO embeddings (id, metadata) VALUES (?, ?)",
            params![&item.id, &metadata_json],
        )
        .map_err(|e| EmbeddingError::Storage {
            message: format!("Failed to insert embedding metadata: {e}"),
        })?;

        conn.execute(
            "INSERT INTO vec_embeddings (rowid, embedding) VALUES (?, ?)",
            params![next_rowid, &embedding_bytes],
        )
        .map_err(|e| EmbeddingError::Storage {
            message: format!("Failed to insert embedding vector: {e}"),
        })?;

        // Update the id->rowid mapping (store rowid in a way we can look it up)
        // We use a separate mapping table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS id_rowid_map (id TEXT PRIMARY KEY, rowid INTEGER)",
            [],
        )
        .ok();

        conn.execute(
            "INSERT OR REPLACE INTO id_rowid_map (id, rowid) VALUES (?, ?)",
            params![&item.id, next_rowid],
        )
        .map_err(|e| EmbeddingError::Storage {
            message: format!("Failed to update id mapping: {e}"),
        })?;

        *self.count.lock().unwrap() += 1;
        Ok(())
    }

    async fn upsert(&mut self, item: StoredEmbedding) -> EmbeddingResult<()> {
        // Try to delete first, then insert
        let _ = self.delete(&item.id).await;
        self.insert(item).await
    }

    async fn get(&self, id: &str) -> EmbeddingResult<Option<StoredEmbedding>> {
        let conn = self.conn.lock().unwrap();

        let result: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT e.id, e.metadata, m.rowid
                 FROM embeddings e
                 JOIN id_rowid_map m ON e.id = m.id
                 WHERE e.id = ?",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        match result {
            Some((id, metadata_json, rowid)) => {
                // Get the embedding vector
                let embedding_bytes: Vec<u8> = conn
                    .query_row(
                        "SELECT embedding FROM vec_embeddings WHERE rowid = ?",
                        [rowid],
                        |row| row.get(0),
                    )
                    .map_err(|e| EmbeddingError::Storage {
                        message: format!("Failed to get embedding vector: {e}"),
                    })?;

                let embedding: Vec<f32> = embedding_bytes
                    .chunks(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let metadata: HashMap<String, serde_json::Value> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();

                Ok(Some(StoredEmbedding::with_metadata(
                    id, embedding, metadata,
                )))
            }
            None => Ok(None),
        }
    }

    async fn delete(&mut self, id: &str) -> EmbeddingResult<bool> {
        let conn = self.conn.lock().unwrap();

        // Get the rowid first
        let rowid: Option<i64> = conn
            .query_row("SELECT rowid FROM id_rowid_map WHERE id = ?", [id], |row| {
                row.get(0)
            })
            .ok();

        if let Some(rowid) = rowid {
            conn.execute("DELETE FROM embeddings WHERE id = ?", [id])
                .ok();
            conn.execute("DELETE FROM vec_embeddings WHERE rowid = ?", [rowid])
                .ok();
            conn.execute("DELETE FROM id_rowid_map WHERE id = ?", [id])
                .ok();

            let mut count = self.count.lock().unwrap();
            *count = count.saturating_sub(1);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn search(
        &self,
        query: &[f32],
        options: SearchOptions,
    ) -> EmbeddingResult<Vec<SearchResult>> {
        if query.len() != self.dimensions {
            return Err(EmbeddingError::DimensionMismatch {
                expected: self.dimensions,
                actual: query.len(),
            });
        }

        let conn = self.conn.lock().unwrap();

        // Normalize query for cosine similarity
        let query_vec = if self.metric == DistanceMetric::Cosine {
            Self::normalize(query)
        } else {
            query.to_vec()
        };

        let query_bytes = Self::vec_to_bytes(&query_vec);

        // Use sqlite-vec's KNN search
        // Note: sqlite-vec requires k = ? in WHERE clause, not just LIMIT
        let mut stmt = conn
            .prepare(
                "SELECT v.rowid, v.distance, m.id, e.metadata
                 FROM vec_embeddings v
                 JOIN id_rowid_map m ON v.rowid = m.rowid
                 JOIN embeddings e ON m.id = e.id
                 WHERE v.embedding MATCH ? AND k = ?
                 ORDER BY v.distance",
            )
            .map_err(|e| EmbeddingError::Storage {
                message: format!("Failed to prepare search query: {e}"),
            })?;

        let mut results = Vec::new();
        let rows = stmt
            .query_map(params![&query_bytes, options.k], |row| {
                let _rowid: i64 = row.get(0)?;
                let distance: f64 = row.get(1)?;
                let id: String = row.get(2)?;
                let metadata_json: String = row.get(3)?;
                Ok((id, distance, metadata_json))
            })
            .map_err(|e| EmbeddingError::Storage {
                message: format!("Failed to execute search: {e}"),
            })?;

        for row in rows {
            let row_result: Result<(String, f64, String), rusqlite::Error> = row;
            let (id, distance, metadata_json) =
                row_result.map_err(|e| EmbeddingError::Storage {
                    message: format!("Failed to read search result: {e}"),
                })?;

            // Convert distance to similarity score
            // sqlite-vec returns L2 distance, convert based on metric
            let score = match self.metric {
                DistanceMetric::Cosine => {
                    // For normalized vectors, L2 distance relates to cosine: cos = 1 - (d^2 / 2)
                    1.0 - (distance as f32 * distance as f32 / 2.0)
                }
                DistanceMetric::Euclidean => -(distance as f32), // Negate so higher is better
                DistanceMetric::DotProduct => -(distance as f32),
            };

            // Apply threshold filter
            if let Some(threshold) = options.threshold {
                match self.metric {
                    DistanceMetric::Euclidean => {
                        if -score > threshold {
                            continue;
                        }
                    }
                    _ => {
                        if score < threshold {
                            continue;
                        }
                    }
                }
            }

            let mut result = SearchResult::new(id, score);

            if options.include_metadata {
                let metadata: HashMap<String, serde_json::Value> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();
                result = result.with_metadata(metadata);
            }

            results.push(result);
        }

        Ok(results)
    }

    fn len(&self) -> usize {
        *self.count.lock().unwrap()
    }

    async fn clear(&mut self) {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM embeddings", []).ok();
        conn.execute("DELETE FROM vec_embeddings", []).ok();
        conn.execute("DELETE FROM id_rowid_map", []).ok();
        *self.count.lock().unwrap() = 0;
    }

    fn metric(&self) -> DistanceMetric {
        self.metric
    }

    fn dimensions(&self) -> Option<usize> {
        Some(self.dimensions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_store_basic() {
        let mut store = SqliteVecStore::in_memory(3).unwrap();

        // Insert
        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("doc2", vec![0.9, 0.1, 0.0]))
            .await
            .unwrap();
        store
            .insert(StoredEmbedding::new("doc3", vec![0.0, 1.0, 0.0]))
            .await
            .unwrap();

        assert_eq!(store.len(), 3);

        // Get
        let doc1 = store.get("doc1").await.unwrap();
        assert!(doc1.is_some());
        assert_eq!(doc1.unwrap().id, "doc1");

        // Search
        let results = store
            .search(&[1.0, 0.0, 0.0], SearchOptions::top_k(2))
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc1");

        // Delete
        assert!(store.delete("doc1").await.unwrap());
        assert_eq!(store.len(), 2);
        assert!(store.get("doc1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_sqlite_store_metadata() {
        let mut store = SqliteVecStore::in_memory(3).unwrap();

        let mut embedding = StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]);
        embedding.insert_metadata("source", "test");
        embedding.insert_metadata("page", 42);

        store.insert(embedding).await.unwrap();

        let retrieved = store.get("doc1").await.unwrap().unwrap();
        assert_eq!(
            retrieved.get_metadata("source"),
            Some(&serde_json::Value::String("test".into()))
        );
    }

    #[tokio::test]
    async fn test_sqlite_store_upsert() {
        let mut store = SqliteVecStore::in_memory(3).unwrap();

        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();

        // Upsert should update
        store
            .upsert(StoredEmbedding::new("doc1", vec![0.0, 1.0, 0.0]))
            .await
            .unwrap();

        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn test_sqlite_store_dimension_validation() {
        let mut store = SqliteVecStore::in_memory(3).unwrap();

        let result = store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0])) // Wrong dimensions
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sqlite_store_duplicate_id() {
        let mut store = SqliteVecStore::in_memory(3).unwrap();

        // First insert should succeed
        store
            .insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0]))
            .await
            .unwrap();

        // Second insert with same ID should fail with DuplicateId error
        let result = store
            .insert(StoredEmbedding::new("doc1", vec![0.0, 1.0, 0.0]))
            .await;

        assert!(matches!(result, Err(EmbeddingError::DuplicateId { .. })));
    }
}
