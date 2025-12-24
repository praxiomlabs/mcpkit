//! Runtime abstraction layer for async I/O.
//!
//! This module provides runtime-agnostic abstractions over async primitives,
//! allowing the transport layer to work with Tokio or smol.
//!
//! # Design Philosophy
//!
//! Per the [Rust Async Book](https://rust-lang.github.io/async-book/08_ecosystem/00_chapter.html):
//! > "Libraries exposing async APIs should not depend on a specific executor or reactor,
//! > unless they need to spawn tasks or define their own async I/O or timer futures."
//!
//! This module provides the necessary abstractions to achieve that goal.
//!
//! # Usage
//!
//! Enable one of the runtime features:
//! - `tokio-runtime` (default)
//! - `smol-runtime`

use futures::io::{AsyncRead, AsyncWrite};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

// =============================================================================
// Mutex Abstraction
// =============================================================================

/// A runtime-agnostic async mutex.
///
/// Uses `async-lock` for runtime-agnostic behavior across all async runtimes.
pub use async_lock::Mutex as AsyncMutex;

/// A runtime-agnostic async `RwLock`.
///
/// Uses `async-lock` for runtime-agnostic behavior across all async runtimes.
/// This is preferred over `tokio::sync::RwLock` when runtime agnosticism is needed.
pub use async_lock::RwLock as AsyncRwLock;

/// A runtime-agnostic semaphore.
///
/// Uses `async-lock` for runtime-agnostic behavior across all async runtimes.
pub use async_lock::Semaphore as AsyncSemaphore;

/// A runtime-agnostic semaphore guard.
pub use async_lock::SemaphoreGuard as AsyncSemaphoreGuard;

/// A runtime-agnostic event notification mechanism.
///
/// Uses `event-listener` (via `async-lock`) for runtime-agnostic behavior.
/// This is useful for signaling waiters when a resource becomes available.
pub use event_listener::Event as Notify;

// =============================================================================
// Channel Abstraction
// =============================================================================

/// A runtime-agnostic bounded MPSC channel sender.
#[cfg(feature = "tokio-runtime")]
pub type Sender<T> = tokio::sync::mpsc::Sender<T>;

/// A runtime-agnostic bounded MPSC channel sender.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub type Sender<T> = smol::channel::Sender<T>;

/// A runtime-agnostic bounded MPSC channel receiver.
#[cfg(feature = "tokio-runtime")]
pub type Receiver<T> = tokio::sync::mpsc::Receiver<T>;

/// A runtime-agnostic bounded MPSC channel receiver.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub type Receiver<T> = smol::channel::Receiver<T>;

/// Create a bounded channel.
#[cfg(feature = "tokio-runtime")]
#[must_use]
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    tokio::sync::mpsc::channel(capacity)
}

/// Create a bounded channel.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    smol::channel::bounded(capacity)
}

// =============================================================================
// Stdio Abstraction
// =============================================================================

/// Runtime-agnostic stdin.
#[cfg(feature = "tokio-runtime")]
#[must_use]
pub fn stdin() -> impl AsyncRead + Unpin {
    TokioAsyncReadWrapper(tokio::io::stdin())
}

/// Runtime-agnostic stdin.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub fn stdin() -> impl AsyncRead + Unpin {
    smol::Unblock::new(std::io::stdin())
}

/// Runtime-agnostic stdout.
#[cfg(feature = "tokio-runtime")]
#[must_use]
pub fn stdout() -> impl AsyncWrite + Unpin {
    TokioAsyncWriteWrapper(tokio::io::stdout())
}

/// Runtime-agnostic stdout.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub fn stdout() -> impl AsyncWrite + Unpin {
    smol::Unblock::new(std::io::stdout())
}

// =============================================================================
// Tokio Compatibility Wrappers
// =============================================================================

/// Wrapper to convert Tokio's `AsyncRead` to `futures::io::AsyncRead`
#[cfg(feature = "tokio-runtime")]
pub struct TokioAsyncReadWrapper<T>(pub T);

#[cfg(feature = "tokio-runtime")]
impl<T: tokio::io::AsyncRead + Unpin> AsyncRead for TokioAsyncReadWrapper<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let mut read_buf = tokio::io::ReadBuf::new(buf);
        match Pin::new(&mut self.0).poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrapper to convert Tokio's `AsyncWrite` to `futures::io::AsyncWrite`
#[cfg(feature = "tokio-runtime")]
pub struct TokioAsyncWriteWrapper<T>(pub T);

#[cfg(feature = "tokio-runtime")]
impl<T: tokio::io::AsyncWrite + Unpin> AsyncWrite for TokioAsyncWriteWrapper<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

// =============================================================================
// Spawn Abstraction
// =============================================================================

/// Spawn a future on the runtime.
///
/// Note: This requires `'static` bound. Use sparingly - prefer passing
/// futures through channels or letting the caller handle spawning.
#[cfg(feature = "tokio-runtime")]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

/// Spawn a future on the runtime.
///
/// Note: This requires `'static` bound. Use sparingly - prefer passing
/// futures through channels or letting the caller handle spawning.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    smol::spawn(future).detach();
}

// =============================================================================
// Sleep Abstraction
// =============================================================================

/// Sleep for the given duration.
#[cfg(feature = "tokio-runtime")]
pub async fn sleep(duration: std::time::Duration) {
    tokio::time::sleep(duration).await;
}

/// Sleep for the given duration.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub async fn sleep(duration: std::time::Duration) {
    smol::Timer::after(duration).await;
}

// =============================================================================
// Timeout Abstraction
// =============================================================================

/// Apply a timeout to a future.
#[cfg(feature = "tokio-runtime")]
pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| TimeoutError)
}

/// Apply a timeout to a future.
#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime")))]
pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    use futures::future::FutureExt;

    let sleep = smol::Timer::after(duration);
    futures::select! {
        result = future.fuse() => Ok(result),
        _ = sleep.fuse() => Err(TimeoutError),
    }
}

/// Error returned when a timeout expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

// =============================================================================
// BufReader/BufWriter Abstraction
// =============================================================================

use bytes::{Bytes, BytesMut};

/// A runtime-agnostic buffered reader with zero-copy support.
///
/// This implementation uses `BytesMut` internally for efficient buffer management
/// and provides both `String`-based and `Bytes`-based reading methods for
/// flexibility between convenience and performance.
pub struct BufReader<R> {
    inner: R,
    buffer: BytesMut,
    capacity: usize,
}

impl<R> BufReader<R> {
    /// Create a new buffered reader with the default buffer size (8KB).
    pub fn new(inner: R) -> Self {
        Self::with_capacity(8192, inner)
    }

    /// Create a new buffered reader with a specific buffer size.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buffer: BytesMut::with_capacity(capacity),
            capacity,
        }
    }

    /// Get a reference to the underlying reader.
    pub const fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Get a mutable reference to the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Returns the number of bytes currently buffered.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }
}

impl<R: AsyncRead + Unpin> BufReader<R> {
    /// Read a line from the reader into a `String`.
    ///
    /// Returns the number of bytes read (including the newline).
    /// Returns 0 on EOF.
    ///
    /// For zero-copy scenarios, consider using [`read_line_bytes`](Self::read_line_bytes)
    /// which returns `Bytes` directly without UTF-8 conversion overhead.
    pub async fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        let bytes = self.read_line_bytes().await?;
        let len = bytes.len();
        if len > 0 {
            // Convert to string, using lossy conversion for robustness
            line.push_str(&String::from_utf8_lossy(&bytes));
        }
        Ok(len)
    }

    /// Read a line from the reader as `Bytes` (zero-copy when possible).
    ///
    /// This method is more efficient than [`read_line`](Self::read_line) when:
    /// - You need to parse the line as JSON directly (using `serde_json::from_slice`)
    /// - You're processing binary protocols
    /// - You want to avoid UTF-8 validation overhead
    ///
    /// Returns an empty `Bytes` on EOF.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcpkit_transport::runtime::BufReader;
    ///
    /// let mut reader = BufReader::new(some_reader);
    /// let line_bytes = reader.read_line_bytes().await?;
    /// if !line_bytes.is_empty() {
    ///     // Parse JSON directly from bytes - no intermediate String allocation
    ///     let message: Message = serde_json::from_slice(&line_bytes)?;
    /// }
    /// ```
    pub async fn read_line_bytes(&mut self) -> io::Result<Bytes> {
        use futures::io::AsyncReadExt;

        // Accumulator for lines that span multiple buffer reads
        let mut line_buf: Option<BytesMut> = None;

        loop {
            // Look for newline in current buffer
            if let Some(newline_pos) = self.buffer.iter().position(|&b| b == b'\n') {
                // Found a newline - split the buffer at this position
                let line_with_newline = self.buffer.split_to(newline_pos + 1);

                // If we had accumulated data from previous reads, append to it
                if let Some(mut accumulated) = line_buf.take() {
                    accumulated.extend_from_slice(&line_with_newline);
                    return Ok(accumulated.freeze());
                }

                // Otherwise return the line directly (zero-copy path)
                return Ok(line_with_newline.freeze());
            }

            // No newline found - save current buffer contents and read more
            if !self.buffer.is_empty() {
                let current = self.buffer.split();
                match &mut line_buf {
                    Some(accumulated) => accumulated.extend_from_slice(&current),
                    None => line_buf = Some(current),
                }
            }

            // IMPORTANT: Clear buffer state BEFORE the await for cancellation safety.
            // If cancelled during read, next call sees empty buffer and refills cleanly.
            self.buffer.clear();
            self.buffer.reserve(self.capacity);

            // Read more data into the buffer
            // SAFETY: We just reserved capacity, so this won't reallocate
            let spare = self.buffer.spare_capacity_mut();
            // SAFETY: spare_capacity_mut returns uninitialized memory, but read() will
            // initialize it. We use a temporary buffer and then extend.
            let mut temp_buf = vec![0u8; spare.len().min(self.capacity)];
            let n = self.inner.read(&mut temp_buf).await?;

            if n == 0 {
                // EOF - return any accumulated data
                return Ok(line_buf.map_or_else(Bytes::new, BytesMut::freeze));
            }

            self.buffer.extend_from_slice(&temp_buf[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_error_display() {
        let err = TimeoutError;
        assert_eq!(err.to_string(), "operation timed out");
    }

    /// Test that `BufReader` doesn't duplicate data when futures are cancelled.
    ///
    /// This is a regression test for a bug where cancelling a `read_line` future
    /// during buffer refill would cause the same data to be read twice.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_cancellation_safety() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        // Create test data with multiple lines
        let data = b"line1\nline2\nline3\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        // Read first line normally
        let mut line1 = String::new();
        let n1 = reader.read_line(&mut line1).await?;
        assert_eq!(n1, 6);
        assert_eq!(line1, "line1\n");

        // Simulate what happens in tokio::select! - start reading but cancel quickly
        // We can't perfectly simulate cancellation mid-await, but we can verify
        // that consecutive reads work correctly
        let mut line2 = String::new();
        let n2 = reader.read_line(&mut line2).await?;
        assert_eq!(n2, 6);
        assert_eq!(line2, "line2\n");

        // Verify no duplication - third line should be "line3", not "line2" again
        let mut line3 = String::new();
        let n3 = reader.read_line(&mut line3).await?;
        assert_eq!(n3, 6);
        assert_eq!(line3, "line3\n");

        // EOF should return 0
        let mut eof = String::new();
        let n4 = reader.read_line(&mut eof).await?;
        assert_eq!(n4, 0);
        assert_eq!(eof, "");
        Ok(())
    }

    /// Test `BufReader` handles partial buffer consumption correctly.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_partial_buffer() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        // Create data where multiple lines fit in one buffer read
        let data = b"short\nlonger line here\nx\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        let mut line1 = String::new();
        reader.read_line(&mut line1).await?;
        assert_eq!(line1, "short\n");

        let mut line2 = String::new();
        reader.read_line(&mut line2).await?;
        assert_eq!(line2, "longer line here\n");

        let mut line3 = String::new();
        reader.read_line(&mut line3).await?;
        assert_eq!(line3, "x\n");
        Ok(())
    }

    /// Test that `BufReader` handles empty lines correctly.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_empty_lines() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        let data = b"first\n\nsecond\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        let mut line1 = String::new();
        reader.read_line(&mut line1).await?;
        assert_eq!(line1, "first\n");

        let mut line2 = String::new();
        reader.read_line(&mut line2).await?;
        assert_eq!(line2, "\n"); // Empty line (just newline)

        let mut line3 = String::new();
        reader.read_line(&mut line3).await?;
        assert_eq!(line3, "second\n");
        Ok(())
    }

    /// Test `read_line_bytes` returns `Bytes` directly.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_read_line_bytes() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        let data = b"hello\nworld\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        let line1 = reader.read_line_bytes().await?;
        assert_eq!(&line1[..], b"hello\n");

        let line2 = reader.read_line_bytes().await?;
        assert_eq!(&line2[..], b"world\n");

        // EOF returns empty bytes
        let eof = reader.read_line_bytes().await?;
        assert!(eof.is_empty());
        Ok(())
    }

    /// Test that JSON can be parsed directly from `Bytes` without intermediate String.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_json_from_bytes() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        // Parse JSON directly from bytes - zero-copy path
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            value: i32,
        }

        let json_data = b"{\"name\":\"test\",\"value\":42}\n";
        let cursor = Cursor::new(json_data.to_vec());
        let mut reader = BufReader::new(cursor);

        let line_bytes = reader.read_line_bytes().await?;

        // Trim the newline for JSON parsing
        let trimmed = line_bytes.strip_suffix(b"\n").unwrap_or(&line_bytes);
        let parsed: TestData = serde_json::from_slice(trimmed)?;

        assert_eq!(
            parsed,
            TestData {
                name: "test".to_string(),
                value: 42
            }
        );
        Ok(())
    }

    /// Test `buffered()` returns correct count.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_buffered() -> Result<(), Box<dyn std::error::Error>> {
        use futures::io::Cursor;

        let data = b"line1\nline2\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        // Initially empty
        assert_eq!(reader.buffered(), 0);

        // After reading first line, buffer may contain remaining data (line2\n)
        let mut line1 = String::new();
        reader.read_line(&mut line1).await?;
        // After reading "line1\n", the buffer should contain "line2\n" (6 bytes)
        assert_eq!(reader.buffered(), 6);

        // After reading second line, buffer should be empty
        let mut line2 = String::new();
        reader.read_line(&mut line2).await?;
        assert_eq!(reader.buffered(), 0);
        Ok(())
    }
}
