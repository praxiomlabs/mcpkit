//! Text splitters for chunking documents.
//!
//! Text splitters break documents into smaller chunks suitable for embedding
//! and retrieval. Different strategies trade off between semantic coherence
//! and consistent chunk sizes.

use crate::document::Document;

/// Trait for splitting documents into chunks.
///
/// Text splitters divide documents into smaller pieces that can be
/// independently embedded and retrieved. The choice of splitter
/// affects retrieval quality significantly.
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{TextSplitter, RecursiveCharacterSplitter, Document};
///
/// let splitter = RecursiveCharacterSplitter::new()
///     .chunk_size(500)
///     .chunk_overlap(50);
///
/// let doc = Document::new("Long document content...");
/// let chunks = splitter.split(&doc);
/// ```
pub trait TextSplitter: Send + Sync {
    /// Split a document into chunks.
    ///
    /// Returns a vector of smaller documents, each representing a chunk
    /// of the original. Metadata from the original document is preserved.
    fn split(&self, document: &Document) -> Vec<Document>;

    /// Split multiple documents.
    fn split_documents(&self, documents: &[Document]) -> Vec<Document> {
        documents.iter().flat_map(|doc| self.split(doc)).collect()
    }

    /// Get a description of the splitter.
    fn description(&self) -> String {
        "TextSplitter".to_string()
    }
}

/// A recursive character text splitter.
///
/// This splitter tries to split text at natural boundaries (paragraphs,
/// sentences, words) while maintaining the target chunk size. It's the
/// most commonly used splitter for general text.
///
/// # Algorithm
///
/// 1. Try to split on double newlines (paragraphs)
/// 2. If chunks are still too large, split on single newlines
/// 3. Then try sentences (., !, ?)
/// 4. Then try words (spaces)
/// 5. Finally, split on characters if necessary
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{TextSplitter, RecursiveCharacterSplitter, Document};
///
/// let splitter = RecursiveCharacterSplitter::new()
///     .chunk_size(1000)
///     .chunk_overlap(100);
///
/// let doc = Document::new("Paragraph 1.\n\nParagraph 2 is longer...");
/// let chunks = splitter.split(&doc);
/// ```
#[derive(Debug, Clone)]
pub struct RecursiveCharacterSplitter {
    /// Target chunk size in characters.
    chunk_size: usize,
    /// Number of characters to overlap between chunks.
    chunk_overlap: usize,
    /// Separators to try, in order of preference.
    separators: Vec<String>,
    /// Keep the separator at the end of chunks.
    keep_separator: bool,
}

impl Default for RecursiveCharacterSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl RecursiveCharacterSplitter {
    /// Create a new recursive character splitter with default settings.
    ///
    /// Default: 1000 character chunks with 200 character overlap.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chunk_size: 1000,
            chunk_overlap: 200,
            separators: vec![
                "\n\n".to_string(),
                "\n".to_string(),
                ". ".to_string(),
                "! ".to_string(),
                "? ".to_string(),
                "; ".to_string(),
                ", ".to_string(),
                " ".to_string(),
                String::new(),
            ],
            keep_separator: true,
        }
    }

    /// Set the target chunk size in characters.
    #[must_use]
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set the overlap between chunks in characters.
    #[must_use]
    pub fn chunk_overlap(mut self, overlap: usize) -> Self {
        self.chunk_overlap = overlap;
        self
    }

    /// Set custom separators.
    #[must_use]
    pub fn separators(mut self, separators: Vec<String>) -> Self {
        self.separators = separators;
        self
    }

    /// Set whether to keep separators at the end of chunks.
    #[must_use]
    pub fn keep_separator(mut self, keep: bool) -> Self {
        self.keep_separator = keep;
        self
    }

    /// Split text using the given separator.
    fn split_text_with_separator(&self, text: &str, separator: &str) -> Vec<String> {
        if separator.is_empty() {
            // Character-level split
            return text.chars().map(|c| c.to_string()).collect();
        }

        let parts: Vec<&str> = text.split(separator).collect();
        let mut result = Vec::new();

        for (i, part) in parts.iter().enumerate() {
            if self.keep_separator && i < parts.len() - 1 {
                result.push(format!("{part}{separator}"));
            } else {
                result.push(part.to_string());
            }
        }

        result.into_iter().filter(|s| !s.is_empty()).collect()
    }

    /// Recursively split text to target size.
    fn recursive_split(&self, text: &str, separator_idx: usize) -> Vec<String> {
        if text.len() <= self.chunk_size {
            return vec![text.to_string()];
        }

        if separator_idx >= self.separators.len() {
            // No more separators, force split at chunk_size
            return self.force_split(text);
        }

        let separator = &self.separators[separator_idx];
        let splits = self.split_text_with_separator(text, separator);

        if splits.len() == 1 {
            // Separator not found, try next
            return self.recursive_split(text, separator_idx + 1);
        }

        // Merge splits into chunks of appropriate size
        self.merge_splits(&splits, separator_idx)
    }

    /// Force split text at chunk boundaries.
    fn force_split(&self, text: &str) -> Vec<String> {
        let mut chunks = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;

        while start < chars.len() {
            let end = (start + self.chunk_size).min(chars.len());
            let chunk: String = chars[start..end].iter().collect();
            chunks.push(chunk);

            if end >= chars.len() {
                break;
            }

            // Move start forward, accounting for overlap
            start = end.saturating_sub(self.chunk_overlap);
            if start == 0 && end > 0 {
                start = end;
            }
        }

        chunks
    }

    /// Merge splits into chunks, respecting size limits.
    fn merge_splits(&self, splits: &[String], separator_idx: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current_chunk = String::new();

        for split in splits {
            // If this split alone is too big, recursively split it
            if split.len() > self.chunk_size {
                // First, flush current chunk if not empty
                if !current_chunk.is_empty() {
                    chunks.push(current_chunk.trim().to_string());
                    current_chunk = String::new();
                }

                // Recursively split the oversized piece
                let sub_chunks = self.recursive_split(split, separator_idx + 1);
                chunks.extend(sub_chunks);
                continue;
            }

            // Would adding this exceed the limit?
            if current_chunk.len() + split.len() > self.chunk_size {
                // Save current chunk if not empty
                if !current_chunk.is_empty() {
                    chunks.push(current_chunk.trim().to_string());
                }

                // Start new chunk, potentially with overlap from previous
                if self.chunk_overlap > 0 && !chunks.is_empty() {
                    let last = chunks.last().unwrap();
                    let overlap_start = last.len().saturating_sub(self.chunk_overlap);
                    current_chunk = last[overlap_start..].to_string();
                    if !current_chunk.ends_with(' ') && !split.starts_with(' ') {
                        current_chunk.push(' ');
                    }
                } else {
                    current_chunk = String::new();
                }
            }

            current_chunk.push_str(split);
        }

        // Don't forget the last chunk
        if !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
        }

        chunks.into_iter().filter(|s| !s.is_empty()).collect()
    }
}

impl TextSplitter for RecursiveCharacterSplitter {
    fn split(&self, document: &Document) -> Vec<Document> {
        let chunks = self.recursive_split(&document.content, 0);

        chunks
            .into_iter()
            .enumerate()
            .map(|(i, content)| {
                let mut doc = Document::new(content);
                doc.metadata.clone_from(&document.metadata);
                doc.metadata
                    .insert("chunk_index".to_string(), serde_json::json!(i));
                if let Some(id) = &document.id {
                    doc.metadata
                        .insert("parent_id".to_string(), serde_json::json!(id));
                }
                doc
            })
            .collect()
    }

    fn description(&self) -> String {
        format!(
            "RecursiveCharacterSplitter(size={}, overlap={})",
            self.chunk_size, self.chunk_overlap
        )
    }
}

/// A simple fixed-size character splitter.
///
/// Splits text at exact character boundaries with optional overlap.
/// Simpler than recursive splitting but may cut words mid-way.
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{TextSplitter, FixedSizeSplitter, Document};
///
/// let splitter = FixedSizeSplitter::new(500).with_overlap(50);
/// let doc = Document::new("Long text...");
/// let chunks = splitter.split(&doc);
/// ```
#[derive(Debug, Clone)]
pub struct FixedSizeSplitter {
    /// Chunk size in characters.
    chunk_size: usize,
    /// Overlap between chunks.
    chunk_overlap: usize,
}

impl FixedSizeSplitter {
    /// Create a new fixed-size splitter.
    #[must_use]
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            chunk_overlap: 0,
        }
    }

    /// Set the overlap between chunks.
    #[must_use]
    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.chunk_overlap = overlap;
        self
    }
}

impl TextSplitter for FixedSizeSplitter {
    fn split(&self, document: &Document) -> Vec<Document> {
        let chars: Vec<char> = document.content.chars().collect();
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < chars.len() {
            let end = (start + self.chunk_size).min(chars.len());
            let content: String = chars[start..end].iter().collect();
            chunks.push(content);

            if end >= chars.len() {
                break;
            }

            start = end.saturating_sub(self.chunk_overlap);
            if start == 0 && end > 0 {
                start = end;
            }
        }

        chunks
            .into_iter()
            .enumerate()
            .map(|(i, content)| {
                let mut doc = Document::new(content);
                doc.metadata.clone_from(&document.metadata);
                doc.metadata
                    .insert("chunk_index".to_string(), serde_json::json!(i));
                if let Some(id) = &document.id {
                    doc.metadata
                        .insert("parent_id".to_string(), serde_json::json!(id));
                }
                doc
            })
            .collect()
    }

    fn description(&self) -> String {
        format!(
            "FixedSizeSplitter(size={}, overlap={})",
            self.chunk_size, self.chunk_overlap
        )
    }
}

/// A token-based splitter that estimates token count.
///
/// Splits text based on estimated token count rather than characters.
/// Uses a simple heuristic of ~4 characters per token (for English).
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{TextSplitter, TokenSplitter, Document};
///
/// let splitter = TokenSplitter::new(256).with_overlap(32);
/// let doc = Document::new("Long text...");
/// let chunks = splitter.split(&doc);
/// ```
#[derive(Debug, Clone)]
pub struct TokenSplitter {
    /// Target chunk size in tokens.
    max_tokens: usize,
    /// Overlap between chunks in tokens.
    token_overlap: usize,
    /// Characters per token estimate.
    chars_per_token: f32,
}

impl TokenSplitter {
    /// Create a new token splitter.
    #[must_use]
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            token_overlap: 0,
            chars_per_token: 4.0,
        }
    }

    /// Set the overlap between chunks in tokens.
    #[must_use]
    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.token_overlap = overlap;
        self
    }

    /// Set the characters per token estimate.
    #[must_use]
    pub fn chars_per_token(mut self, ratio: f32) -> Self {
        self.chars_per_token = ratio;
        self
    }

    /// Estimate token count for text.
    #[allow(dead_code)]
    fn estimate_tokens(&self, text: &str) -> usize {
        (text.len() as f32 / self.chars_per_token).ceil() as usize
    }
}

impl TextSplitter for TokenSplitter {
    fn split(&self, document: &Document) -> Vec<Document> {
        // Convert token limits to character limits
        let char_size = (self.max_tokens as f32 * self.chars_per_token) as usize;
        let char_overlap = (self.token_overlap as f32 * self.chars_per_token) as usize;

        let inner = RecursiveCharacterSplitter::new()
            .chunk_size(char_size)
            .chunk_overlap(char_overlap);

        inner.split(document)
    }

    fn description(&self) -> String {
        format!(
            "TokenSplitter(max_tokens={}, overlap={})",
            self.max_tokens, self.token_overlap
        )
    }
}

/// A sentence splitter that preserves sentence boundaries.
///
/// Splits text at sentence boundaries, grouping sentences until
/// the target size is reached.
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{TextSplitter, SentenceSplitter, Document};
///
/// let splitter = SentenceSplitter::new().max_sentences(5);
/// let doc = Document::new("First sentence. Second sentence. Third.");
/// let chunks = splitter.split(&doc);
/// ```
#[derive(Debug, Clone)]
pub struct SentenceSplitter {
    /// Maximum sentences per chunk.
    max_sentences: usize,
    /// Overlap in sentences.
    sentence_overlap: usize,
}

impl Default for SentenceSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl SentenceSplitter {
    /// Create a new sentence splitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_sentences: 5,
            sentence_overlap: 1,
        }
    }

    /// Set the maximum sentences per chunk.
    #[must_use]
    pub fn max_sentences(mut self, max: usize) -> Self {
        self.max_sentences = max;
        self
    }

    /// Set the overlap in sentences.
    #[must_use]
    pub fn with_overlap(mut self, overlap: usize) -> Self {
        self.sentence_overlap = overlap;
        self
    }

    /// Split text into sentences.
    fn split_into_sentences(&self, text: &str) -> Vec<String> {
        let mut sentences = Vec::new();
        let mut current = String::new();

        for c in text.chars() {
            current.push(c);
            if c == '.' || c == '!' || c == '?' {
                // Check if this is likely end of sentence
                sentences.push(current.trim().to_string());
                current = String::new();
            }
        }

        if !current.trim().is_empty() {
            sentences.push(current.trim().to_string());
        }

        sentences
    }
}

impl TextSplitter for SentenceSplitter {
    fn split(&self, document: &Document) -> Vec<Document> {
        let sentences = self.split_into_sentences(&document.content);
        let mut chunks = Vec::new();
        let mut start = 0;

        while start < sentences.len() {
            let end = (start + self.max_sentences).min(sentences.len());
            let chunk_sentences = &sentences[start..end];
            let content = chunk_sentences.join(" ");
            chunks.push(content);

            if end >= sentences.len() {
                break;
            }

            start = end.saturating_sub(self.sentence_overlap);
            if start == 0 && end > 0 {
                start = end;
            }
        }

        chunks
            .into_iter()
            .enumerate()
            .map(|(i, content)| {
                let mut doc = Document::new(content);
                doc.metadata.clone_from(&document.metadata);
                doc.metadata
                    .insert("chunk_index".to_string(), serde_json::json!(i));
                if let Some(id) = &document.id {
                    doc.metadata
                        .insert("parent_id".to_string(), serde_json::json!(id));
                }
                doc
            })
            .collect()
    }

    fn description(&self) -> String {
        format!(
            "SentenceSplitter(max={}, overlap={})",
            self.max_sentences, self.sentence_overlap
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recursive_splitter_basic() {
        let splitter = RecursiveCharacterSplitter::new()
            .chunk_size(50)
            .chunk_overlap(10);

        let doc =
            Document::new("This is a test. It has multiple sentences. Each one is fairly short.");
        let chunks = splitter.split(&doc);

        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.content.len() <= 50 + 20); // Allow some flexibility
        }
    }

    #[test]
    fn test_recursive_splitter_paragraphs() {
        let splitter = RecursiveCharacterSplitter::new()
            .chunk_size(100)
            .chunk_overlap(0);

        let doc = Document::new("Paragraph one.\n\nParagraph two.\n\nParagraph three.");
        let chunks = splitter.split(&doc);

        // Should split on paragraph boundaries
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_recursive_splitter_preserves_metadata() {
        let splitter = RecursiveCharacterSplitter::new().chunk_size(20);

        let doc = Document::new("Short text that will be split into chunks.")
            .with_id("doc-1")
            .with_metadata("source", "test");

        let chunks = splitter.split(&doc);

        for chunk in &chunks {
            assert_eq!(
                chunk.get_metadata("source"),
                Some(&serde_json::json!("test"))
            );
            assert_eq!(
                chunk.get_metadata("parent_id"),
                Some(&serde_json::json!("doc-1"))
            );
            assert!(chunk.get_metadata("chunk_index").is_some());
        }
    }

    #[test]
    fn test_fixed_size_splitter() {
        let splitter = FixedSizeSplitter::new(10).with_overlap(2);

        let doc = Document::new("0123456789ABCDEFGHIJ");
        let chunks = splitter.split(&doc);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].content, "0123456789");
        assert_eq!(chunks[1].content, "89ABCDEFGH"); // Overlap from previous
        assert_eq!(chunks[2].content, "GHIJ");
    }

    #[test]
    fn test_token_splitter() {
        let splitter = TokenSplitter::new(10).with_overlap(2);

        let doc = Document::new(
            "This is a fairly long document that should be split into multiple chunks based on token count.",
        );
        let chunks = splitter.split(&doc);

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_sentence_splitter() {
        let splitter = SentenceSplitter::new().max_sentences(2).with_overlap(1);

        let doc =
            Document::new("First sentence. Second sentence. Third sentence. Fourth sentence.");
        let chunks = splitter.split(&doc);

        assert!(chunks.len() >= 2);
        assert!(chunks[0].content.contains("First"));
    }

    #[test]
    fn test_split_documents() {
        let splitter = FixedSizeSplitter::new(10);

        let docs = vec![Document::new("0123456789AB"), Document::new("XYZ")];

        let chunks = splitter.split_documents(&docs);

        assert_eq!(chunks.len(), 3); // 2 from first doc, 1 from second
    }

    #[test]
    fn test_small_document_no_split() {
        let splitter = RecursiveCharacterSplitter::new().chunk_size(1000);

        let doc = Document::new("Short text.");
        let chunks = splitter.split(&doc);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "Short text.");
    }
}
