//! Vector storage and similarity search for mcpkit-forge.
//!
//! `mcpkit-embedding` provides abstractions for storing, indexing, and searching
//! embedding vectors. It enables semantic search, similarity matching, and
//! retrieval-augmented generation (RAG) workflows.
//!
//! # Features
//!
//! - **`VectorStore` trait**: Common interface for vector storage backends
//! - **`InMemoryStore`**: Fast in-memory implementation for development and small datasets
//! - **`SqliteVecStore`**: Persistent `SQLite` storage with sqlite-vec extension (feature: `sqlite`)
//! - **`PgVectorStore`**: Production-ready `PostgreSQL` storage with pgvector (feature: `postgres`)
//! - **Distance metrics**: Cosine, Euclidean, and Dot Product similarity
//! - **Metadata support**: Associate JSON metadata with embeddings for filtering
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use mcpkit_embedding::{
//!     InMemoryStore, VectorStore, StoredEmbedding, SearchOptions, DistanceMetric,
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create an in-memory store with cosine similarity
//!     let mut store = InMemoryStore::new();
//!
//!     // Insert some embeddings (typically from an LLM provider)
//!     store.insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0])).await?;
//!     store.insert(StoredEmbedding::new("doc2", vec![0.9, 0.1, 0.0])).await?;
//!     store.insert(StoredEmbedding::new("doc3", vec![0.0, 1.0, 0.0])).await?;
//!
//!     // Search for similar vectors
//!     let query = vec![1.0, 0.0, 0.0];
//!     let results = store.search(&query, SearchOptions::top_k(2)).await?;
//!
//!     println!("Most similar: {} (score: {:.4})", results[0].id, results[0].score);
//!     Ok(())
//! }
//! ```
//!
//! # With LLM Provider
//!
//! Combine with `mcpkit-provider` to generate embeddings:
//!
//! ```rust,ignore
//! use mcpkit_embedding::{InMemoryStore, VectorStore, StoredEmbedding, SearchOptions};
//! use mcpkit_provider::{openai::OpenAiProvider, Provider, EmbeddingRequest};
//!
//! async fn semantic_search(query: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
//!     let provider = OpenAiProvider::new(std::env::var("OPENAI_API_KEY")?)?;
//!     let mut store = InMemoryStore::new();
//!
//!     // Index some documents
//!     let docs = vec!["Rust programming", "Machine learning", "Vector databases"];
//!     let embeddings = provider.embed(EmbeddingRequest::batch(
//!         docs.iter().map(|s| s.to_string()).collect()
//!     )).await?;
//!
//!     for (doc, emb) in docs.iter().zip(embeddings.embeddings.iter()) {
//!         store.insert(StoredEmbedding::new(*doc, emb.embedding.clone())).await?;
//!     }
//!
//!     // Search
//!     let query_emb = provider.embed(EmbeddingRequest::new(query)).await?;
//!     let results = store.search(
//!         &query_emb.embeddings[0].embedding,
//!         SearchOptions::top_k(3)
//!     ).await?;
//!
//!     Ok(results.into_iter().map(|r| r.id).collect())
//! }
//! ```
//!
//! # Distance Metrics
//!
//! Choose the right metric for your use case:
//!
//! - **Cosine** (default): Best for semantic similarity, ignores magnitude
//! - **Euclidean**: Best for clustering, considers absolute distances
//! - **`DotProduct`**: Fast alternative for normalized vectors
//!
//! ```rust,ignore
//! use mcpkit_embedding::{InMemoryStore, DistanceMetric};
//!
//! // Use Euclidean distance for clustering
//! let store = InMemoryStore::with_metric(DistanceMetric::Euclidean);
//!
//! // Or Dot Product for normalized vectors
//! let store = InMemoryStore::with_metric(DistanceMetric::DotProduct);
//! ```

#![warn(missing_docs)]

mod distance;
mod error;
mod memory_store;
mod store;

#[cfg(feature = "sqlite")]
mod sqlite_store;

#[cfg(feature = "postgres")]
mod postgres_store;

// Re-exports
pub use distance::DistanceMetric;
pub use error::{EmbeddingError, EmbeddingResult};
pub use memory_store::InMemoryStore;
pub use store::{SearchOptions, SearchResult, StoredEmbedding, VectorStore};

#[cfg(feature = "sqlite")]
pub use sqlite_store::SqliteVecStore;

#[cfg(feature = "postgres")]
pub use postgres_store::PgVectorStore;
