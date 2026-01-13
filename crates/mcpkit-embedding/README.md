# mcpkit-embedding

Vector storage and similarity search for the mcpkit-forge orchestration layer.

## Features

- **`VectorStore` trait**: Common interface for vector storage backends
- **`InMemoryStore`**: Fast in-memory implementation for development and small datasets
- **`SqliteVecStore`**: Persistent SQLite storage with sqlite-vec extension (feature: `sqlite`)
- **`PgVectorStore`**: Production-ready PostgreSQL storage with pgvector (feature: `postgres`)
- **Distance metrics**: Cosine, Euclidean, and Dot Product similarity
- **Metadata support**: Associate JSON metadata with embeddings for filtering

## Quick Start

```rust
use mcpkit_embedding::{
    InMemoryStore, VectorStore, StoredEmbedding, SearchOptions, DistanceMetric,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an in-memory store with cosine similarity
    let mut store = InMemoryStore::new();

    // Insert some embeddings (typically from an LLM provider)
    store.insert(StoredEmbedding::new("doc1", vec![1.0, 0.0, 0.0])).await?;
    store.insert(StoredEmbedding::new("doc2", vec![0.9, 0.1, 0.0])).await?;
    store.insert(StoredEmbedding::new("doc3", vec![0.0, 1.0, 0.0])).await?;

    // Search for similar vectors
    let query = vec![1.0, 0.0, 0.0];
    let results = store.search(&query, SearchOptions::top_k(2)).await?;

    println!("Most similar: {} (score: {:.4})", results[0].id, results[0].score);
    Ok(())
}
```

## Storage Backends

### InMemoryStore (default)

Fast, ephemeral storage for development and small datasets:

```rust
use mcpkit_embedding::{InMemoryStore, DistanceMetric};

// Default cosine similarity
let store = InMemoryStore::new();

// Or with custom metric
let store = InMemoryStore::with_metric(DistanceMetric::Euclidean);
```

### SqliteVecStore (feature: `sqlite`)

Persistent storage using SQLite with the sqlite-vec extension:

```rust
use mcpkit_embedding::{SqliteVecStore, VectorStore, StoredEmbedding, SearchOptions};

// Open or create a database
let mut store = SqliteVecStore::open("vectors.db", 384)?;

// Insert embeddings
store.insert(StoredEmbedding::new("doc1", vec![0.1; 384])).await?;

// Search
let results = store.search(&query_vec, SearchOptions::top_k(5)).await?;

// In-memory for testing
let store = SqliteVecStore::in_memory(384)?;
```

### PgVectorStore (feature: `postgres`)

Production-ready PostgreSQL storage with pgvector extension:

```rust
use mcpkit_embedding::{PgVectorStore, VectorStore, StoredEmbedding, SearchOptions};
use sqlx::PgPool;

// Connect to PostgreSQL
let pool = PgPool::connect("postgres://user:pass@localhost/db").await?;
let mut store = PgVectorStore::new(pool, 384).await?;

// Insert embeddings
store.insert(StoredEmbedding::new("doc1", vec![0.1; 384])).await?;

// Search with metadata
let results = store.search(&query_vec, SearchOptions::top_k(5).include_metadata()).await?;
```

**Prerequisites**: PostgreSQL must have the pgvector extension installed:
```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

## Distance Metrics

Choose the right metric for your use case:

- **Cosine** (default): Best for semantic similarity, ignores magnitude
- **Euclidean**: Best for clustering, considers absolute distances
- **DotProduct**: Fast alternative for normalized vectors

## Feature Flags

```toml
[dependencies]
mcpkit-embedding = { version = "0.5", features = ["sqlite", "postgres"] }
```

| Feature | Description |
|---------|-------------|
| `default` | In-memory store with tokio runtime |
| `sqlite` | SQLite storage with sqlite-vec |
| `postgres` | PostgreSQL storage with pgvector |

## License

Licensed under the MIT License.
