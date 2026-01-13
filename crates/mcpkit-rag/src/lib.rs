//! Retrieval-Augmented Generation (RAG) components for mcpkit-forge.
//!
//! `mcpkit-rag` provides the building blocks for creating RAG pipelines:
//! loading documents, splitting them into chunks, indexing for retrieval,
//! and generating answers using retrieved context.
//!
//! # Features
//!
//! - **Document loaders**: Load from files, directories, JSON, or memory
//! - **Text splitters**: Recursive, fixed-size, token-based, sentence-based
//! - **Retrievers**: Vector store retrieval, multi-query, filtering
//! - **RAG Pipeline**: Complete end-to-end workflow
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use mcpkit_rag::{RagPipeline, PipelineConfig, MemoryLoader, RecursiveCharacterSplitter};
//! use mcpkit_embedding::InMemoryStore;
//! use mcpkit_provider::openai::OpenAiProvider;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create components
//!     let provider = OpenAiProvider::new(std::env::var("OPENAI_API_KEY")?)?;
//!     let store = InMemoryStore::new();
//!     let splitter = RecursiveCharacterSplitter::new()
//!         .chunk_size(500)
//!         .chunk_overlap(50);
//!
//!     // Build pipeline
//!     let mut pipeline = RagPipeline::new(store, provider, splitter)
//!         .config(PipelineConfig::new().k(5));
//!
//!     // Add documents
//!     pipeline.add_loader(MemoryLoader::from_texts(vec![
//!         "Rust is a systems programming language...",
//!         "The borrow checker ensures memory safety...",
//!     ]));
//!     pipeline.ingest().await?;
//!
//!     // Query
//!     let response = pipeline.query("What is the borrow checker?").await?;
//!     println!("{}", response.answer);
//!     println!("\n{}", response.with_citations());
//!
//!     Ok(())
//! }
//! ```
//!
//! # Document Loaders
//!
//! Load documents from various sources:
//!
//! ```rust,ignore
//! use mcpkit_rag::{TextLoader, DirectoryLoader, JsonLoader, MemoryLoader};
//!
//! // Single text file
//! let loader = TextLoader::new("document.txt");
//!
//! // Directory of files
//! let loader = DirectoryLoader::new("docs/")
//!     .with_extensions(vec!["md".into(), "txt".into()])
//!     .recursive(true);
//!
//! // JSON file with documents
//! let loader = JsonLoader::new("data.json")
//!     .content_key("text")
//!     .metadata_key("meta");
//!
//! // In-memory documents
//! let loader = MemoryLoader::from_texts(vec!["doc1", "doc2"]);
//! ```
//!
//! # Text Splitters
//!
//! Split documents into appropriately-sized chunks:
//!
//! ```rust
//! use mcpkit_rag::{
//!     RecursiveCharacterSplitter, FixedSizeSplitter,
//!     TokenSplitter, SentenceSplitter, TextSplitter, Document,
//! };
//!
//! // Recursive: splits at natural boundaries
//! let splitter = RecursiveCharacterSplitter::new()
//!     .chunk_size(1000)
//!     .chunk_overlap(200);
//!
//! // Fixed size: simple character-based
//! let splitter = FixedSizeSplitter::new(500).with_overlap(50);
//!
//! // Token-based: estimates token count
//! let splitter = TokenSplitter::new(256).with_overlap(32);
//!
//! // Sentence-based: preserves sentence boundaries
//! let splitter = SentenceSplitter::new().max_sentences(5);
//! ```
//!
//! # Retrievers
//!
//! Retrieve relevant documents for queries:
//!
//! ```rust,ignore
//! use mcpkit_rag::{VectorStoreRetriever, MultiQueryRetriever, FilterRetriever, Retriever};
//!
//! // Basic vector store retrieval
//! let retriever = VectorStoreRetriever::new(store, provider)
//!     .model("text-embedding-3-small");
//!
//! // Multi-query for better recall
//! let retriever = MultiQueryRetriever::new(base_retriever, provider)
//!     .num_queries(3);
//!
//! // Filter by metadata
//! let retriever = FilterRetriever::new(base, |doc| {
//!     doc.get_metadata("type") == Some(&serde_json::json!("technical"))
//! });
//!
//! let results = retriever.retrieve("search query", 5).await?;
//! ```
//!
//! # Chunking Best Practices
//!
//! Chunking strategy significantly affects retrieval quality:
//!
//! - **Chunk size**: Balance between context and specificity (500-1000 chars typical)
//! - **Overlap**: 10-20% of chunk size helps maintain context across boundaries
//! - **Semantic splits**: `RecursiveCharacterSplitter` is usually best for general text
//! - **Match query size**: Chunks similar in length to expected queries work better

#![warn(missing_docs)]

mod document;
mod error;
mod loader;
mod pipeline;
mod retriever;
mod splitter;

// Re-exports
pub use document::{Document, RetrievedDocument};
pub use error::{RagError, RagResult};
pub use loader::{DirectoryLoader, DocumentLoader, JsonLoader, MemoryLoader, TextLoader};
pub use pipeline::{PipelineConfig, RagPipeline, RagResponse};
pub use retriever::{FilterRetriever, MultiQueryRetriever, Retriever, VectorStoreRetriever};
pub use splitter::{
    FixedSizeSplitter, RecursiveCharacterSplitter, SentenceSplitter, TextSplitter, TokenSplitter,
};
