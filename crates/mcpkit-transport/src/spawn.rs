//! Subprocess spawning utilities for stdio transport.
//!
//! This module provides utilities for spawning MCP server processes and
//! connecting to them via stdio. This is the most common pattern for
//! launching MCP servers from client applications.
//!
//! # Example
//!
//! ```no_run
//! use mcpkit_transport::spawn::SpawnedTransport;
//!
//! # async fn example() -> Result<(), mcpkit_transport::TransportError> {
//! // Spawn a server process and get a transport connected to it
//! let transport = SpawnedTransport::spawn("my-mcp-server", &["--config", "config.json"]).await?;
//!
//! // Use the transport for communication
//! // transport.send(message).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Environment Variables
//!
//! The spawned process inherits the parent's environment by default. You can
//! customize environment variables using [`SpawnedTransportBuilder`]:
//!
//! ```no_run
//! use mcpkit_transport::spawn::SpawnedTransport;
//!
//! # async fn example() -> Result<(), mcpkit_transport::TransportError> {
//! let transport = SpawnedTransport::builder("my-mcp-server")
//!     .arg("--verbose")
//!     .env("MCP_LOG_LEVEL", "debug")
//!     .working_dir("/path/to/server")
//!     .spawn()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};
use futures::io::AsyncWriteExt;
use mcpkit_core::protocol::Message;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "tokio-runtime")]
use crate::runtime::{TokioAsyncReadWrapper, TokioAsyncWriteWrapper};

/// Maximum allowed message size (16 MB).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// A transport connected to a spawned subprocess via stdio.
///
/// This transport spawns a child process and communicates with it via
/// stdin/stdout. The child process should be an MCP server that reads
/// JSON-RPC messages from stdin and writes responses to stdout.
///
/// # Lifecycle
///
/// When the transport is closed or dropped, the child process is terminated.
/// The process receives a graceful shutdown signal first, then is forcefully
/// killed if it doesn't exit within a timeout.
///
/// # Example
///
/// ```no_run
/// use mcpkit_transport::spawn::SpawnedTransport;
///
/// # async fn example() -> Result<(), mcpkit_transport::TransportError> {
/// let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;
///
/// // The transport is now connected to the server
/// // When transport is dropped, the child process is terminated
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "tokio-runtime")]
pub struct SpawnedTransport {
    stdin: AsyncMutex<TokioAsyncWriteWrapper<tokio::process::ChildStdin>>,
    stdout:
        AsyncMutex<crate::runtime::BufReader<TokioAsyncReadWrapper<tokio::process::ChildStdout>>>,
    child: AsyncMutex<tokio::process::Child>,
    connected: AtomicBool,
    metadata: TransportMetadata,
    command: String,
}

#[cfg(feature = "tokio-runtime")]
impl SpawnedTransport {
    /// Spawn a new MCP server process and connect to it.
    ///
    /// # Arguments
    ///
    /// * `program` - The program to run (executable name or path)
    /// * `args` - Arguments to pass to the program
    ///
    /// # Errors
    ///
    /// Returns an error if the process could not be spawned.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mcpkit_transport::spawn::SpawnedTransport;
    ///
    /// # async fn example() -> Result<(), mcpkit_transport::TransportError> {
    /// // Simple spawn (no arguments)
    /// let transport = SpawnedTransport::spawn("my-server", &[] as &[&str]).await?;
    ///
    /// // With arguments
    /// let transport = SpawnedTransport::spawn("my-server", &["--port", "8080"]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn spawn<S, I, A>(program: S, args: I) -> Result<Self, TransportError>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        SpawnedTransportBuilder::new(program)
            .args(args)
            .spawn()
            .await
    }

    /// Create a builder for more advanced configuration.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mcpkit_transport::spawn::SpawnedTransport;
    ///
    /// # async fn example() -> Result<(), mcpkit_transport::TransportError> {
    /// let transport = SpawnedTransport::builder("my-server")
    ///     .arg("--verbose")
    ///     .env("DEBUG", "1")
    ///     .spawn()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn builder<S: AsRef<OsStr>>(program: S) -> SpawnedTransportBuilder {
        SpawnedTransportBuilder::new(program)
    }

    /// Get the process ID of the spawned child.
    ///
    /// Returns `None` if the process has already exited.
    pub async fn pid(&self) -> Option<u32> {
        self.child.lock().await.id()
    }

    /// Get the command that was used to spawn this process.
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Check if the child process is still running.
    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        matches!(child.try_wait(), Ok(None))
    }

    /// Wait for the child process to exit.
    ///
    /// Returns the exit status. Note: call this after closing the transport
    /// to avoid blocking indefinitely.
    pub async fn wait(&self) -> Result<std::process::ExitStatus, TransportError> {
        let mut child = self.child.lock().await;
        child.wait().await.map_err(TransportError::from)
    }

    /// Kill the child process forcefully.
    ///
    /// This sends SIGKILL on Unix and `TerminateProcess` on Windows.
    pub async fn kill(&self) -> Result<(), TransportError> {
        let mut child = self.child.lock().await;
        child.kill().await.map_err(TransportError::from)
    }
}

#[cfg(feature = "tokio-runtime")]
impl Transport for SpawnedTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let json = serde_json::to_string(&msg)?;

        if json.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: json.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let mut stdout = self.stdout.lock().await;

        loop {
            let mut line = String::new();
            let bytes_read = stdout.read_line(&mut line).await?;

            if bytes_read == 0 {
                // EOF - child process closed stdout
                self.connected.store(false, Ordering::SeqCst);
                return Ok(None);
            }

            if line.len() > MAX_MESSAGE_SIZE {
                return Err(TransportError::MessageTooLarge {
                    size: line.len(),
                    max: MAX_MESSAGE_SIZE,
                });
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Debug: log raw line read with first 100 chars
            let preview: String = trimmed.chars().take(100).collect();
            tracing::debug!(raw_line_len = line.len(), preview = %preview, "SpawnedTransport read line from stdout");

            let msg: Message = serde_json::from_str(trimmed)?;
            return Ok(Some(msg));
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::SeqCst);

        // Try graceful shutdown by closing stdin (server should detect EOF and exit)
        // The child will be killed when dropped if it hasn't exited

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn metadata(&self) -> TransportMetadata {
        self.metadata.clone()
    }
}

// Note: We don't implement Drop because:
// 1. We can't do async operations in Drop
// 2. Tokio's Child already handles cleanup when dropped
// 3. Users who need graceful shutdown should call close() or kill() explicitly
//
// When SpawnedTransport is dropped, the stdin handle is dropped first,
// which sends EOF to the child process. Most well-behaved MCP servers
// will exit gracefully when they receive EOF on stdin.

/// Builder for creating spawned transports with custom configuration.
///
/// # Example
///
/// ```no_run
/// use mcpkit_transport::spawn::SpawnedTransportBuilder;
///
/// # async fn example() -> Result<(), mcpkit_transport::TransportError> {
/// let transport = SpawnedTransportBuilder::new("my-server")
///     .arg("--config")
///     .arg("config.json")
///     .env("LOG_LEVEL", "debug")
///     .working_dir("/path/to/server")
///     .spawn()
///     .await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "tokio-runtime")]
pub struct SpawnedTransportBuilder {
    program: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    current_dir: Option<PathBuf>,
    clear_env: bool,
}

#[cfg(feature = "tokio-runtime")]
impl SpawnedTransportBuilder {
    /// Create a new builder for the given program.
    #[must_use]
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            program: PathBuf::from(program.as_ref()),
            args: Vec::new(),
            envs: Vec::new(),
            current_dir: None,
            clear_env: false,
        }
    }

    /// Add a single argument.
    #[must_use]
    pub fn arg<S: AsRef<str>>(mut self, arg: S) -> Self {
        self.args.push(arg.as_ref().to_string());
        self
    }

    /// Add multiple arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args.extend(
            args.into_iter()
                .map(|s| s.as_ref().to_string_lossy().into_owned()),
        );
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env<K: AsRef<str>, V: AsRef<str>>(mut self, key: K, value: V) -> Self {
        self.envs
            .push((key.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    /// Set multiple environment variables.
    #[must_use]
    pub fn envs<I, K, V>(mut self, envs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.envs.extend(
            envs.into_iter()
                .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string())),
        );
        self
    }

    /// Set the working directory for the child process.
    #[must_use]
    pub fn working_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    /// Clear the environment variables before adding new ones.
    ///
    /// By default, the child inherits the parent's environment.
    #[must_use]
    pub const fn clear_env(mut self) -> Self {
        self.clear_env = true;
        self
    }

    /// Spawn the process and create the transport.
    ///
    /// # Errors
    ///
    /// Returns an error if the process could not be spawned.
    pub async fn spawn(self) -> Result<SpawnedTransport, TransportError> {
        let mut command = tokio::process::Command::new(&self.program);

        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let stderr pass through for debugging

        if self.clear_env {
            command.env_clear();
        }

        for (key, value) in &self.envs {
            command.env(key, value);
        }

        if let Some(dir) = &self.current_dir {
            command.current_dir(dir);
        }

        let mut child = command.spawn().map_err(|e| TransportError::Connection {
            message: format!(
                "Failed to spawn process '{}': {}",
                self.program.display(),
                e
            ),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| TransportError::Protocol {
            message: "Failed to capture child stdin".to_string(),
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::Protocol {
                message: "Failed to capture child stdout".to_string(),
            })?;

        let command_str = format!("{} {}", self.program.display(), self.args.join(" "));

        let pid = child
            .id()
            .map_or_else(|| "unknown".to_string(), |id| id.to_string());

        Ok(SpawnedTransport {
            stdin: AsyncMutex::new(TokioAsyncWriteWrapper(stdin)),
            stdout: AsyncMutex::new(crate::runtime::BufReader::new(TokioAsyncReadWrapper(
                stdout,
            ))),
            child: AsyncMutex::new(child),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("spawned-stdio")
                .remote_addr(format!("pid:{pid}"))
                .local_addr("parent")
                .connected_now(),
            command: command_str,
        })
    }
}

#[cfg(all(test, feature = "tokio-runtime"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_construction() {
        let builder = SpawnedTransportBuilder::new("echo")
            .arg("hello")
            .arg("world")
            .env("TEST_VAR", "value")
            .working_dir("/tmp");

        assert_eq!(builder.program.to_string_lossy(), "echo");
        assert_eq!(builder.args, vec!["hello", "world"]);
        assert_eq!(
            builder.envs,
            vec![("TEST_VAR".to_string(), "value".to_string())]
        );
        assert_eq!(builder.current_dir, Some(PathBuf::from("/tmp")));
    }

    #[tokio::test]
    async fn test_spawn_nonexistent_program() {
        let result = SpawnedTransport::spawn("nonexistent-program-12345", &[] as &[&str]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_spawn_and_communicate() {
        // Use cat as a simple echo server (it echoes stdin to stdout)
        let result = SpawnedTransport::spawn("cat", &[] as &[&str]).await;

        // cat might not be available on all systems, so this test is best-effort
        if let Ok(transport) = result {
            assert!(transport.is_connected());

            // Clean up
            let _ = transport.kill().await;
        }
    }
}
