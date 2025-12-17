//! Runtime abstraction layer for async I/O.
//!
//! This module provides runtime-agnostic abstractions over async primitives,
//! allowing the transport layer to work with Tokio, async-std, or smol.
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
//! - `async-std-runtime`
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

// =============================================================================
// Channel Abstraction
// =============================================================================

/// A runtime-agnostic bounded MPSC channel sender.
#[cfg(feature = "tokio-runtime")]
pub type Sender<T> = tokio::sync::mpsc::Sender<T>;

/// A runtime-agnostic bounded MPSC channel sender.
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub type Sender<T> = async_std::channel::Sender<T>;

/// A runtime-agnostic bounded MPSC channel sender.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
pub type Sender<T> = smol::channel::Sender<T>;

/// A runtime-agnostic bounded MPSC channel receiver.
#[cfg(feature = "tokio-runtime")]
pub type Receiver<T> = tokio::sync::mpsc::Receiver<T>;

/// A runtime-agnostic bounded MPSC channel receiver.
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub type Receiver<T> = async_std::channel::Receiver<T>;

/// A runtime-agnostic bounded MPSC channel receiver.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
pub type Receiver<T> = smol::channel::Receiver<T>;

/// Create a bounded channel.
#[cfg(feature = "tokio-runtime")]
#[must_use]
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    tokio::sync::mpsc::channel(capacity)
}

/// Create a bounded channel.
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    async_std::channel::bounded(capacity)
}

/// Create a bounded channel.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub fn stdin() -> impl AsyncRead + Unpin {
    async_std::io::stdin()
}

/// Runtime-agnostic stdin.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub fn stdout() -> impl AsyncWrite + Unpin {
    async_std::io::stdout()
}

/// Runtime-agnostic stdout.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    async_std::task::spawn(future);
}

/// Spawn a future on the runtime.
///
/// Note: This requires `'static` bound. Use sparingly - prefer passing
/// futures through channels or letting the caller handle spawning.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub async fn sleep(duration: std::time::Duration) {
    async_std::task::sleep(duration).await;
}

/// Sleep for the given duration.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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
#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    async_std::future::timeout(duration, future)
        .await
        .map_err(|_| TimeoutError)
}

/// Apply a timeout to a future.
#[cfg(all(
    feature = "smol-runtime",
    not(feature = "tokio-runtime"),
    not(feature = "async-std-runtime")
))]
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

/// A runtime-agnostic buffered reader.
pub struct BufReader<R> {
    inner: R,
    buffer: Vec<u8>,
    pos: usize,
    filled: usize,
}

impl<R> BufReader<R> {
    /// Create a new buffered reader with the default buffer size.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(8192, inner)
    }

    /// Create a new buffered reader with a specific buffer size.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buffer: vec![0; capacity],
            pos: 0,
            filled: 0,
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
}

impl<R: AsyncRead + Unpin> BufReader<R> {
    /// Read a line from the reader.
    ///
    /// Returns the number of bytes read (including the newline).
    /// Returns 0 on EOF.
    pub async fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        use futures::io::AsyncReadExt;

        let mut total_read = 0;

        loop {
            // Check if we have buffered data
            if self.pos < self.filled {
                // Look for newline in buffer
                if let Some(newline_pos) = self.buffer[self.pos..self.filled]
                    .iter()
                    .position(|&b| b == b'\n')
                {
                    let end = self.pos + newline_pos + 1;
                    let bytes = &self.buffer[self.pos..end];
                    line.push_str(&String::from_utf8_lossy(bytes));
                    total_read += bytes.len();
                    self.pos = end;
                    return Ok(total_read);
                }

                // No newline, consume all buffered data
                let bytes = &self.buffer[self.pos..self.filled];
                line.push_str(&String::from_utf8_lossy(bytes));
                total_read += bytes.len();
                self.pos = self.filled;
            }

            // Refill buffer
            // IMPORTANT: Mark buffer as empty BEFORE the await to ensure cancellation safety.
            // If the future is cancelled by tokio::select! during the read, the next call
            // will see an empty buffer (pos=0, filled=0) and refill it, avoiding data duplication.
            self.filled = 0;
            self.pos = 0;
            let n = self.inner.read(&mut self.buffer).await?;
            self.filled = n;

            if n == 0 {
                // EOF
                return Ok(total_read);
            }
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
    async fn test_bufreader_cancellation_safety() {
        use futures::io::Cursor;

        // Create test data with multiple lines
        let data = b"line1\nline2\nline3\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        // Read first line normally
        let mut line1 = String::new();
        let n1 = reader.read_line(&mut line1).await.unwrap();
        assert_eq!(n1, 6);
        assert_eq!(line1, "line1\n");

        // Simulate what happens in tokio::select! - start reading but cancel quickly
        // We can't perfectly simulate cancellation mid-await, but we can verify
        // that consecutive reads work correctly
        let mut line2 = String::new();
        let n2 = reader.read_line(&mut line2).await.unwrap();
        assert_eq!(n2, 6);
        assert_eq!(line2, "line2\n");

        // Verify no duplication - third line should be "line3", not "line2" again
        let mut line3 = String::new();
        let n3 = reader.read_line(&mut line3).await.unwrap();
        assert_eq!(n3, 6);
        assert_eq!(line3, "line3\n");

        // EOF should return 0
        let mut eof = String::new();
        let n4 = reader.read_line(&mut eof).await.unwrap();
        assert_eq!(n4, 0);
        assert_eq!(eof, "");
    }

    /// Test `BufReader` handles partial buffer consumption correctly.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_partial_buffer() {
        use futures::io::Cursor;

        // Create data where multiple lines fit in one buffer read
        let data = b"short\nlonger line here\nx\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        let mut line1 = String::new();
        reader.read_line(&mut line1).await.unwrap();
        assert_eq!(line1, "short\n");

        let mut line2 = String::new();
        reader.read_line(&mut line2).await.unwrap();
        assert_eq!(line2, "longer line here\n");

        let mut line3 = String::new();
        reader.read_line(&mut line3).await.unwrap();
        assert_eq!(line3, "x\n");
    }

    /// Test that `BufReader` handles empty lines correctly.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_bufreader_empty_lines() {
        use futures::io::Cursor;

        let data = b"first\n\nsecond\n";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = BufReader::new(cursor);

        let mut line1 = String::new();
        reader.read_line(&mut line1).await.unwrap();
        assert_eq!(line1, "first\n");

        let mut line2 = String::new();
        reader.read_line(&mut line2).await.unwrap();
        assert_eq!(line2, "\n"); // Empty line (just newline)

        let mut line3 = String::new();
        reader.read_line(&mut line3).await.unwrap();
        assert_eq!(line3, "second\n");
    }
}
