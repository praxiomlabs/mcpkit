//! Document loaders for ingesting content from various sources.

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::error::{RagError, RagResult};

/// Trait for loading documents from various sources.
///
/// Document loaders convert raw data (files, URLs, APIs) into Documents
/// that can be processed by the RAG pipeline.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::{DocumentLoader, TextLoader};
///
/// let loader = TextLoader::new("data/document.txt");
/// let documents = loader.load().await?;
/// ```
#[async_trait]
pub trait DocumentLoader: Send + Sync {
    /// Load documents from the source.
    ///
    /// Returns a vector of documents loaded from the source.
    async fn load(&self) -> RagResult<Vec<Document>>;

    /// Get a description of the loader for debugging.
    fn description(&self) -> String {
        "DocumentLoader".to_string()
    }
}

/// Load a single text file as a document.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::TextLoader;
///
/// let loader = TextLoader::new("README.md");
/// let docs = loader.load().await?;
/// assert_eq!(docs.len(), 1);
/// ```
pub struct TextLoader {
    path: PathBuf,
    encoding: Option<String>,
}

impl TextLoader {
    /// Create a new text loader for the given file path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            encoding: None,
        }
    }

    /// Set the expected encoding (currently only UTF-8 is supported).
    #[must_use]
    pub fn encoding(mut self, encoding: impl Into<String>) -> Self {
        self.encoding = Some(encoding.into());
        self
    }
}

#[async_trait]
impl DocumentLoader for TextLoader {
    async fn load(&self) -> RagResult<Vec<Document>> {
        let content =
            tokio::fs::read_to_string(&self.path)
                .await
                .map_err(|e| RagError::file(self.path.display().to_string(), e))?;

        let doc = Document::new(content)
            .with_metadata("source", self.path.display().to_string())
            .with_metadata("loader", "TextLoader");

        Ok(vec![doc])
    }

    fn description(&self) -> String {
        format!("TextLoader({})", self.path.display())
    }
}

/// Load all text files from a directory.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::DirectoryLoader;
///
/// let loader = DirectoryLoader::new("docs/")
///     .with_glob("**/*.md")
///     .recursive(true);
///
/// let docs = loader.load().await?;
/// ```
pub struct DirectoryLoader {
    path: PathBuf,
    glob_pattern: Option<String>,
    recursive: bool,
    extensions: Vec<String>,
}

impl DirectoryLoader {
    /// Create a new directory loader.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            glob_pattern: None,
            recursive: false,
            extensions: vec![],
        }
    }

    /// Set a glob pattern for matching files.
    #[must_use]
    pub fn with_glob(mut self, pattern: impl Into<String>) -> Self {
        self.glob_pattern = Some(pattern.into());
        self
    }

    /// Enable recursive directory traversal.
    #[must_use]
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Filter by file extensions (e.g., "txt", "md").
    #[must_use]
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Check if a file matches the configured filters.
    fn matches_filter(&self, path: &Path) -> bool {
        if self.extensions.is_empty() {
            return true;
        }

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.extensions.iter().any(|e| e == ext))
            .unwrap_or(false)
    }

    /// Recursively collect all matching files.
    fn collect_files<'a>(
        &'a self,
        dir: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = RagResult<Vec<PathBuf>>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut files = Vec::new();
            let mut entries = tokio::fs::read_dir(dir)
                .await
                .map_err(|e| RagError::file(dir.display().to_string(), e))?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| RagError::file(dir.display().to_string(), e))?
            {
                let path = entry.path();
                let file_type = entry
                    .file_type()
                    .await
                    .map_err(|e| RagError::file(path.display().to_string(), e))?;

                if file_type.is_file() && self.matches_filter(&path) {
                    files.push(path);
                } else if file_type.is_dir() && self.recursive {
                    let mut sub_files = self.collect_files(&path).await?;
                    files.append(&mut sub_files);
                }
            }

            Ok(files)
        })
    }
}

#[async_trait]
impl DocumentLoader for DirectoryLoader {
    async fn load(&self) -> RagResult<Vec<Document>> {
        let files = self.collect_files(&self.path).await?;

        if files.is_empty() {
            return Err(RagError::NoDocuments);
        }

        let mut documents = Vec::with_capacity(files.len());

        for file_path in files {
            let content = tokio::fs::read_to_string(&file_path)
                .await
                .map_err(|e| RagError::file(file_path.display().to_string(), e))?;

            let doc = Document::new(content)
                .with_metadata("source", file_path.display().to_string())
                .with_metadata("loader", "DirectoryLoader");

            documents.push(doc);
        }

        Ok(documents)
    }

    fn description(&self) -> String {
        format!(
            "DirectoryLoader({}, recursive={})",
            self.path.display(),
            self.recursive
        )
    }
}

/// Load documents from a JSON file.
///
/// Expects either a JSON array of document objects or a single document object.
/// Each object should have a "content" field and optional "metadata" field.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_rag::JsonLoader;
///
/// // File contents: [{"content": "doc 1"}, {"content": "doc 2", "metadata": {"source": "api"}}]
/// let loader = JsonLoader::new("documents.json");
/// let docs = loader.load().await?;
/// ```
pub struct JsonLoader {
    path: PathBuf,
    content_key: String,
    metadata_key: Option<String>,
}

impl JsonLoader {
    /// Create a new JSON loader.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            content_key: "content".to_string(),
            metadata_key: Some("metadata".to_string()),
        }
    }

    /// Set the key for extracting content from JSON objects.
    #[must_use]
    pub fn content_key(mut self, key: impl Into<String>) -> Self {
        self.content_key = key.into();
        self
    }

    /// Set the key for extracting metadata from JSON objects.
    #[must_use]
    pub fn metadata_key(mut self, key: impl Into<String>) -> Self {
        self.metadata_key = Some(key.into());
        self
    }

    /// Parse a single JSON object into a document.
    fn parse_object(&self, obj: &serde_json::Map<String, serde_json::Value>) -> RagResult<Document> {
        let content = obj
            .get(&self.content_key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RagError::load(format!(
                    "Missing or invalid '{}' field in JSON object",
                    self.content_key
                ))
            })?;

        let mut doc = Document::new(content);

        // Extract metadata if present
        if let Some(meta_key) = &self.metadata_key {
            if let Some(meta) = obj.get(meta_key).and_then(|v| v.as_object()) {
                for (k, v) in meta {
                    doc.metadata.insert(k.clone(), v.clone());
                }
            }
        }

        // Add source metadata
        doc.metadata.insert(
            "source".to_string(),
            serde_json::json!(self.path.display().to_string()),
        );
        doc.metadata
            .insert("loader".to_string(), serde_json::json!("JsonLoader"));

        Ok(doc)
    }
}

#[async_trait]
impl DocumentLoader for JsonLoader {
    async fn load(&self) -> RagResult<Vec<Document>> {
        let content =
            tokio::fs::read_to_string(&self.path)
                .await
                .map_err(|e| RagError::file(self.path.display().to_string(), e))?;

        let json: serde_json::Value = serde_json::from_str(&content)?;

        match json {
            serde_json::Value::Array(arr) => {
                let mut documents = Vec::with_capacity(arr.len());
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        documents.push(self.parse_object(obj)?);
                    }
                }
                Ok(documents)
            }
            serde_json::Value::Object(obj) => Ok(vec![self.parse_object(&obj)?]),
            _ => Err(RagError::load("JSON must be an array or object")),
        }
    }

    fn description(&self) -> String {
        format!("JsonLoader({})", self.path.display())
    }
}

/// Load documents from in-memory data.
///
/// Useful for testing or when documents are already loaded.
///
/// # Example
///
/// ```rust
/// use mcpkit_rag::{DocumentLoader, MemoryLoader, Document};
///
/// # tokio_test::block_on(async {
/// let docs = vec![
///     Document::new("First document"),
///     Document::new("Second document"),
/// ];
///
/// let loader = MemoryLoader::new(docs);
/// let loaded = loader.load().await.unwrap();
/// assert_eq!(loaded.len(), 2);
/// # });
/// ```
pub struct MemoryLoader {
    documents: Vec<Document>,
}

impl MemoryLoader {
    /// Create a new memory loader with the given documents.
    pub fn new(documents: Vec<Document>) -> Self {
        Self { documents }
    }

    /// Create from string contents.
    pub fn from_texts(texts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            documents: texts.into_iter().map(|t| Document::new(t)).collect(),
        }
    }
}

#[async_trait]
impl DocumentLoader for MemoryLoader {
    async fn load(&self) -> RagResult<Vec<Document>> {
        Ok(self.documents.clone())
    }

    fn description(&self) -> String {
        format!("MemoryLoader({} documents)", self.documents.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_text_loader() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Hello, World!").unwrap();

        let loader = TextLoader::new(file.path());
        let docs = loader.load().await.unwrap();

        assert_eq!(docs.len(), 1);
        assert!(docs[0].content.contains("Hello, World!"));
        assert!(docs[0].get_metadata("source").is_some());
    }

    #[tokio::test]
    async fn test_json_loader_array() {
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(
            file,
            r#"[{{"content": "First doc"}}, {{"content": "Second doc", "metadata": {{"key": "value"}}}}]"#
        )
        .unwrap();

        let loader = JsonLoader::new(file.path());
        let docs = loader.load().await.unwrap();

        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].content, "First doc");
        assert_eq!(docs[1].content, "Second doc");
        assert_eq!(docs[1].get_metadata("key"), Some(&serde_json::json!("value")));
    }

    #[tokio::test]
    async fn test_json_loader_single_object() {
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        writeln!(file, r#"{{"content": "Single document"}}"#).unwrap();

        let loader = JsonLoader::new(file.path());
        let docs = loader.load().await.unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].content, "Single document");
    }

    #[tokio::test]
    async fn test_memory_loader() {
        let loader = MemoryLoader::from_texts(vec!["Doc 1", "Doc 2", "Doc 3"]);
        let docs = loader.load().await.unwrap();

        assert_eq!(docs.len(), 3);
        assert_eq!(docs[0].content, "Doc 1");
    }

    #[tokio::test]
    async fn test_directory_loader() {
        let dir = tempfile::tempdir().unwrap();

        // Create test files
        let file1 = dir.path().join("test1.txt");
        let file2 = dir.path().join("test2.txt");
        tokio::fs::write(&file1, "Content 1").await.unwrap();
        tokio::fs::write(&file2, "Content 2").await.unwrap();

        let loader = DirectoryLoader::new(dir.path()).with_extensions(vec!["txt".to_string()]);
        let docs = loader.load().await.unwrap();

        assert_eq!(docs.len(), 2);
    }
}
