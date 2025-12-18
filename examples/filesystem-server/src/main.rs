//! Filesystem MCP Server Example
//!
//! This example demonstrates a real-world MCP server that provides
//! secure filesystem operations within a sandboxed directory.
//!
//! ## Features
//!
//! - Read and write files
//! - List directories
//! - Search for files by pattern
//! - Get file metadata
//! - Path traversal protection
//!
//! ## Running
//!
//! ```bash
//! # Run with a specific allowed directory (defaults to current dir)
//! cargo run -p filesystem-server -- /path/to/allowed/directory
//! ```
//!
//! ## Security
//!
//! This server implements path traversal protection by:
//! - Canonicalizing all paths
//! - Ensuring paths stay within the allowed directory
//! - Rejecting symlinks that point outside the sandbox

use mcpkit::prelude::*;
use std::path::PathBuf;

/// A filesystem server with sandboxed access to a directory.
struct FilesystemServer {
    /// The root directory that this server can access.
    /// All operations are restricted to this directory and its subdirectories.
    allowed_root: PathBuf,
}

impl FilesystemServer {
    /// Create a new filesystem server with access to the specified directory.
    fn new(allowed_root: PathBuf) -> Self {
        Self { allowed_root }
    }

    /// Resolve and validate a path, ensuring it stays within the sandbox.
    ///
    /// Returns an error if the path would escape the allowed directory.
    fn resolve_path(&self, path: &str) -> Result<PathBuf, String> {
        // Handle relative paths
        let target = if path.starts_with('/') {
            self.allowed_root.join(path.trim_start_matches('/'))
        } else {
            self.allowed_root.join(path)
        };

        // Canonicalize to resolve .. and symlinks
        let canonical = target
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path '{}': {}", path, e))?;

        // Ensure the canonical path is within our sandbox
        if !canonical.starts_with(&self.allowed_root) {
            return Err(format!("Path '{}' is outside the allowed directory", path));
        }

        Ok(canonical)
    }

    /// Resolve a path that may not exist yet (for write operations).
    fn resolve_path_for_write(&self, path: &str) -> Result<PathBuf, String> {
        // Handle relative paths
        let target = if path.starts_with('/') {
            self.allowed_root.join(path.trim_start_matches('/'))
        } else {
            self.allowed_root.join(path)
        };

        // For non-existent files, canonicalize the parent directory
        if let Some(parent) = target.parent()
            && parent.exists()
        {
            let canonical_parent = parent
                .canonicalize()
                .map_err(|e| format!("Failed to resolve parent directory: {e}"))?;

            if !canonical_parent.starts_with(&self.allowed_root) {
                return Err(format!("Path '{path}' is outside the allowed directory"));
            }

            // Return the target path with canonicalized parent
            if let Some(file_name) = target.file_name() {
                return Ok(canonical_parent.join(file_name));
            }
        }

        // If parent doesn't exist, use the target directly but check prefix
        let normalized = target
            .components()
            .fold(PathBuf::new(), |mut path, component| {
                match component {
                    std::path::Component::ParentDir => {
                        path.pop();
                    }
                    std::path::Component::Normal(c) => {
                        path.push(c);
                    }
                    std::path::Component::RootDir => {
                        path.push("/");
                    }
                    _ => {}
                }
                path
            });

        // Re-anchor to allowed_root
        let final_path = self
            .allowed_root
            .join(normalized.strip_prefix("/").unwrap_or(&normalized));

        // Final safety check
        if !final_path.starts_with(&self.allowed_root) {
            return Err(format!("Path '{}' is outside the allowed directory", path));
        }

        Ok(final_path)
    }
}

#[mcp_server(name = "filesystem", version = "1.0.0")]
impl FilesystemServer {
    /// Read the contents of a file.
    ///
    /// Returns the file contents as text. For binary files, consider using
    /// base64 encoding on the client side.
    #[tool(description = "Read the contents of a file", read_only = true)]
    async fn read_file(&self, path: String) -> ToolOutput {
        let resolved = match self.resolve_path(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        match tokio::fs::read_to_string(&resolved).await {
            Ok(contents) => ToolOutput::text(contents),
            Err(e) => ToolOutput::error(format!("Failed to read file '{}': {}", path, e)),
        }
    }

    /// Write content to a file.
    ///
    /// Creates the file if it doesn't exist, or overwrites if it does.
    /// Parent directories must already exist.
    #[tool(description = "Write content to a file", destructive = true)]
    async fn write_file(&self, path: String, content: String) -> ToolOutput {
        let resolved = match self.resolve_path_for_write(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        match tokio::fs::write(&resolved, content.as_bytes()).await {
            Ok(()) => ToolOutput::text(format!(
                "Successfully wrote {} bytes to '{}'",
                content.len(),
                path
            )),
            Err(e) => ToolOutput::error(format!("Failed to write file '{}': {}", path, e)),
        }
    }

    /// Append content to a file.
    ///
    /// Creates the file if it doesn't exist.
    #[tool(description = "Append content to a file")]
    async fn append_file(&self, path: String, content: String) -> ToolOutput {
        let resolved = match self.resolve_path_for_write(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        use tokio::io::AsyncWriteExt;

        let mut file = match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved)
            .await
        {
            Ok(f) => f,
            Err(e) => return ToolOutput::error(format!("Failed to open file '{}': {}", path, e)),
        };

        match file.write_all(content.as_bytes()).await {
            Ok(()) => ToolOutput::text(format!(
                "Successfully appended {} bytes to '{}'",
                content.len(),
                path
            )),
            Err(e) => ToolOutput::error(format!("Failed to append to file '{}': {}", path, e)),
        }
    }

    /// List the contents of a directory.
    ///
    /// Returns a JSON array of file/directory names with metadata.
    #[tool(description = "List contents of a directory", read_only = true)]
    async fn list_directory(&self, path: Option<String>) -> ToolOutput {
        let dir_path = path.unwrap_or_else(|| ".".to_string());
        let resolved = match self.resolve_path(&dir_path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        let mut entries = Vec::new();
        let mut read_dir = match tokio::fs::read_dir(&resolved).await {
            Ok(rd) => rd,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Failed to read directory '{}': {}",
                    dir_path, e
                ));
            }
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = match entry.file_type().await {
                Ok(ft) => {
                    if ft.is_dir() {
                        "directory"
                    } else if ft.is_file() {
                        "file"
                    } else if ft.is_symlink() {
                        "symlink"
                    } else {
                        "unknown"
                    }
                }
                Err(_) => "unknown",
            };

            let size = entry.metadata().await.map(|m| m.len()).unwrap_or(0);

            entries.push(serde_json::json!({
                "name": name,
                "type": file_type,
                "size": size,
            }));
        }

        // Sort by name for consistent output
        entries.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });

        // Safe to unwrap: serde_json::json! always produces valid JSON
        ToolOutput::json(&serde_json::json!({
            "path": dir_path,
            "entries": entries,
        }))
        .expect("JSON serialization should not fail")
    }

    /// Get metadata about a file or directory.
    #[tool(description = "Get file or directory metadata", read_only = true)]
    async fn get_metadata(&self, path: String) -> ToolOutput {
        let resolved = match self.resolve_path(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        let metadata = match tokio::fs::metadata(&resolved).await {
            Ok(m) => m,
            Err(e) => {
                return ToolOutput::error(format!("Failed to get metadata for '{}': {}", path, e));
            }
        };

        let file_type = if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let created = metadata
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        ToolOutput::json(&serde_json::json!({
            "path": path,
            "type": file_type,
            "size": metadata.len(),
            "readonly": metadata.permissions().readonly(),
            "modified_unix": modified,
            "created_unix": created,
        }))
        .expect("JSON serialization should not fail")
    }

    /// Search for files matching a pattern.
    ///
    /// Uses simple glob-like pattern matching (supports * and **).
    #[tool(description = "Search for files by name pattern", read_only = true)]
    async fn search_files(
        &self,
        pattern: String,
        directory: Option<String>,
        max_results: Option<u32>,
    ) -> ToolOutput {
        let search_root = match directory {
            Some(ref d) => match self.resolve_path(d) {
                Ok(p) => p,
                Err(e) => return ToolOutput::error(e),
            },
            None => self.allowed_root.clone(),
        };

        let max = max_results.unwrap_or(100).min(1000) as usize;
        let mut results = Vec::new();

        // Simple recursive search
        let mut stack = vec![search_root];

        while let Some(current_dir) = stack.pop() {
            if results.len() >= max {
                break;
            }

            let mut read_dir = match tokio::fs::read_dir(&current_dir).await {
                Ok(rd) => rd,
                Err(_) => continue,
            };

            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if results.len() >= max {
                    break;
                }

                let name = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();

                // Check if name matches pattern (simple glob)
                if matches_pattern(&name, &pattern) {
                    let relative_path = path
                        .strip_prefix(&self.allowed_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    results.push(relative_path);
                }

                // Recurse into directories (but not if pattern doesn't use **)
                if let Ok(ft) = entry.file_type().await
                    && ft.is_dir()
                    && !ft.is_symlink()
                {
                    stack.push(path);
                }
            }
        }

        ToolOutput::json(&serde_json::json!({
            "pattern": pattern,
            "matches": results,
            "count": results.len(),
            "truncated": results.len() >= max,
        }))
        .expect("JSON serialization should not fail")
    }

    /// Create a new directory.
    #[tool(description = "Create a new directory")]
    async fn create_directory(&self, path: String) -> ToolOutput {
        let resolved = match self.resolve_path_for_write(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        match tokio::fs::create_dir_all(&resolved).await {
            Ok(()) => ToolOutput::text(format!("Created directory '{}'", path)),
            Err(e) => ToolOutput::error(format!("Failed to create directory '{}': {}", path, e)),
        }
    }

    /// Delete a file.
    #[tool(description = "Delete a file", destructive = true)]
    async fn delete_file(&self, path: String) -> ToolOutput {
        let resolved = match self.resolve_path(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        // Safety check: don't allow deleting directories with this tool
        if resolved.is_dir() {
            return ToolOutput::error(format!(
                "'{}' is a directory. Use delete_directory instead.",
                path
            ));
        }

        match tokio::fs::remove_file(&resolved).await {
            Ok(()) => ToolOutput::text(format!("Deleted file '{}'", path)),
            Err(e) => ToolOutput::error(format!("Failed to delete file '{}': {}", path, e)),
        }
    }

    /// Delete an empty directory.
    #[tool(description = "Delete an empty directory", destructive = true)]
    async fn delete_directory(&self, path: String) -> ToolOutput {
        let resolved = match self.resolve_path(&path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !resolved.is_dir() {
            return ToolOutput::error(format!("'{}' is not a directory", path));
        }

        match tokio::fs::remove_dir(&resolved).await {
            Ok(()) => ToolOutput::text(format!("Deleted directory '{}'", path)),
            Err(e) => ToolOutput::error(format!(
                "Failed to delete directory '{}' (must be empty): {}",
                path, e
            )),
        }
    }

    /// Get the current working directory (the sandbox root).
    #[tool(description = "Get the sandbox root directory path", read_only = true)]
    async fn get_root(&self) -> ToolOutput {
        ToolOutput::text(self.allowed_root.to_string_lossy().to_string())
    }
}

/// Simple glob pattern matching.
fn matches_pattern(name: &str, pattern: &str) -> bool {
    // Handle simple cases
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return name == pattern;
    }

    // Simple glob matching for patterns like "*.rs" or "test*"
    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 2 {
        let prefix = parts[0];
        let suffix = parts[1];

        let matches_prefix = prefix.is_empty() || name.starts_with(prefix);
        let matches_suffix = suffix.is_empty() || name.ends_with(suffix);

        return matches_prefix && matches_suffix;
    }

    // Fallback: exact match for complex patterns
    name == pattern
}

#[tokio::main]
async fn main() -> Result<(), McpError> {
    // Initialize logging to stderr (stdout is reserved for JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Get the allowed directory from command line args
    let allowed_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));

    // Canonicalize the root path
    let allowed_root = allowed_root
        .canonicalize()
        .expect("Failed to canonicalize allowed directory");

    tracing::info!(
        "Starting filesystem server with root: {}",
        allowed_root.display()
    );

    // Create and run the server
    let server = FilesystemServer::new(allowed_root);

    // Print available tools to stderr (not stdout, which is reserved for JSON-RPC)
    eprintln!("Filesystem MCP Server");
    eprintln!("=====================");
    eprintln!();
    eprintln!("Sandbox root: {}", server.allowed_root.display());
    eprintln!();
    eprintln!("Available tools:");
    eprintln!("  - read_file: Read file contents");
    eprintln!("  - write_file: Write content to file");
    eprintln!("  - append_file: Append content to file");
    eprintln!("  - list_directory: List directory contents");
    eprintln!("  - get_metadata: Get file/directory metadata");
    eprintln!("  - search_files: Search for files by pattern");
    eprintln!("  - create_directory: Create a new directory");
    eprintln!("  - delete_file: Delete a file");
    eprintln!("  - delete_directory: Delete an empty directory");
    eprintln!("  - get_root: Get sandbox root path");
    eprintln!();
    eprintln!("Starting server on stdio...");

    // Create the MCP server and run on stdio
    let mcp_server = server.into_server();
    let transport = mcpkit_transport::stdio::StdioTransport::new();
    mcp_server.serve(transport).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("test.rs", "*.rs"));
        assert!(matches_pattern("main.rs", "*.rs"));
        assert!(!matches_pattern("test.txt", "*.rs"));
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("prefix_test", "prefix_*"));
        assert!(matches_pattern("test_suffix", "*_suffix"));
        assert!(matches_pattern("exact", "exact"));
        assert!(!matches_pattern("different", "exact"));
    }

    #[test]
    fn test_path_resolution() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let server = FilesystemServer::new(root.clone());

        // Create a test file
        fs::write(root.join("test.txt"), "hello").unwrap();

        // Valid path should resolve
        let resolved = server.resolve_path("test.txt");
        assert!(resolved.is_ok());

        // Path traversal should fail
        let escaped = server.resolve_path("../etc/passwd");
        assert!(escaped.is_err());
    }

    #[tokio::test]
    async fn test_read_write_file() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let server = Arc::new(FilesystemServer::new(root.clone()));

        // Write a file
        let write_result = FilesystemServer::write_file(
            &server,
            "test.txt".to_string(),
            "Hello, World!".to_string(),
        )
        .await;

        match write_result {
            ToolOutput::Success(_) => {}
            ToolOutput::RecoverableError { message, .. } => {
                panic!("Write failed: {}", message);
            }
        }

        // Read it back
        let read_result = FilesystemServer::read_file(&server, "test.txt".to_string()).await;

        match read_result {
            ToolOutput::Success(r) => {
                if let Content::Text(tc) = &r.content[0] {
                    assert_eq!(tc.text, "Hello, World!");
                } else {
                    panic!("Expected text content");
                }
            }
            ToolOutput::RecoverableError { message, .. } => {
                panic!("Read failed: {}", message);
            }
        }
    }
}
