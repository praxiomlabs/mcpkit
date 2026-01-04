//! Retrievers for finding relevant documents.
//!
//! Retrievers fetch documents relevant to a query from a vector store or
//! other source. They are the core component that enables semantic search
//! in RAG pipelines.

use std::sync::Arc;

use async_trait::async_trait;

use mcpkit_embedding::{SearchOptions, StoredEmbedding, VectorStore};
use mcpkit_provider::{EmbeddingRequest, Provider};

use crate::document::{Document, RetrievedDocument};
use crate::error::{RagError, RagResult};

/// Trait for retrieving documents relevant to a query.
///
/// Retrievers take a text query and return the most relevant documents
/// from a collection. They typically use embeddings and vector similarity
/// for semantic search.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::{Retriever, VectorStoreRetriever};
///
/// let retriever = VectorStoreRetriever::new(store, provider);
/// let results = retriever.retrieve("What is Rust?", 5).await?;
///
/// for doc in results {
///     println!("Score: {:.3}, Content: {}", doc.score, doc.content());
/// }
/// ```
#[async_trait]
pub trait Retriever: Send + Sync {
    /// Retrieve documents relevant to the query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query
    /// * `k` - Number of documents to retrieve
    ///
    /// # Returns
    ///
    /// Vector of retrieved documents, ordered by relevance (most relevant first).
    async fn retrieve(&self, query: &str, k: usize) -> RagResult<Vec<RetrievedDocument>>;

    /// Retrieve with a minimum score threshold.
    async fn retrieve_with_threshold(
        &self,
        query: &str,
        k: usize,
        threshold: f32,
    ) -> RagResult<Vec<RetrievedDocument>> {
        let results = self.retrieve(query, k).await?;
        Ok(results.into_iter().filter(|r| r.score >= threshold).collect())
    }

    /// Get a description of the retriever.
    fn description(&self) -> String {
        "Retriever".to_string()
    }
}

/// A retriever backed by a vector store.
///
/// This is the most common retriever type. It embeds the query using
/// a provider and searches a vector store for similar documents.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::VectorStoreRetriever;
/// use mcpkit_embedding::InMemoryStore;
/// use mcpkit_provider::openai::OpenAiProvider;
///
/// let store = InMemoryStore::new();
/// let provider = OpenAiProvider::new(api_key)?;
///
/// let retriever = VectorStoreRetriever::new(store, provider)
///     .model("text-embedding-3-small");
///
/// let results = retriever.retrieve("semantic search query", 5).await?;
/// ```
pub struct VectorStoreRetriever<S, P>
where
    S: VectorStore,
    P: Provider,
{
    store: Arc<tokio::sync::RwLock<S>>,
    provider: Arc<P>,
    model: Option<String>,
}

impl<S, P> VectorStoreRetriever<S, P>
where
    S: VectorStore + 'static,
    P: Provider + 'static,
{
    /// Create a new vector store retriever.
    pub fn new(store: S, provider: P) -> Self {
        Self {
            store: Arc::new(tokio::sync::RwLock::new(store)),
            provider: Arc::new(provider),
            model: None,
        }
    }

    /// Create from Arc'd components.
    pub fn from_arcs(store: Arc<tokio::sync::RwLock<S>>, provider: Arc<P>) -> Self {
        Self {
            store,
            provider,
            model: None,
        }
    }

    /// Set the embedding model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Get a reference to the underlying store.
    #[must_use] 
    pub fn store(&self) -> &Arc<tokio::sync::RwLock<S>> {
        &self.store
    }

    /// Get a reference to the provider.
    #[must_use] 
    pub fn provider(&self) -> &Arc<P> {
        &self.provider
    }

    /// Index documents into the store.
    ///
    /// This embeds the documents and stores them for later retrieval.
    pub async fn index(&self, documents: &[Document]) -> RagResult<()> {
        if documents.is_empty() {
            return Ok(());
        }

        // Collect contents for batch embedding
        let contents: Vec<String> = documents.iter().map(|d| d.content.clone()).collect();

        // Create embedding request
        let mut request = EmbeddingRequest::batch(contents);
        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        // Get embeddings
        let response = self.provider.embed(request).await?;

        // Store embeddings with document content and metadata
        let mut store = self.store.write().await;
        for (doc, emb) in documents.iter().zip(response.embeddings.iter()) {
            let id = doc.id_or_default();
            let mut metadata = doc.metadata.clone();
            metadata.insert("content".to_string(), serde_json::json!(doc.content));

            let stored = StoredEmbedding::with_metadata(id, emb.embedding.clone(), metadata);
            store.upsert(stored).await?;
        }

        Ok(())
    }

    /// Clear all indexed documents.
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear().await;
    }

    /// Get the number of indexed documents.
    pub async fn len(&self) -> usize {
        let store = self.store.read().await;
        store.len()
    }

    /// Check if the store is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
impl<S, P> Retriever for VectorStoreRetriever<S, P>
where
    S: VectorStore + 'static,
    P: Provider + 'static,
{
    async fn retrieve(&self, query: &str, k: usize) -> RagResult<Vec<RetrievedDocument>> {
        // Embed the query
        let mut request = EmbeddingRequest::new(query);
        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        let response = self.provider.embed(request).await?;
        let query_embedding = response
            .embeddings
            .first()
            .ok_or_else(|| RagError::retrieval("No embedding returned for query"))?;

        // Search the store
        let store = self.store.read().await;
        let results = store
            .search(&query_embedding.embedding, SearchOptions::top_k(k))
            .await?;

        // Convert to RetrievedDocuments
        let documents = results
            .into_iter()
            .map(|result| {
                let content = result
                    .metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut metadata = result.metadata;
                metadata.remove("content"); // Don't duplicate content in metadata

                let doc = Document {
                    content,
                    id: Some(result.id),
                    metadata,
                };

                RetrievedDocument::new(doc, result.score)
            })
            .collect();

        Ok(documents)
    }

    fn description(&self) -> String {
        format!(
            "VectorStoreRetriever(model={})",
            self.model.as_deref().unwrap_or("default")
        )
    }
}

/// A multi-query retriever that generates multiple query variations.
///
/// This retriever uses an LLM to generate multiple versions of the query,
/// retrieves results for each, and combines them. This can improve recall
/// for ambiguous or complex queries.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::{MultiQueryRetriever, VectorStoreRetriever};
///
/// let base_retriever = VectorStoreRetriever::new(store, provider.clone());
/// let multi = MultiQueryRetriever::new(base_retriever, provider)
///     .num_queries(3);
///
/// let results = multi.retrieve("What are the benefits of Rust?", 5).await?;
/// ```
pub struct MultiQueryRetriever<R, P>
where
    R: Retriever,
    P: Provider,
{
    retriever: Arc<R>,
    provider: Arc<P>,
    num_queries: usize,
    model: Option<String>,
}

impl<R, P> MultiQueryRetriever<R, P>
where
    R: Retriever + 'static,
    P: Provider + 'static,
{
    /// Create a new multi-query retriever.
    pub fn new(retriever: R, provider: P) -> Self {
        Self {
            retriever: Arc::new(retriever),
            provider: Arc::new(provider),
            num_queries: 3,
            model: None,
        }
    }

    /// Set the number of query variations to generate.
    #[must_use]
    pub fn num_queries(mut self, num: usize) -> Self {
        self.num_queries = num;
        self
    }

    /// Set the model to use for query generation.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Generate query variations using the LLM.
    async fn generate_queries(&self, query: &str) -> RagResult<Vec<String>> {
        let prompt = format!(
            "Generate {} different ways to ask this question. \
             Each version should capture the same intent but use different wording. \
             Output only the questions, one per line, without numbering.\n\n\
             Original question: {}",
            self.num_queries, query
        );

        let mut request =
            mcpkit_provider::CompletionRequest::new().message(mcpkit_provider::Message::user(prompt));

        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        let response = self.provider.complete(request).await?;
        let text = response.text().unwrap_or_default();

        // Parse the generated queries
        let queries: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(std::string::ToString::to_string)
            .take(self.num_queries)
            .collect();

        // Always include the original query
        let mut all_queries = vec![query.to_string()];
        all_queries.extend(queries);

        Ok(all_queries)
    }
}

#[async_trait]
impl<R, P> Retriever for MultiQueryRetriever<R, P>
where
    R: Retriever + 'static,
    P: Provider + 'static,
{
    async fn retrieve(&self, query: &str, k: usize) -> RagResult<Vec<RetrievedDocument>> {
        let queries = self.generate_queries(query).await?;

        // Retrieve for each query
        let mut all_results = Vec::new();
        for q in &queries {
            let results = self.retriever.retrieve(q, k).await?;
            all_results.extend(results);
        }

        // Deduplicate by document ID, keeping highest score
        let mut seen = std::collections::HashMap::new();
        for result in all_results {
            let id = result.document.id_or_default();
            seen.entry(id)
                .and_modify(|existing: &mut RetrievedDocument| {
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert(result);
        }

        // Sort by score and take top k
        let mut results: Vec<_> = seen.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);

        Ok(results)
    }

    fn description(&self) -> String {
        format!("MultiQueryRetriever(num_queries={})", self.num_queries)
    }
}

/// A retriever that wraps another retriever and filters results.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::{FilterRetriever, VectorStoreRetriever};
///
/// let base = VectorStoreRetriever::new(store, provider);
/// let filtered = FilterRetriever::new(base, |doc| {
///     doc.get_metadata("category") == Some(&serde_json::json!("technical"))
/// });
/// ```
pub struct FilterRetriever<R, F>
where
    R: Retriever,
    F: Fn(&Document) -> bool + Send + Sync,
{
    retriever: R,
    filter: F,
}

impl<R, F> FilterRetriever<R, F>
where
    R: Retriever,
    F: Fn(&Document) -> bool + Send + Sync,
{
    /// Create a new filter retriever.
    pub fn new(retriever: R, filter: F) -> Self {
        Self { retriever, filter }
    }
}

#[async_trait]
impl<R, F> Retriever for FilterRetriever<R, F>
where
    R: Retriever + 'static,
    F: Fn(&Document) -> bool + Send + Sync + 'static,
{
    async fn retrieve(&self, query: &str, k: usize) -> RagResult<Vec<RetrievedDocument>> {
        // Retrieve more than k to account for filtering
        let results = self.retriever.retrieve(query, k * 3).await?;

        // Filter and take top k
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| (self.filter)(&r.document))
            .take(k)
            .collect();

        Ok(filtered)
    }

    fn description(&self) -> String {
        format!("FilterRetriever(inner={})", self.retriever.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_embedding::InMemoryStore;
    use mcpkit_provider::{
        CompletionRequest, CompletionResponse, ContentBlock, EmbeddingRequest, EmbeddingResponse,
        Embedding, EmbeddingUsage, FinishReason, ModelInfo, ProviderCapabilities, ProviderError,
        ProviderInfo, Usage,
    };
    use mcpkit_provider::streaming::CompletionStream;

    // Mock provider for testing
    struct MockProvider {
        info: ProviderInfo,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                info: ProviderInfo::new("mock", "Mock Provider")
                    .capabilities(ProviderCapabilities::full()),
            }
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse {
                id: "mock-completion".to_string(),
                model: "mock-model".to_string(),
                content: vec![ContentBlock::text("Query variant 1\nQuery variant 2\nQuery variant 3")],
                finish_reason: FinishReason::Stop,
                usage: Usage::with_tokens(10, 20),
            })
        }

        async fn complete_stream(&self, _request: CompletionRequest) -> Result<CompletionStream, ProviderError> {
            Err(ProviderError::Unsupported {
                provider: "mock".to_string(),
                feature: "streaming".to_string(),
            })
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Ok(vec![ModelInfo::new("mock-model")])
        }

        async fn get_model(&self, model_id: &str) -> Result<ModelInfo, ProviderError> {
            Ok(ModelInfo::new(model_id))
        }

        async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
            // Return simple embeddings based on input
            let embeddings: Vec<Embedding> = request
                .input
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let mut vec = vec![0.0; 3];
                    vec[i % 3] = 1.0;
                    Embedding {
                        index: i,
                        embedding: vec,
                    }
                })
                .collect();

            Ok(EmbeddingResponse {
                model: "mock-embedding".to_string(),
                embeddings,
                usage: EmbeddingUsage {
                    prompt_tokens: request.input.len() as u32 * 10,
                    total_tokens: request.input.len() as u32 * 10,
                },
            })
        }
    }

    #[tokio::test]
    async fn test_vector_store_retriever_index_and_retrieve() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();

        let retriever = VectorStoreRetriever::new(store, provider);

        // Index documents
        let docs = vec![
            Document::new("Document about Rust").with_id("doc-1"),
            Document::new("Document about Python").with_id("doc-2"),
            Document::new("Document about JavaScript").with_id("doc-3"),
        ];

        retriever.index(&docs).await.unwrap();
        assert_eq!(retriever.len().await, 3);

        // Retrieve
        let results = retriever.retrieve("Rust programming", 2).await.unwrap();
        assert!(!results.is_empty());
        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_vector_store_retriever_clear() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();

        let retriever = VectorStoreRetriever::new(store, provider);

        let docs = vec![Document::new("Test document")];
        retriever.index(&docs).await.unwrap();
        assert_eq!(retriever.len().await, 1);

        retriever.clear().await;
        assert!(retriever.is_empty().await);
    }

    #[tokio::test]
    async fn test_filter_retriever() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();

        let base = VectorStoreRetriever::new(store, provider);

        let docs = vec![
            Document::new("Rust doc").with_id("doc-1").with_metadata("lang", "rust"),
            Document::new("Python doc").with_id("doc-2").with_metadata("lang", "python"),
        ];
        base.index(&docs).await.unwrap();

        let filtered = FilterRetriever::new(base, |doc| {
            doc.get_metadata("lang") == Some(&serde_json::json!("rust"))
        });

        let results = filtered.retrieve("programming", 10).await.unwrap();
        for result in &results {
            assert_eq!(
                result.document.get_metadata("lang"),
                Some(&serde_json::json!("rust"))
            );
        }
    }

    #[tokio::test]
    async fn test_retrieve_with_threshold() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();

        let retriever = VectorStoreRetriever::new(store, provider);

        let docs = vec![
            Document::new("High relevance").with_id("doc-1"),
            Document::new("Low relevance").with_id("doc-2"),
        ];
        retriever.index(&docs).await.unwrap();

        // With a high threshold, might get fewer results
        let results = retriever
            .retrieve_with_threshold("query", 10, 0.5)
            .await
            .unwrap();

        for result in &results {
            assert!(result.score >= 0.5);
        }
    }
}
