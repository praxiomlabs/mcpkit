//! # Self-Evolving Knowledge Agent
//!
//! This example demonstrates the most advanced capabilities of mcpkit by combining
//! **all 8 forge crates** with the MCP protocol layer:
//!
//! ## MCP Protocol Layer
//! - **Tools**: Document ingestion, knowledge queries, statistics, evaluation
//! - **Resources**: Knowledge base state, query history, evaluation reports
//! - **Prompts**: Specialized prompts for QA, summarization, evaluation
//! - **Tasks**: Long-running document ingestion with progress tracking
//! - **Stdio Transport**: Real MCP server that accepts connections
//!
//! ## Forge Orchestration Layer (all 8 crates)
//! - **mcpkit-provider**: Multi-LLM abstraction (OpenAI, Anthropic, Ollama)
//! - **mcpkit-template**: Compile-time validated prompt templates
//! - **mcpkit-memory**: Conversation history with token-aware eviction
//! - **mcpkit-embedding**: Vector store and similarity search
//! - **mcpkit-chain**: LCEL-inspired composable pipelines
//! - **mcpkit-agent**: ReAct agent with tool execution
//! - **mcpkit-rag**: Document loading, chunking, retrieval
//! - **mcpkit-eval**: LLM-as-judge metrics (Faithfulness, Relevancy)
//!
//! ## Advanced Patterns
//! - **Feedback Loop**: Low-quality answers trigger regeneration
//! - **Self-Improvement**: Evaluation influences future responses
//! - **Memory-Aware**: Conversation context persisted across queries
//!
//! Run with: `cargo run -p knowledge-agent`
//! Or for stdio mode: `cargo run -p knowledge-agent -- --stdio`

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

// MCP Protocol imports
use mcpkit_core::{
    McpError,
    capability::{ServerCapabilities, ServerInfo},
    types::{
        CallToolResult, Content, GetPromptResult, Prompt, PromptMessage, Resource,
        ResourceContents, Task, TaskId, TaskProgress, TaskStatus, Tool as McpTool, ToolAnnotations,
    },
};

// Forge imports - all 8 crates
use mcpkit_agent::{AgentExecutor, ReActAgent, Tool as AgentTool, ToolOutput, ToolSchema};
use mcpkit_chain::{ChainValue, LlmRunnable, PromptRunnable, Runnable, RunnableRetry};
use mcpkit_embedding::{InMemoryStore, SearchOptions, StoredEmbedding, VectorStore};
use mcpkit_eval::{AnswerRelevancyMetric, FaithfulnessMetric, Metric, TestCase};
use mcpkit_memory::{Memory, TokenMemory};
use mcpkit_provider::streaming::CompletionStream;
use mcpkit_provider::{
    CompletionRequest, CompletionResponse, ContentBlock, Embedding, EmbeddingRequest,
    EmbeddingResponse, EmbeddingUsage, FinishReason, Message, ModelInfo, Provider,
    ProviderCapabilities, ProviderError, ProviderInfo, Usage,
};
use mcpkit_rag::{Document, RecursiveCharacterSplitter, TextSplitter};
use mcpkit_template::Template;

// Transport for stdio mode
use mcpkit_core::error::JsonRpcError;
use mcpkit_core::protocol::{Message as ProtocolMessage, Response};
use mcpkit_transport::Transport;
use mcpkit_transport::stdio::StdioTransport;

// ============================================================================
// PROMPT TEMPLATES (using mcpkit-template with compile-time validation)
// ============================================================================

/// Template for question answering prompts.
/// Uses compile-time validated template from file.
#[derive(Template)]
#[template(path = "templates/qa_prompt.txt")]
struct QAPromptTemplate {
    question: String,
    context: String,
}

/// Template for evaluation prompts.
/// Uses compile-time validated template from file.
#[derive(Template)]
#[template(path = "templates/eval_prompt.txt")]
struct EvalPromptTemplate {
    question: String,
    answer: String,
    context: String,
}

/// Template for synthesis prompts.
/// Uses compile-time validated template from file.
#[derive(Template)]
#[template(path = "templates/synthesis_prompt.txt")]
struct SynthesisPromptTemplate {
    question: String,
    context: String,
}

// ============================================================================
// KNOWLEDGE BASE STATE
// ============================================================================

/// A document stored in the knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source: String,
    pub ingested_at: DateTime<Utc>,
    pub chunk_count: usize,
}

/// A query and its response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRecord {
    pub id: String,
    pub query: String,
    pub response: String,
    pub sources: Vec<String>,
    pub confidence: f64,
    pub evaluation: Option<EvaluationResult>,
    pub timestamp: DateTime<Utc>,
}

/// Evaluation result for a query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub faithfulness: f64,
    pub relevancy: f64,
    pub passed: bool,
    pub feedback: String,
}

/// Task tracking for async operations
#[derive(Debug, Clone)]
pub struct IngestionTask {
    pub id: TaskId,
    pub document_title: String,
    pub status: TaskStatus,
    pub progress: f64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// The central knowledge base state
pub struct KnowledgeBase {
    /// All ingested documents
    documents: HashMap<String, KnowledgeDocument>,
    /// Query history
    queries: Vec<QueryRecord>,
    /// Active ingestion tasks
    tasks: HashMap<TaskId, IngestionTask>,
    /// Vector store for embeddings
    vector_store: InMemoryStore,
    /// Conversation memory (token-aware)
    memory: TokenMemory,
    /// Evaluation statistics
    eval_stats: EvalStats,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalStats {
    pub total_queries: usize,
    pub passed_evaluations: usize,
    pub failed_evaluations: usize,
    pub avg_faithfulness: f64,
    pub avg_relevancy: f64,
    pub regeneration_count: usize,
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
            queries: Vec::new(),
            tasks: HashMap::new(),
            vector_store: InMemoryStore::new(),
            // TokenMemory with system message and 4k token budget
            // The system message is preserved during eviction, ensuring
            // consistent agent behavior as context grows
            memory: TokenMemory::with_system(
                4000,
                "You are a knowledge assistant. Answer questions based on the ingested documents.",
            ),
            eval_stats: EvalStats::default(),
        }
    }

    pub fn add_document(&mut self, doc: KnowledgeDocument) {
        self.documents.insert(doc.id.clone(), doc);
    }

    pub fn add_query(&mut self, record: QueryRecord) {
        if let Some(eval) = &record.evaluation {
            self.eval_stats.total_queries += 1;
            if eval.passed {
                self.eval_stats.passed_evaluations += 1;
            } else {
                self.eval_stats.failed_evaluations += 1;
            }
            // Update rolling averages
            let n = self.eval_stats.total_queries as f64;
            self.eval_stats.avg_faithfulness =
                ((n - 1.0) * self.eval_stats.avg_faithfulness + eval.faithfulness) / n;
            self.eval_stats.avg_relevancy =
                ((n - 1.0) * self.eval_stats.avg_relevancy + eval.relevancy) / n;
        }
        self.queries.push(record);
    }

    pub fn get_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "documents": self.documents.len(),
            "total_chunks": self.documents.values().map(|d| d.chunk_count).sum::<usize>(),
            "queries": self.queries.len(),
            // TokenMemory provides detailed token tracking
            "memory": {
                "messages": self.memory.len(),
                "current_tokens": self.memory.current_tokens(),
                "max_tokens": self.memory.max_tokens(),
                "remaining_tokens": self.memory.remaining_tokens(),
                "utilization_percent": (self.memory.current_tokens() as f64 / self.memory.max_tokens() as f64 * 100.0).round(),
            },
            "evaluation": self.eval_stats,
        })
    }
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AGENT TOOLS (using mcpkit-agent Tool trait)
// ============================================================================

/// Tool for searching the knowledge base
pub struct SearchKnowledgeTool<P: Provider> {
    kb: Arc<RwLock<KnowledgeBase>>,
    provider: Arc<P>,
}

#[async_trait]
impl<P: Provider + 'static> AgentTool for SearchKnowledgeTool<P> {
    fn schema(&self) -> ToolSchema {
        ToolSchema::new(
            "search_knowledge",
            "Search the knowledge base for relevant information. Use this when you need to find \
             specific facts, context, or documentation to answer a question.",
        )
        .add_parameter(
            "query",
            "string",
            "The search query to find relevant documents",
            true,
        )
        .add_parameter(
            "top_k",
            "integer",
            "Number of results to return (default: 5)",
            false,
        )
    }

    async fn execute(&self, args: serde_json::Value) -> mcpkit_agent::AgentResult<ToolOutput> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'query' parameter"))?;
        let top_k = args["top_k"].as_u64().unwrap_or(5) as usize;

        // Generate embedding for query
        let embed_req = EmbeddingRequest::new(query);

        let embed_response = self
            .provider
            .embed(embed_req)
            .await
            .map_err(|e| mcpkit_agent::AgentError::custom(format!("Embedding failed: {e}")))?;

        let query_embedding = embed_response
            .embeddings
            .first()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("No embedding returned"))?;

        // Search vector store
        let kb = self.kb.read().await;
        let options = SearchOptions::top_k(top_k).threshold(0.3);

        let results = kb
            .vector_store
            .search(&query_embedding.embedding, options)
            .await
            .map_err(|e| mcpkit_agent::AgentError::custom(format!("Search failed: {e}")))?;

        // Format results
        let formatted: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "content": r.metadata.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    "source": r.metadata.get("source")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    "score": r.score,
                })
            })
            .collect();

        let response = serde_json::json!({
            "results": formatted,
            "query": query,
            "count": formatted.len()
        });

        Ok(
            ToolOutput::success(serde_json::to_string_pretty(&response).unwrap())
                .with_data(response),
        )
    }

    fn name(&self) -> &str {
        "search_knowledge"
    }
}

/// Tool for evaluating an answer's quality
pub struct EvaluateAnswerTool<P: Provider> {
    provider: Arc<P>,
}

#[async_trait]
impl<P: Provider + 'static> AgentTool for EvaluateAnswerTool<P> {
    fn schema(&self) -> ToolSchema {
        ToolSchema::new(
            "evaluate_answer",
            "Evaluate the quality of an answer against the original question and retrieved context. \
             Use this to assess faithfulness and relevancy before providing a final answer.",
        )
        .add_parameter("question", "string", "The original question", true)
        .add_parameter("answer", "string", "The generated answer to evaluate", true)
        .add_parameter("context", "string", "The context used to generate the answer", true)
    }

    async fn execute(&self, args: serde_json::Value) -> mcpkit_agent::AgentResult<ToolOutput> {
        let question = args["question"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'question'"))?;
        let answer = args["answer"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'answer'"))?;
        let context = args["context"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'context'"))?;

        // Use compile-time validated template
        let eval_template = EvalPromptTemplate {
            question: question.to_string(),
            answer: answer.to_string(),
            context: context.to_string(),
        };

        let request = CompletionRequest::new()
            .message(Message::user(eval_template.render()))
            .temperature(0.0)
            .max_tokens(500);

        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| mcpkit_agent::AgentError::custom(format!("Evaluation failed: {e}")))?;

        let content = response.text().unwrap_or_default();

        // Parse the JSON response
        let eval: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| {
            serde_json::json!({
                "faithfulness": 0.5,
                "relevancy": 0.5,
                "passed": false,
                "feedback": "Could not parse evaluation"
            })
        });

        Ok(ToolOutput::success(serde_json::to_string_pretty(&eval).unwrap()).with_data(eval))
    }

    fn name(&self) -> &str {
        "evaluate_answer"
    }
}

/// Tool for synthesizing a final answer
pub struct SynthesizeAnswerTool<P: Provider> {
    provider: Arc<P>,
}

#[async_trait]
impl<P: Provider + 'static> AgentTool for SynthesizeAnswerTool<P> {
    fn schema(&self) -> ToolSchema {
        ToolSchema::new(
            "synthesize_answer",
            "Synthesize a comprehensive answer from retrieved context. Use this after searching \
             the knowledge base to formulate a well-structured response.",
        )
        .add_parameter("question", "string", "The question to answer", true)
        .add_parameter("context", "string", "Retrieved context (joined text)", true)
    }

    async fn execute(&self, args: serde_json::Value) -> mcpkit_agent::AgentResult<ToolOutput> {
        let question = args["question"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'question'"))?;
        let context = args["context"]
            .as_str()
            .ok_or_else(|| mcpkit_agent::AgentError::custom("Missing 'context'"))?;

        // Use compile-time validated template
        let synthesis_template = SynthesisPromptTemplate {
            question: question.to_string(),
            context: context.to_string(),
        };

        let request = CompletionRequest::new()
            .message(Message::user(synthesis_template.render()))
            .temperature(0.3)
            .max_tokens(1000);

        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| mcpkit_agent::AgentError::custom(format!("Synthesis failed: {e}")))?;

        let answer = response.text().unwrap_or_default();
        let result = serde_json::json!({
            "answer": answer,
            "tokens_used": response.usage.total_tokens
        });

        Ok(ToolOutput::success(answer).with_data(result))
    }

    fn name(&self) -> &str {
        "synthesize_answer"
    }
}

// ============================================================================
// SELF-IMPROVING QUERY CHAIN (using mcpkit-chain)
// ============================================================================

/// A chain that queries, evaluates, and potentially regenerates answers
/// Uses mcpkit-chain for LCEL-style composition
pub struct SelfImprovingQueryChain<P: Provider + 'static> {
    provider: Arc<P>,
    kb: Arc<RwLock<KnowledgeBase>>,
    max_retries: usize,
    quality_threshold: f64,
}

impl<P: Provider + 'static> SelfImprovingQueryChain<P> {
    pub fn new(provider: Arc<P>, kb: Arc<RwLock<KnowledgeBase>>) -> Self {
        Self {
            provider,
            kb,
            max_retries: 2,
            quality_threshold: 0.7,
        }
    }

    /// Execute the full query → retrieve → generate → evaluate → improve loop
    pub async fn execute(&self, query: &str) -> Result<QueryRecord, McpError> {
        let mut attempts = 0;
        let mut last_response = String::new();
        let mut last_context = Vec::new();
        let mut evaluation = None;

        // Add query to conversation memory (mcpkit-memory)
        {
            let mut kb = self.kb.write().await;
            let _ = kb.memory.add(Message::user(query.to_string())).await;
        }

        while attempts <= self.max_retries {
            attempts += 1;
            info!(attempt = attempts, query = query, "Processing query");

            // Step 1: Retrieve relevant documents
            let context = self.retrieve(query).await?;

            // IMPORTANT: Store context BEFORE using it to ensure sources are populated
            last_context = context.clone();

            // If no context found, provide a helpful message
            if context.is_empty() {
                last_response = "No relevant documents found in the knowledge base. Please ingest some documents first.".to_string();
                evaluation = Some(EvaluationResult {
                    faithfulness: 0.0,
                    relevancy: 0.0,
                    passed: false,
                    feedback: "No context available".to_string(),
                });
                break;
            }

            // Step 2: Generate answer using chain composition (mcpkit-chain)
            let response = self.generate_with_chain(query, &context).await?;
            last_response = response.clone();

            // Step 3: Evaluate using mcpkit-eval metrics
            let eval = self
                .evaluate_with_metrics(query, &response, &context)
                .await?;

            if eval.passed || attempts > self.max_retries {
                evaluation = Some(eval);
                break;
            }

            // Step 4: Log regeneration attempt
            warn!(
                attempt = attempts,
                faithfulness = eval.faithfulness,
                relevancy = eval.relevancy,
                "Answer quality below threshold, regenerating"
            );

            // Update stats
            {
                let mut kb = self.kb.write().await;
                kb.eval_stats.regeneration_count += 1;
            }

            evaluation = Some(eval);
        }

        // Add response to conversation memory
        {
            let mut kb = self.kb.write().await;
            let _ = kb
                .memory
                .add(Message::assistant(last_response.clone()))
                .await;
        }

        // Build sources from last_context (FIX: ensure sources are populated)
        let sources: Vec<String> = last_context
            .iter()
            .take(3)
            .map(|s| {
                if s.len() > 100 {
                    format!("{}...", &s[..100])
                } else {
                    s.clone()
                }
            })
            .collect();

        let record = QueryRecord {
            id: Uuid::new_v4().to_string(),
            query: query.to_string(),
            response: last_response,
            sources,
            confidence: evaluation
                .as_ref()
                .map(|e| (e.faithfulness + e.relevancy) / 2.0)
                .unwrap_or(0.5),
            evaluation,
            timestamp: Utc::now(),
        };

        // Store in knowledge base
        {
            let mut kb = self.kb.write().await;
            kb.add_query(record.clone());
        }

        Ok(record)
    }

    async fn retrieve(&self, query: &str) -> Result<Vec<String>, McpError> {
        // Generate query embedding
        let embed_req = EmbeddingRequest::new(query);

        let embed_response =
            self.provider
                .embed(embed_req)
                .await
                .map_err(|e| McpError::InternalMessage {
                    message: format!("Embedding failed: {e}"),
                })?;

        let query_embedding =
            embed_response
                .embeddings
                .first()
                .ok_or_else(|| McpError::InternalMessage {
                    message: "No embedding returned".to_string(),
                })?;

        // Search vector store
        let kb = self.kb.read().await;
        let options = SearchOptions::top_k(5).threshold(0.3);

        let results = kb
            .vector_store
            .search(&query_embedding.embedding, options)
            .await
            .map_err(|e| McpError::InternalMessage {
                message: format!("Search failed: {e}"),
            })?;

        Ok(results
            .iter()
            .filter_map(|r| {
                r.metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect())
    }

    /// Generate answer using mcpkit-chain composition
    ///
    /// Demonstrates LCEL-style chain composition with:
    /// - PromptRunnable: Template formatting with {variable} syntax
    /// - LlmRunnable: LLM provider integration
    /// - RunnableRetry: Fault-tolerant retry logic
    /// - `.then()`: Sequential composition
    async fn generate_with_chain(
        &self,
        query: &str,
        context: &[String],
    ) -> Result<String, McpError> {
        let context_str = context.join("\n\n---\n\n");

        // Step 1: Format prompt using PromptRunnable
        // Uses {variable} syntax for runtime template interpolation
        let prompt_step = PromptRunnable::new(
            r#"Answer the following question based on the provided context.
Be accurate and cite specific information from the context.

CONTEXT:
{context}

QUESTION: {question}

ANSWER:"#,
        )
        .with_name("FormatPrompt");

        // Step 2: Call LLM with configurable parameters
        let llm_step = LlmRunnable::from_arc(Arc::clone(&self.provider))
            .temperature(0.3)
            .max_tokens(1000)
            .with_name("GenerateAnswer");

        // Step 3: Wrap LLM call in retry logic for fault tolerance
        // RunnableRetry provides automatic retries on transient failures
        let llm_with_retry = RunnableRetry::new(llm_step, 3)
            .delay_ms(500)
            .with_name("GenerateWithRetry");

        // Compose the chain: Prompt → Retry(LLM)
        let chain = prompt_step.then(llm_with_retry);

        // Create input object with context and question
        let mut input_obj = HashMap::new();
        input_obj.insert("context".to_string(), ChainValue::String(context_str));
        input_obj.insert(
            "question".to_string(),
            ChainValue::String(query.to_string()),
        );

        let result = chain
            .invoke(ChainValue::Object(input_obj))
            .await
            .map_err(|e| McpError::InternalMessage {
                message: format!("Chain execution failed: {e}"),
            })?;

        Ok(result.to_string_value())
    }

    /// Evaluate using mcpkit-eval metrics (FaithfulnessMetric, AnswerRelevancyMetric)
    async fn evaluate_with_metrics(
        &self,
        question: &str,
        answer: &str,
        context: &[String],
    ) -> Result<EvaluationResult, McpError> {
        // Create test case for evaluation
        let test_case = TestCase::new(question)
            .with_actual_output(answer)
            .with_contexts(context.to_vec());

        // Create metrics using mcpkit-eval
        let faithfulness_metric = FaithfulnessMetric::from_arc(Arc::clone(&self.provider));
        let relevancy_metric = AnswerRelevancyMetric::from_arc(Arc::clone(&self.provider));

        // Run faithfulness evaluation
        let faithfulness_result = faithfulness_metric
            .evaluate(&test_case)
            .await
            .map_err(|e| McpError::InternalMessage {
                message: format!("Faithfulness evaluation failed: {e}"),
            })?;

        // Run relevancy evaluation
        let relevancy_result =
            relevancy_metric
                .evaluate(&test_case)
                .await
                .map_err(|e| McpError::InternalMessage {
                    message: format!("Relevancy evaluation failed: {e}"),
                })?;

        let passed = faithfulness_result.score >= self.quality_threshold
            && relevancy_result.score >= self.quality_threshold;

        Ok(EvaluationResult {
            faithfulness: faithfulness_result.score,
            relevancy: relevancy_result.score,
            passed,
            feedback: format!(
                "Faithfulness: {} | Relevancy: {}",
                faithfulness_result.reason.unwrap_or_default(),
                relevancy_result.reason.unwrap_or_default()
            ),
        })
    }
}

// ============================================================================
// MCP SERVER IMPLEMENTATION
// ============================================================================

/// The main Knowledge Agent MCP server
pub struct KnowledgeAgentServer<P: Provider + 'static> {
    kb: Arc<RwLock<KnowledgeBase>>,
    provider: Arc<P>,
    query_chain: Arc<SelfImprovingQueryChain<P>>,
}

impl<P: Provider + 'static> KnowledgeAgentServer<P> {
    pub fn new(provider: Arc<P>) -> Self {
        let kb = Arc::new(RwLock::new(KnowledgeBase::new()));
        let query_chain = Arc::new(SelfImprovingQueryChain::new(
            Arc::clone(&provider),
            Arc::clone(&kb),
        ));

        Self {
            kb,
            provider,
            query_chain,
        }
    }

    /// Get server capabilities
    pub fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities::new()
            .with_tools()
            .with_resources()
            .with_prompts()
            .with_tasks()
    }

    /// Get server info
    pub fn info(&self) -> ServerInfo {
        ServerInfo::new("knowledge-agent", "0.1.0")
    }

    // ========================================================================
    // TOOLS
    // ========================================================================

    /// List available tools
    pub fn list_tools(&self) -> Vec<McpTool> {
        vec![
            McpTool::new("ingest_document")
                .description(
                    "Ingest a document into the knowledge base. Supports text content \
                     with automatic chunking and embedding.",
                )
                .input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Document title"
                        },
                        "content": {
                            "type": "string",
                            "description": "Document content (plain text)"
                        },
                        "source": {
                            "type": "string",
                            "description": "Source URL or identifier"
                        }
                    },
                    "required": ["title", "content"]
                }))
                .annotations(ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
            McpTool::new("query_knowledge")
                .description(
                    "Query the knowledge base with a natural language question. \
                     Uses RAG to retrieve relevant context and generate an answer. \
                     Automatically evaluates answer quality and regenerates if needed.",
                )
                .input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "Natural language question"
                        }
                    },
                    "required": ["question"]
                }))
                .annotations(ToolAnnotations {
                    read_only_hint: Some(true),
                    ..Default::default()
                }),
            McpTool::new("get_statistics")
                .description(
                    "Get knowledge base statistics including document count, \
                             query history, memory usage, and evaluation metrics.",
                )
                .input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {}
                }))
                .annotations(ToolAnnotations {
                    read_only_hint: Some(true),
                    ..Default::default()
                }),
            McpTool::new("get_conversation_history")
                .description(
                    "Get recent conversation history from memory. \
                     Demonstrates mcpkit-memory integration.",
                )
                .input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum messages to return (default: 10)"
                        }
                    }
                }))
                .annotations(ToolAnnotations {
                    read_only_hint: Some(true),
                    ..Default::default()
                }),
            McpTool::new("run_agent")
                .description(
                    "Run the autonomous ReAct agent to answer complex questions. \
                     The agent can search knowledge, evaluate answers, and iterate \
                     until it produces a high-quality response.",
                )
                .input_schema(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "The task for the agent to complete"
                        },
                        "max_iterations": {
                            "type": "integer",
                            "description": "Maximum reasoning iterations (default: 5)",
                            "default": 5
                        }
                    },
                    "required": ["task"]
                })),
        ]
    }

    /// Execute a tool
    pub async fn call_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        match name {
            "ingest_document" => self.tool_ingest_document(args).await,
            "query_knowledge" => self.tool_query_knowledge(args).await,
            "get_statistics" => self.tool_get_statistics().await,
            "get_conversation_history" => self.tool_get_conversation_history(args).await,
            "run_agent" => self.tool_run_agent(args).await,
            _ => Err(McpError::invalid_params(
                "call_tool",
                format!("Unknown tool: {name}"),
            )),
        }
    }

    async fn tool_ingest_document(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let title = args["title"]
            .as_str()
            .ok_or_else(|| McpError::invalid_params("ingest_document", "Missing 'title'"))?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| McpError::invalid_params("ingest_document", "Missing 'content'"))?;
        let source = args["source"]
            .as_str()
            .unwrap_or("user-provided")
            .to_string();

        info!(
            title = title,
            content_len = content.len(),
            "Ingesting document"
        );

        // Create splitter and chunk document (mcpkit-rag)
        let splitter = RecursiveCharacterSplitter::new()
            .chunk_size(500)
            .chunk_overlap(50);

        let doc = Document::new(content);
        let chunk_docs = splitter.split(&doc);
        let chunks: Vec<String> = chunk_docs.iter().map(|d| d.content.clone()).collect();
        let chunk_count = chunks.len();

        // Generate embeddings for each chunk (mcpkit-provider)
        let embed_req = EmbeddingRequest::batch(chunks.clone());

        let embed_response =
            self.provider
                .embed(embed_req)
                .await
                .map_err(|e| McpError::InternalMessage {
                    message: format!("Embedding failed: {e}"),
                })?;

        // Store in vector store (mcpkit-embedding)
        let doc_id = Uuid::new_v4().to_string();
        {
            let mut kb = self.kb.write().await;
            for (i, (chunk, embedding)) in chunks
                .iter()
                .zip(embed_response.embeddings.iter())
                .enumerate()
            {
                let chunk_id = format!("{doc_id}_{i}");
                let mut metadata = HashMap::new();
                metadata.insert("content".to_string(), serde_json::json!(chunk));
                metadata.insert("source".to_string(), serde_json::json!(&source));
                metadata.insert("title".to_string(), serde_json::json!(title));
                metadata.insert("chunk_index".to_string(), serde_json::json!(i));

                let stored =
                    StoredEmbedding::with_metadata(chunk_id, embedding.embedding.clone(), metadata);

                kb.vector_store
                    .insert(stored)
                    .await
                    .map_err(|e| McpError::InternalMessage {
                        message: format!("Storage failed: {e}"),
                    })?;
            }

            // Record document
            kb.add_document(KnowledgeDocument {
                id: doc_id.clone(),
                title: title.to_string(),
                content: content.to_string(),
                source,
                ingested_at: Utc::now(),
                chunk_count,
            });
        }

        Ok(CallToolResult::text(format!(
            "Successfully ingested document '{title}' with {chunk_count} chunks"
        )))
    }

    async fn tool_query_knowledge(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let question = args["question"]
            .as_str()
            .ok_or_else(|| McpError::invalid_params("query_knowledge", "Missing 'question'"))?;

        let record = self.query_chain.execute(question).await?;

        let response_json = serde_json::json!({
            "answer": record.response,
            "confidence": record.confidence,
            "sources": record.sources,
            "evaluation": record.evaluation,
        });

        Ok(CallToolResult::text(
            serde_json::to_string_pretty(&response_json).unwrap(),
        ))
    }

    async fn tool_get_statistics(&self) -> Result<CallToolResult, McpError> {
        let kb = self.kb.read().await;
        let stats = kb.get_stats();

        Ok(CallToolResult::text(
            serde_json::to_string_pretty(&stats).unwrap(),
        ))
    }

    async fn tool_get_conversation_history(
        &self,
        args: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let limit = args["limit"].as_u64().unwrap_or(10) as usize;

        let kb = self.kb.read().await;
        let messages = kb
            .memory
            .last_n(limit)
            .await
            .map_err(|e| McpError::InternalMessage {
                message: format!("Memory read failed: {e}"),
            })?;

        let formatted: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": format!("{:?}", m.role),
                    "content": m.text().unwrap_or_default(),
                })
            })
            .collect();

        Ok(CallToolResult::text(
            serde_json::to_string_pretty(&formatted).unwrap(),
        ))
    }

    async fn tool_run_agent(&self, args: serde_json::Value) -> Result<CallToolResult, McpError> {
        let task = args["task"]
            .as_str()
            .ok_or_else(|| McpError::invalid_params("run_agent", "Missing 'task'"))?;
        let max_iterations = args["max_iterations"].as_u64().unwrap_or(5) as usize;

        info!(
            task = task,
            max_iterations = max_iterations,
            "Running ReAct agent"
        );

        // Create agent with tools (mcpkit-agent)
        let agent = ReActAgent::from_arc(Arc::clone(&self.provider)).system_prompt(
            "You are a knowledge assistant with access to a document database. \
             Use the search_knowledge tool to find information, then synthesize \
             an answer. Always evaluate your answers for quality before finishing.",
        );

        // Create executor and register tools
        let mut executor = AgentExecutor::new(agent).max_iterations(max_iterations);

        executor.register_tool(SearchKnowledgeTool {
            kb: Arc::clone(&self.kb),
            provider: Arc::clone(&self.provider),
        });
        executor.register_tool(EvaluateAnswerTool {
            provider: Arc::clone(&self.provider),
        });
        executor.register_tool(SynthesizeAnswerTool {
            provider: Arc::clone(&self.provider),
        });

        // Run agent
        let result = executor
            .run(task)
            .await
            .map_err(|e| McpError::InternalMessage {
                message: format!("Agent execution failed: {e}"),
            })?;

        let response = serde_json::json!({
            "result": result.output,
            "steps": result.steps.len(),
            "iterations": result.iterations,
            "trace": result.trace(),
        });

        Ok(CallToolResult::text(
            serde_json::to_string_pretty(&response).unwrap(),
        ))
    }

    // ========================================================================
    // RESOURCES
    // ========================================================================

    /// List available resources
    pub fn list_resources(&self) -> Vec<Resource> {
        vec![
            Resource::new("knowledge://stats", "Knowledge Base Statistics")
                .mime_type("application/json")
                .description("Current statistics about the knowledge base"),
            Resource::new("knowledge://queries", "Query History")
                .mime_type("application/json")
                .description("Recent queries and their evaluations"),
            Resource::new("knowledge://documents", "Document Index")
                .mime_type("application/json")
                .description("List of all ingested documents"),
            Resource::new("knowledge://memory", "Conversation Memory")
                .mime_type("application/json")
                .description("Current conversation memory state"),
        ]
    }

    /// Read a resource
    pub async fn read_resource(&self, uri: &str) -> Result<ResourceContents, McpError> {
        match uri {
            "knowledge://stats" => {
                let kb = self.kb.read().await;
                let stats = kb.get_stats();
                Ok(ResourceContents::text(
                    uri,
                    serde_json::to_string_pretty(&stats).unwrap(),
                ))
            }
            "knowledge://queries" => {
                let kb = self.kb.read().await;
                let queries: Vec<_> = kb.queries.iter().rev().take(10).collect();
                Ok(ResourceContents::text(
                    uri,
                    serde_json::to_string_pretty(&queries).unwrap(),
                ))
            }
            "knowledge://documents" => {
                let kb = self.kb.read().await;
                let docs: Vec<_> = kb
                    .documents
                    .values()
                    .map(|d| {
                        serde_json::json!({
                            "id": d.id,
                            "title": d.title,
                            "source": d.source,
                            "chunks": d.chunk_count,
                            "ingested_at": d.ingested_at,
                        })
                    })
                    .collect();
                Ok(ResourceContents::text(
                    uri,
                    serde_json::to_string_pretty(&docs).unwrap(),
                ))
            }
            "knowledge://memory" => {
                let kb = self.kb.read().await;
                let messages =
                    kb.memory
                        .messages()
                        .await
                        .map_err(|e| McpError::InternalMessage {
                            message: format!("Memory read failed: {e}"),
                        })?;
                let formatted: Vec<serde_json::Value> = messages
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": format!("{:?}", m.role),
                            "content": m.text().unwrap_or_default(),
                        })
                    })
                    .collect();
                Ok(ResourceContents::text(
                    uri,
                    serde_json::to_string_pretty(&formatted).unwrap(),
                ))
            }
            _ => Err(McpError::ResourceNotFound {
                uri: uri.to_string(),
            }),
        }
    }

    // ========================================================================
    // PROMPTS
    // ========================================================================

    /// List available prompts
    pub fn list_prompts(&self) -> Vec<Prompt> {
        vec![
            Prompt::new("question_answering")
                .description("Generate a prompt for answering questions using the knowledge base")
                .required_arg("question", "The question to answer"),
            Prompt::new("document_summary")
                .description("Generate a prompt for summarizing a document")
                .required_arg("document_id", "ID of the document to summarize"),
            Prompt::new("evaluation_criteria")
                .description("Generate evaluation criteria for assessing answer quality"),
        ]
    }

    /// Get a prompt
    pub async fn get_prompt(
        &self,
        name: &str,
        args: HashMap<String, String>,
    ) -> Result<GetPromptResult, McpError> {
        match name {
            "question_answering" => {
                let question = args.get("question").ok_or_else(|| {
                    McpError::invalid_params("get_prompt", "Missing 'question' argument")
                })?;

                // Use mcpkit-template for prompt building
                let qa_template = QAPromptTemplate {
                    question: question.clone(),
                    context: "[Context will be retrieved from knowledge base]".to_string(),
                };

                Ok(GetPromptResult {
                    description: Some("Question answering prompt".to_string()),
                    messages: vec![PromptMessage::user(qa_template.render())],
                })
            }
            "document_summary" => {
                let doc_id = args.get("document_id").ok_or_else(|| {
                    McpError::invalid_params("get_prompt", "Missing 'document_id' argument")
                })?;

                let kb = self.kb.read().await;
                let doc = kb.documents.get(doc_id).ok_or_else(|| {
                    McpError::invalid_params("get_prompt", format!("Document not found: {doc_id}"))
                })?;

                Ok(GetPromptResult {
                    description: Some(format!("Summary prompt for '{}'", doc.title)),
                    messages: vec![PromptMessage::user(format!(
                        "[System: You are a document summarization expert. Create clear, \
                         concise summaries that capture the key points.]\n\n\
                         Please summarize the following document:\n\n\
                         Title: {}\n\
                         Source: {}\n\n\
                         Content:\n{}",
                        doc.title, doc.source, doc.content
                    ))],
                })
            }
            "evaluation_criteria" => Ok(GetPromptResult {
                description: Some("Evaluation criteria prompt".to_string()),
                messages: vec![PromptMessage::user(
                    "Use these criteria to evaluate answers:\n\n\
                     1. FAITHFULNESS (0.0-1.0): Is the answer factually consistent \
                        with the source context? Does it avoid hallucination?\n\n\
                     2. RELEVANCY (0.0-1.0): Does the answer directly address the \
                        question asked? Is it on-topic?\n\n\
                     3. COMPLETENESS: Does the answer cover all aspects of the question?\n\n\
                     4. CLARITY: Is the answer well-structured and easy to understand?\n\n\
                     An answer passes evaluation if both faithfulness and relevancy \
                     scores are >= 0.7.",
                )],
            }),
            _ => Err(McpError::invalid_params(
                "get_prompt",
                format!("Unknown prompt: {name}"),
            )),
        }
    }

    // ========================================================================
    // TASKS (Long-running operations)
    // ========================================================================

    /// Create a task for async document ingestion
    pub async fn create_ingestion_task(
        &self,
        title: String,
        content: String,
        source: String,
    ) -> TaskId {
        let task_id = TaskId::generate();
        let task = IngestionTask {
            id: task_id.clone(),
            document_title: title.clone(),
            status: TaskStatus::Running,
            progress: 0.0,
            started_at: Utc::now(),
            completed_at: None,
            error: None,
        };

        {
            let mut kb = self.kb.write().await;
            kb.tasks.insert(task_id.clone(), task);
        }

        // Spawn async processing
        let kb = Arc::clone(&self.kb);
        let provider = Arc::clone(&self.provider);
        let task_id_clone = task_id.clone();

        tokio::spawn(async move {
            // Simulate chunking progress
            let splitter = RecursiveCharacterSplitter::new()
                .chunk_size(500)
                .chunk_overlap(50);

            let doc = Document::new(&content);
            let chunk_docs = splitter.split(&doc);
            let chunks: Vec<String> = chunk_docs.iter().map(|d| d.content.clone()).collect();
            let total_chunks = chunks.len();

            // Update progress
            {
                let mut kb = kb.write().await;
                if let Some(task) = kb.tasks.get_mut(&task_id_clone) {
                    task.progress = 0.2;
                }
            }

            // Generate embeddings
            let embed_req = EmbeddingRequest::batch(chunks.clone());

            match provider.embed(embed_req).await {
                Ok(embed_response) => {
                    // Store embeddings
                    let doc_id = Uuid::new_v4().to_string();
                    {
                        let mut kb = kb.write().await;

                        // Update progress
                        if let Some(task) = kb.tasks.get_mut(&task_id_clone) {
                            task.progress = 0.5;
                        }

                        for (i, (chunk, embedding)) in chunks
                            .iter()
                            .zip(embed_response.embeddings.iter())
                            .enumerate()
                        {
                            let chunk_id = format!("{doc_id}_{i}");
                            let mut metadata = HashMap::new();
                            metadata.insert("content".to_string(), serde_json::json!(chunk));
                            metadata.insert("source".to_string(), serde_json::json!(&source));
                            metadata.insert("title".to_string(), serde_json::json!(&title));
                            metadata.insert("chunk_index".to_string(), serde_json::json!(i));

                            let stored = StoredEmbedding::with_metadata(
                                chunk_id,
                                embedding.embedding.clone(),
                                metadata,
                            );

                            let _ = kb.vector_store.insert(stored).await;

                            // Update progress
                            if let Some(task) = kb.tasks.get_mut(&task_id_clone) {
                                task.progress = 0.5 + (0.5 * (i as f64 / total_chunks as f64));
                            }
                        }

                        // Record document
                        kb.add_document(KnowledgeDocument {
                            id: doc_id,
                            title: title.clone(),
                            content,
                            source,
                            ingested_at: Utc::now(),
                            chunk_count: total_chunks,
                        });

                        // Mark complete
                        if let Some(task) = kb.tasks.get_mut(&task_id_clone) {
                            task.status = TaskStatus::Completed;
                            task.progress = 1.0;
                            task.completed_at = Some(Utc::now());
                        }
                    }
                }
                Err(e) => {
                    let mut kb = kb.write().await;
                    if let Some(task) = kb.tasks.get_mut(&task_id_clone) {
                        task.status = TaskStatus::Failed;
                        task.error = Some(e.to_string());
                        task.completed_at = Some(Utc::now());
                    }
                }
            }
        });

        task_id
    }

    /// Get task status
    pub async fn get_task_status(&self, task_id: &TaskId) -> Option<Task> {
        let kb = self.kb.read().await;
        kb.tasks.get(task_id).map(|t| {
            let mut task = Task::new(t.id.clone());
            // Update task status based on internal state
            match t.status {
                TaskStatus::Running => task.start(),
                TaskStatus::Completed => {
                    task.complete(serde_json::json!({
                        "document": t.document_title,
                        "status": "ingested"
                    }));
                }
                TaskStatus::Failed => {
                    if let Some(ref err) = t.error {
                        task.fail(mcpkit_core::types::TaskError::new(-1, err.clone()));
                    }
                }
                _ => {}
            }
            // Update progress if running
            if t.status == TaskStatus::Running {
                let progress_percent = (t.progress * 100.0) as u64;
                task.update_progress(
                    TaskProgress::new(progress_percent)
                        .total(100)
                        .message(format!("Processing document: {}", t.document_title)),
                );
            }
            task
        })
    }
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for --stdio flag
    let args: Vec<String> = std::env::args().collect();
    let stdio_mode = args.iter().any(|a| a == "--stdio");

    // Initialize tracing (to stderr to not interfere with stdio)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("knowledge_agent=info".parse()?)
                .add_directive("mcpkit=debug".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("Starting Knowledge Agent MCP Server");

    // Create provider (mock for demo - replace with real provider)
    // In production: Arc::new(OpenAiProvider::new(api_key))
    let provider: Arc<MockProvider> = Arc::new(MockProvider::new());

    // Create server
    let server = KnowledgeAgentServer::new(provider);

    if stdio_mode {
        // Run as actual MCP server with stdio transport
        eprintln!("Knowledge Agent MCP Server running on stdio...");
        eprintln!("Waiting for JSON-RPC messages on stdin...");

        let transport = StdioTransport::new();

        // Message processing loop - real MCP stdio transport implementation
        loop {
            // Receive next message from stdin
            let msg = match transport.recv().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    eprintln!("Connection closed (EOF)");
                    break;
                }
                Err(e) => {
                    eprintln!("Transport error: {e}");
                    break;
                }
            };

            // Process the message based on type
            let response: Option<ProtocolMessage> = match msg {
                ProtocolMessage::Request(request) => {
                    let id = request.id.clone();
                    let method = request.method.as_ref();
                    let params = request.params.as_ref();

                    eprintln!("Received request: {} (id: {})", method, id);

                    // Route request to appropriate handler
                    let result: Result<serde_json::Value, McpError> = match method {
                        // Initialization
                        "initialize" => {
                            let protocol_version = params
                                .and_then(|p: &serde_json::Value| p.get("protocolVersion"))
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .unwrap_or("2025-11-05");

                            Ok(serde_json::json!({
                                "protocolVersion": protocol_version,
                                "serverInfo": {
                                    "name": server.info().name,
                                    "version": server.info().version,
                                },
                                "capabilities": {
                                    "tools": server.capabilities().tools,
                                    "resources": server.capabilities().resources,
                                    "prompts": server.capabilities().prompts,
                                    "tasks": server.capabilities().tasks,
                                }
                            }))
                        }
                        "ping" => Ok(serde_json::json!({})),

                        // Tools
                        "tools/list" => Ok(serde_json::json!({ "tools": server.list_tools() })),
                        "tools/call" => {
                            let name = params
                                .and_then(|p: &serde_json::Value| p.get("name"))
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .ok_or_else(|| {
                                    McpError::invalid_params("tools/call", "missing name")
                                })?;
                            let args = params
                                .and_then(|p: &serde_json::Value| p.get("arguments"))
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!({}));

                            server
                                .call_tool(name, args)
                                .await
                                .map(|r| serde_json::to_value(r).unwrap_or_default())
                        }

                        // Resources
                        "resources/list" => {
                            Ok(serde_json::json!({ "resources": server.list_resources() }))
                        }
                        "resources/read" => {
                            let uri = params
                                .and_then(|p: &serde_json::Value| p.get("uri"))
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .ok_or_else(|| {
                                    McpError::invalid_params("resources/read", "missing uri")
                                })?;

                            server
                                .read_resource(uri)
                                .await
                                .map(|contents| serde_json::json!({ "contents": [contents] }))
                        }

                        // Prompts
                        "prompts/list" => {
                            Ok(serde_json::json!({ "prompts": server.list_prompts() }))
                        }
                        "prompts/get" => {
                            let name = params
                                .and_then(|p: &serde_json::Value| p.get("name"))
                                .and_then(|v: &serde_json::Value| v.as_str())
                                .ok_or_else(|| {
                                    McpError::invalid_params("prompts/get", "missing name")
                                })?;
                            let args: HashMap<String, String> = params
                                .and_then(|p: &serde_json::Value| p.get("arguments"))
                                .and_then(|v: &serde_json::Value| v.as_object())
                                .map(|obj| {
                                    obj.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.clone(), s.to_string()))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            server
                                .get_prompt(name, args)
                                .await
                                .map(|r| serde_json::to_value(r).unwrap_or_default())
                        }

                        // Unknown method
                        _ => Err(McpError::InternalMessage {
                            message: format!("Method not found: {}", method),
                        }),
                    };

                    // Build response
                    let response = match result {
                        Ok(value) => Response::success(id, value),
                        Err(e) => {
                            let error = JsonRpcError {
                                code: -32603, // Internal error
                                message: e.to_string(),
                                data: None,
                            };
                            Response::error(id, error)
                        }
                    };

                    Some(ProtocolMessage::Response(response))
                }

                ProtocolMessage::Notification(notification) => {
                    let method = notification.method.as_ref();
                    eprintln!("Received notification: {}", method);

                    // Handle notifications (no response needed)
                    match method {
                        "notifications/initialized" => {
                            eprintln!("Client initialized successfully");
                        }
                        "notifications/cancelled" => {
                            eprintln!("Request cancelled by client");
                        }
                        _ => {
                            eprintln!("Unknown notification: {}", method);
                        }
                    }
                    None
                }

                ProtocolMessage::Response(response) => {
                    // We're a server, shouldn't receive responses
                    eprintln!("Unexpected response received: {:?}", response.id);
                    None
                }
            };

            // Send response if one was generated
            if let Some(resp) = response {
                if let Err(e) = transport.send(resp).await {
                    eprintln!("Failed to send response: {e}");
                    break;
                }
            }
        }

        eprintln!("Server shutting down");
    } else {
        // Demo mode: Show capabilities
        println!("\n=== Knowledge Agent MCP Server ===\n");
        println!("Server: {} v{}", server.info().name, server.info().version);
        println!("\nCapabilities:");
        println!("  - Tools: {}", server.capabilities().tools.is_some());
        println!(
            "  - Resources: {}",
            server.capabilities().resources.is_some()
        );
        println!("  - Prompts: {}", server.capabilities().prompts.is_some());
        println!("  - Tasks: {}", server.capabilities().tasks.is_some());

        println!("\nForge Crates Demonstrated:");
        println!("  - mcpkit-provider: LLM abstraction (MockProvider)");
        println!("  - mcpkit-template: Compile-time validated prompts (QAPromptTemplate, etc.)");
        println!("  - mcpkit-memory: TokenMemory for conversation history");
        println!("  - mcpkit-embedding: InMemoryStore for vector search");
        println!("  - mcpkit-chain: LCEL-style chain composition");
        println!("  - mcpkit-agent: ReAct agent with tool execution");
        println!("  - mcpkit-rag: Document chunking with RecursiveCharacterSplitter");
        println!("  - mcpkit-eval: FaithfulnessMetric & AnswerRelevancyMetric");

        println!("\nAvailable Tools:");
        for tool in server.list_tools() {
            println!(
                "  - {}: {}",
                tool.name,
                tool.description.unwrap_or_default()
            );
        }

        println!("\nAvailable Resources:");
        for resource in server.list_resources() {
            println!("  - {}: {}", resource.uri, resource.name);
        }

        println!("\nAvailable Prompts:");
        for prompt in server.list_prompts() {
            println!(
                "  - {}: {}",
                prompt.name,
                prompt.description.unwrap_or_default()
            );
        }

        // Demo: Ingest a sample document
        println!("\n--- Demo: Document Ingestion ---");
        let result = server
            .call_tool(
                "ingest_document",
                serde_json::json!({
                    "title": "Rust Programming Guide",
                    "content": r#"
                        Rust is a systems programming language focused on safety, speed, and concurrency.

                        Key features of Rust include:
                        - Memory safety without garbage collection through ownership and borrowing
                        - Zero-cost abstractions that compile to efficient machine code
                        - Fearless concurrency with thread safety guaranteed at compile time
                        - Pattern matching and algebraic data types for expressive code
                        - A powerful type system with traits and generics

                        Rust uses a unique ownership model where each value has a single owner,
                        and the value is dropped when the owner goes out of scope. References
                        can borrow values temporarily, with compile-time checks ensuring safety.

                        The Cargo package manager handles dependencies, building, testing, and
                        documentation generation. Crates.io hosts the Rust package ecosystem.
                    "#,
                    "source": "demo"
                }),
            )
            .await?;

        println!("Ingestion result: {:?}", result);

        // Demo: Query the knowledge base
        println!("\n--- Demo: Knowledge Query ---");
        let result = server
            .call_tool(
                "query_knowledge",
                serde_json::json!({
                    "question": "What are the key features of Rust?"
                }),
            )
            .await?;

        println!("Query result:");
        for output in &result.content {
            if let Content::Text(tc) = output {
                println!("{}", tc.text);
            }
        }

        // Demo: Get conversation history (demonstrates mcpkit-memory)
        println!("\n--- Demo: Conversation Memory ---");
        let result = server
            .call_tool("get_conversation_history", serde_json::json!({}))
            .await?;
        for output in &result.content {
            if let Content::Text(tc) = output {
                println!("{}", tc.text);
            }
        }

        // Demo: Get statistics
        println!("\n--- Demo: Statistics ---");
        let result = server
            .call_tool("get_statistics", serde_json::json!({}))
            .await?;
        for output in &result.content {
            if let Content::Text(tc) = output {
                println!("{}", tc.text);
            }
        }

        println!("\n=== Server Ready ===");
        println!("Run with --stdio for actual MCP server mode");
    }

    Ok(())
}

// ============================================================================
// MOCK PROVIDER (for demo purposes)
// ============================================================================

/// A mock provider for demonstration
struct MockProvider {
    info: ProviderInfo,
}

impl MockProvider {
    fn new() -> Self {
        Self {
            info: ProviderInfo::new("mock", "Mock Provider")
                .capabilities(ProviderCapabilities::full())
                .default_model("mock-model"),
        }
    }

    /// Creates semantic embeddings using keyword/concept matching
    /// Texts containing similar concepts will have similar vectors
    #[allow(clippy::needless_range_loop)]
    fn create_semantic_embedding(text: &str) -> Vec<f32> {
        let text_lower = text.to_lowercase();
        let mut embedding = vec![0.0f32; 384];

        // Define semantic concepts and their associated keywords
        // Each concept occupies a range of dimensions in the embedding
        let concepts: &[(&[&str], usize)] = &[
            // Rust language concepts (dims 0-49)
            (&["rust", "rustlang", "cargo", "crate"], 0),
            (&["memory", "heap", "stack", "allocation"], 10),
            (&["ownership", "borrow", "lifetime", "reference"], 20),
            (&["safety", "safe", "unsafe", "sound"], 30),
            (
                &["concurrency", "concurrent", "parallel", "thread", "async"],
                40,
            ),
            // Programming concepts (dims 50-99)
            (&["type", "types", "typing", "generic", "trait"], 50),
            (&["error", "result", "option", "handle", "handling"], 60),
            (
                &["performance", "fast", "speed", "efficient", "zero-cost"],
                70,
            ),
            (&["compile", "compiler", "compilation", "static"], 80),
            (&["pattern", "match", "matching", "enum"], 90),
            // General programming (dims 100-149)
            (&["feature", "features", "capability", "capabilities"], 100),
            (&["system", "systems", "low-level", "kernel"], 110),
            (&["abstraction", "abstract", "interface"], 120),
            (&["garbage", "gc", "collection", "collector"], 130),
            (&["data", "structure", "structures", "algorithm"], 140),
            // Questions and actions (dims 150-199)
            (&["what", "which", "how", "why", "when"], 150),
            (&["key", "main", "primary", "important", "core"], 160),
            (&["explain", "describe", "tell", "show"], 170),
            (&["benefit", "advantage", "strength", "pro"], 180),
            (&["use", "using", "used", "usage"], 190),
            // Additional technical concepts (dims 200-249)
            (&["language", "programming", "code", "coding"], 200),
            (&["model", "owner", "owned", "move", "moved"], 210),
            (&["guarantee", "guarantees", "ensure", "prevent"], 220),
            (&["race", "races", "deadlock", "mutex", "lock"], 230),
            (&["macro", "macros", "derive", "procedural"], 240),
        ];

        // Activate dimensions based on keyword matches
        for (keywords, base_dim) in concepts {
            for keyword in *keywords {
                if text_lower.contains(keyword) {
                    // Activate a range of dimensions for this concept
                    for i in 0..10 {
                        let dim = base_dim + i;
                        if dim < 384 {
                            // Use different weights for variety
                            embedding[dim] += 1.0 + (i as f32 * 0.1);
                        }
                    }
                    break; // Only count each concept once
                }
            }
        }

        // Add some text-length based signal to fill remaining dimensions
        let len_signal = (text.len() as f32 / 1000.0).min(1.0);
        for i in 250..300 {
            embedding[i] = len_signal * ((i - 250) as f32 / 50.0);
        }

        // Add word count signal
        let word_count = text.split_whitespace().count() as f32;
        let word_signal = (word_count / 100.0).min(1.0);
        for i in 300..350 {
            embedding[i] = word_signal * ((i - 300) as f32 / 50.0);
        }

        // Hash-based component for uniqueness (dims 350-383)
        let hash = text
            .bytes()
            .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        for i in 350..384 {
            embedding[i] = ((hash >> (i - 350)) & 1) as f32 * 0.1;
        }

        // Normalize the embedding
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for x in &mut embedding {
                *x /= magnitude;
            }
        } else {
            // Fallback: if no keywords matched, use a default vector
            embedding[0] = 1.0;
        }

        embedding
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Return mock responses based on content
        let input = request
            .messages
            .last()
            .and_then(|m| m.text())
            .unwrap_or_default();

        let response = if input.contains("evaluate")
            || input.contains("Evaluate")
            || input.contains("FAITHFULNESS")
        {
            // Return evaluation JSON for mcpkit-eval metrics
            r#"{"score": 0.85, "reason": "Good answer grounded in context"}"#.to_string()
        } else if input.contains("QUESTION:") || input.contains("CONTEXT:") {
            "Based on the context, Rust's key features include memory safety without garbage \
             collection through its ownership model, zero-cost abstractions, fearless concurrency, \
             pattern matching, and a powerful type system with traits and generics. The ownership \
             model ensures each value has a single owner, preventing memory leaks and data races."
                .to_string()
        } else {
            "This is a mock response for demonstration purposes.".to_string()
        };

        Ok(CompletionResponse {
            id: "mock-completion".to_string(),
            model: "mock-model".to_string(),
            content: vec![ContentBlock::text(response)],
            finish_reason: FinishReason::Stop,
            usage: Usage::with_tokens(100, 50),
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

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError> {
        // Generate semantic-aware mock embeddings
        // Uses keyword matching to create embeddings where texts about similar topics
        // have similar vectors (enabling proper RAG retrieval in demos)
        let embeddings = request
            .input
            .iter()
            .enumerate()
            .map(|(idx, text)| {
                let embedding = Self::create_semantic_embedding(text);
                Embedding {
                    index: idx,
                    embedding,
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
