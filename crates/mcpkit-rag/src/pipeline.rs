//! RAG pipeline for end-to-end retrieval-augmented generation.
//!
//! A pipeline combines document loading, splitting, indexing, retrieval,
//! and generation into a cohesive workflow.

use std::sync::Arc;

use mcpkit_embedding::VectorStore;
use mcpkit_provider::{CompletionRequest, Message, Provider};

use crate::document::{Document, RetrievedDocument};
use crate::error::RagResult;
use crate::loader::DocumentLoader;
use crate::retriever::{Retriever, VectorStoreRetriever};
use crate::splitter::TextSplitter;

/// Configuration for a RAG pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Number of documents to retrieve.
    pub k: usize,
    /// Minimum similarity threshold for retrieval.
    pub threshold: Option<f32>,
    /// Whether to include source citations in the response.
    pub include_sources: bool,
    /// Model to use for generation.
    pub model: Option<String>,
    /// Temperature for generation.
    pub temperature: Option<f32>,
    /// System prompt for generation.
    pub system_prompt: Option<String>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            k: 5,
            threshold: None,
            include_sources: true,
            model: None,
            temperature: None,
            system_prompt: None,
        }
    }
}

impl PipelineConfig {
    /// Create a new configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the number of documents to retrieve.
    #[must_use]
    pub fn k(mut self, k: usize) -> Self {
        self.k = k;
        self
    }

    /// Set the similarity threshold.
    #[must_use]
    pub fn threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Set whether to include sources.
    #[must_use]
    pub fn include_sources(mut self, include: bool) -> Self {
        self.include_sources = include;
        self
    }

    /// Set the model.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the temperature.
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the system prompt.
    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

/// The result of a RAG query.
#[derive(Debug, Clone)]
pub struct RagResponse {
    /// The generated answer.
    pub answer: String,
    /// The documents used to generate the answer.
    pub sources: Vec<RetrievedDocument>,
}

impl RagResponse {
    /// Create a new RAG response.
    #[must_use]
    pub fn new(answer: impl Into<String>, sources: Vec<RetrievedDocument>) -> Self {
        Self {
            answer: answer.into(),
            sources,
        }
    }

    /// Get the answer.
    #[must_use]
    pub fn answer(&self) -> &str {
        &self.answer
    }

    /// Get the number of sources.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Format the answer with sources as citations.
    #[must_use]
    pub fn with_citations(&self) -> String {
        let mut result = self.answer.clone();

        if !self.sources.is_empty() {
            result.push_str("\n\nSources:\n");
            for (i, source) in self.sources.iter().enumerate() {
                let id = source.id().unwrap_or("unknown");
                result.push_str(&format!(
                    "[{}] {} (score: {:.3})\n",
                    i + 1,
                    id,
                    source.score
                ));
            }
        }

        result
    }
}

/// A complete RAG pipeline.
///
/// The pipeline handles the full RAG workflow:
/// 1. Load documents from sources
/// 2. Split into chunks
/// 3. Index chunks for retrieval
/// 4. Retrieve relevant chunks for queries
/// 5. Generate answers using retrieved context
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::{RagPipeline, PipelineConfig, TextLoader, RecursiveCharacterSplitter};
/// use mcpkit_embedding::InMemoryStore;
/// use mcpkit_provider::openai::OpenAiProvider;
///
/// let provider = OpenAiProvider::new(api_key)?;
/// let store = InMemoryStore::new();
/// let splitter = RecursiveCharacterSplitter::new().chunk_size(500);
///
/// let mut pipeline = RagPipeline::new(store, provider, splitter)
///     .config(PipelineConfig::new().k(5));
///
/// // Index documents
/// pipeline.add_loader(TextLoader::new("docs/manual.txt"));
/// pipeline.ingest().await?;
///
/// // Query
/// let response = pipeline.query("How do I configure the system?").await?;
/// println!("{}", response.answer);
/// ```
pub struct RagPipeline<S, P, T>
where
    S: VectorStore,
    P: Provider,
    T: TextSplitter,
{
    retriever: VectorStoreRetriever<S, P>,
    provider: Arc<P>,
    splitter: T,
    loaders: Vec<Box<dyn DocumentLoader>>,
    config: PipelineConfig,
}

impl<S, P, T> RagPipeline<S, P, T>
where
    S: VectorStore + 'static,
    P: Provider + 'static,
    T: TextSplitter,
{
    /// Create a new RAG pipeline.
    pub fn new(store: S, provider: P, splitter: T) -> Self {
        let provider = Arc::new(provider);
        let retriever = VectorStoreRetriever::from_arcs(
            Arc::new(tokio::sync::RwLock::new(store)),
            Arc::clone(&provider),
        );

        Self {
            retriever,
            provider,
            splitter,
            loaders: Vec::new(),
            config: PipelineConfig::default(),
        }
    }

    /// Set the pipeline configuration.
    #[must_use]
    pub fn config(mut self, config: PipelineConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the embedding model.
    #[must_use]
    pub fn embedding_model(mut self, model: impl Into<String>) -> Self {
        self.retriever = self.retriever.model(model);
        self
    }

    /// Add a document loader.
    pub fn add_loader(&mut self, loader: impl DocumentLoader + 'static) {
        self.loaders.push(Box::new(loader));
    }

    /// Add documents directly.
    pub async fn add_documents(&self, documents: Vec<Document>) -> RagResult<usize> {
        let chunks = self.splitter.split_documents(&documents);
        let count = chunks.len();
        self.retriever.index(&chunks).await?;
        Ok(count)
    }

    /// Ingest documents from all configured loaders.
    ///
    /// Loads documents from each loader, splits them into chunks,
    /// and indexes them for retrieval.
    pub async fn ingest(&self) -> RagResult<usize> {
        let mut total_chunks = 0;

        for loader in &self.loaders {
            tracing::info!(loader = %loader.description(), "Loading documents");
            let documents = loader.load().await?;

            tracing::info!(count = documents.len(), "Loaded documents");

            let chunks = self.splitter.split_documents(&documents);
            tracing::info!(
                count = chunks.len(),
                splitter = %self.splitter.description(),
                "Split into chunks"
            );

            self.retriever.index(&chunks).await?;
            total_chunks += chunks.len();
        }

        tracing::info!(total = total_chunks, "Ingestion complete");
        Ok(total_chunks)
    }

    /// Retrieve relevant documents for a query.
    pub async fn retrieve(&self, query: &str) -> RagResult<Vec<RetrievedDocument>> {
        let results = if let Some(threshold) = self.config.threshold {
            self.retriever
                .retrieve_with_threshold(query, self.config.k, threshold)
                .await?
        } else {
            self.retriever.retrieve(query, self.config.k).await?
        };

        Ok(results)
    }

    /// Query the pipeline and generate an answer.
    ///
    /// This retrieves relevant documents and uses them as context
    /// to generate an answer to the query.
    pub async fn query(&self, query: &str) -> RagResult<RagResponse> {
        // Retrieve relevant documents
        let sources = self.retrieve(query).await?;

        // Build context from sources
        let context = sources
            .iter()
            .enumerate()
            .map(|(i, doc)| format!("[{}] {}", i + 1, doc.content()))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Build the prompt
        let system = self.config.system_prompt.clone().unwrap_or_else(|| {
            "You are a helpful assistant. Answer questions based on the provided context. \
             If the context doesn't contain relevant information, say so."
                .to_string()
        });

        let user_prompt = format!(
            "Context:\n{context}\n\nQuestion: {query}\n\nAnswer based on the context above."
        );

        // Generate the answer
        let mut request = CompletionRequest::new()
            .message(Message::system(system))
            .message(Message::user(user_prompt));

        if let Some(model) = &self.config.model {
            request = request.model(model.clone());
        }

        if let Some(temp) = self.config.temperature {
            request = request.temperature(temp);
        }

        let response = self.provider.complete(request).await?;
        let answer = response.text().unwrap_or_default();

        Ok(RagResponse::new(answer, sources))
    }

    /// Clear all indexed documents.
    pub async fn clear(&self) {
        self.retriever.clear().await;
    }

    /// Get the number of indexed chunks.
    pub async fn chunk_count(&self) -> usize {
        self.retriever.len().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::splitter::FixedSizeSplitter;
    use mcpkit_embedding::InMemoryStore;
    use mcpkit_provider::streaming::CompletionStream;
    use mcpkit_provider::{
        CompletionRequest, CompletionResponse, ContentBlock, Embedding, EmbeddingRequest,
        EmbeddingResponse, EmbeddingUsage, FinishReason, ModelInfo, ProviderCapabilities,
        ProviderError, ProviderInfo, Usage,
    };

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

    #[async_trait::async_trait]
    impl Provider for MockProvider {
        fn info(&self) -> &ProviderInfo {
            &self.info
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse {
                id: "mock-completion".to_string(),
                model: "mock-model".to_string(),
                content: vec![ContentBlock::text(
                    "This is the generated answer based on the context.",
                )],
                finish_reason: FinishReason::Stop,
                usage: Usage::with_tokens(10, 20),
            })
        }

        async fn complete_stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionStream, ProviderError> {
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

        async fn embed(
            &self,
            request: EmbeddingRequest,
        ) -> Result<EmbeddingResponse, ProviderError> {
            let embeddings: Vec<Embedding> = request
                .input
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let mut vec = vec![0.0; 8];
                    vec[i % 8] = 1.0;
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
    async fn test_pipeline_config() {
        let config = PipelineConfig::new()
            .k(10)
            .threshold(0.5)
            .model("gpt-4")
            .temperature(0.7)
            .include_sources(true);

        assert_eq!(config.k, 10);
        assert_eq!(config.threshold, Some(0.5));
        assert_eq!(config.model, Some("gpt-4".to_string()));
    }

    #[tokio::test]
    async fn test_rag_response() {
        let doc = Document::new("Test content").with_id("doc-1");
        let sources = vec![RetrievedDocument::new(doc, 0.95)];
        let response = RagResponse::new("Test answer", sources);

        assert_eq!(response.answer(), "Test answer");
        assert_eq!(response.source_count(), 1);

        let with_citations = response.with_citations();
        assert!(with_citations.contains("Test answer"));
        assert!(with_citations.contains("doc-1"));
    }

    #[tokio::test]
    async fn test_pipeline_add_documents() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();
        let splitter = FixedSizeSplitter::new(50);

        let pipeline = RagPipeline::new(store, provider, splitter);

        let docs = vec![
            Document::new("This is document one with some content."),
            Document::new("This is document two with different content."),
        ];

        let count = pipeline.add_documents(docs).await.unwrap();
        assert!(count >= 2);
        assert!(pipeline.chunk_count().await >= 2);
    }

    #[tokio::test]
    async fn test_pipeline_query() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();
        let splitter = FixedSizeSplitter::new(100);

        let pipeline =
            RagPipeline::new(store, provider, splitter).config(PipelineConfig::new().k(3));

        let docs = vec![
            Document::new("Rust is a systems programming language.").with_id("rust-doc"),
            Document::new("Python is great for data science.").with_id("python-doc"),
        ];

        pipeline.add_documents(docs).await.unwrap();

        let response = pipeline.query("What is Rust?").await.unwrap();
        assert!(!response.answer.is_empty());
        assert!(!response.sources.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_clear() {
        let store = InMemoryStore::new();
        let provider = MockProvider::new();
        let splitter = FixedSizeSplitter::new(50);

        let pipeline = RagPipeline::new(store, provider, splitter);

        let docs = vec![Document::new("Test document")];
        pipeline.add_documents(docs).await.unwrap();
        assert!(pipeline.chunk_count().await > 0);

        pipeline.clear().await;
        assert_eq!(pipeline.chunk_count().await, 0);
    }
}
