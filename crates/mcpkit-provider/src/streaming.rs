//! Streaming support for LLM completions.
//!
//! This module provides types for handling streaming responses from LLM providers,
//! with support for backpressure, cancellation, and state tracking.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

use futures::Stream;
use pin_project_lite::pin_project;

use crate::error::{ProviderError, ProviderResult};
use crate::types::{ContentBlock, FinishReason, StreamEvent, Usage};

/// State of a streaming completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StreamState {
    /// Stream has not started yet.
    NotStarted = 0,
    /// Stream is actively receiving data.
    Streaming = 1,
    /// Stream completed successfully.
    Completed = 2,
    /// Stream was cancelled.
    Cancelled = 3,
    /// Stream encountered an error.
    Error = 4,
}

impl StreamState {
    /// Check if the stream is still active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::NotStarted | Self::Streaming)
    }

    /// Check if the stream has finished (successfully or not).
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Error)
    }
}

pin_project! {
    /// A streaming completion from an LLM provider.
    ///
    /// This type wraps a stream of [`StreamEvent`]s and provides utilities for
    /// collecting the full response, handling cancellation, and tracking state.
    pub struct CompletionStream {
        #[pin]
        inner: Pin<Box<dyn Stream<Item = ProviderResult<StreamEvent>> + Send>>,
        state: Arc<AtomicU8>,
        collected_content: Vec<ContentBlock>,
        model: Option<String>,
        id: Option<String>,
        usage: Usage,
        finish_reason: Option<FinishReason>,
    }
}

impl CompletionStream {
    /// Create a new completion stream.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = ProviderResult<StreamEvent>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
            state: Arc::new(AtomicU8::new(StreamState::NotStarted as u8)),
            collected_content: Vec::new(),
            model: None,
            id: None,
            usage: Usage::new(),
            finish_reason: None,
        }
    }

    /// Get the current state of the stream.
    #[must_use]
    pub fn state(&self) -> StreamState {
        match self.state.load(Ordering::Acquire) {
            0 => StreamState::NotStarted,
            1 => StreamState::Streaming,
            2 => StreamState::Completed,
            3 => StreamState::Cancelled,
            _ => StreamState::Error,
        }
    }

    /// Cancel the stream.
    pub fn cancel(&self) {
        self.state
            .store(StreamState::Cancelled as u8, Ordering::Release);
    }

    /// Check if the stream has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state() == StreamState::Cancelled
    }

    /// Get a handle for cancellation.
    #[must_use]
    pub fn cancellation_handle(&self) -> CancellationHandle {
        CancellationHandle {
            state: Arc::clone(&self.state),
        }
    }

    /// Collect the full response from the stream.
    ///
    /// This consumes the stream and returns the complete response.
    ///
    /// # Errors
    ///
    /// Returns an error if any event in the stream is an error.
    pub async fn collect(mut self) -> ProviderResult<CollectedResponse> {
        use futures::StreamExt;

        // Track accumulated JSON for each tool use block (by index)
        let mut tool_json_buffers: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();

        while let Some(event) = self.next().await {
            match event? {
                StreamEvent::Start { id, model } => {
                    self.id = Some(id);
                    self.model = Some(model);
                }
                StreamEvent::ToolUseStart { index, id, name } => {
                    // Ensure we have space for this content block
                    while self.collected_content.len() <= index {
                        self.collected_content.push(ContentBlock::text(""));
                    }

                    // Initialize the tool use block with empty input
                    self.collected_content[index] = ContentBlock::ToolUse {
                        id,
                        name,
                        input: serde_json::Value::Null,
                    };

                    // Initialize the JSON buffer for this tool
                    tool_json_buffers.insert(index, String::new());
                }
                StreamEvent::ContentDelta { index, delta } => {
                    // Ensure we have a content block at this index
                    while self.collected_content.len() <= index {
                        self.collected_content.push(ContentBlock::text(""));
                    }

                    // Append delta to content block
                    match &delta {
                        crate::types::ContentDelta::Text { text } => {
                            if let Some(ContentBlock::Text { text: existing }) =
                                self.collected_content.get_mut(index)
                            {
                                existing.push_str(text);
                            }
                        }
                        crate::types::ContentDelta::ToolInput { partial_json } => {
                            // Accumulate the partial JSON into the buffer
                            if let Some(buffer) = tool_json_buffers.get_mut(&index) {
                                buffer.push_str(partial_json);
                            }
                        }
                    }
                }
                StreamEvent::Stop {
                    finish_reason,
                    usage,
                } => {
                    self.finish_reason = Some(finish_reason);
                    self.usage = usage;

                    // Parse accumulated JSON for all tool use blocks
                    for (index, json_str) in &tool_json_buffers {
                        if let Some(ContentBlock::ToolUse { input, .. }) =
                            self.collected_content.get_mut(*index)
                        {
                            // Try to parse the accumulated JSON
                            *input = serde_json::from_str(json_str).unwrap_or_else(|_| {
                                // If parsing fails, store as a string value
                                if json_str.is_empty() {
                                    serde_json::Value::Object(serde_json::Map::new())
                                } else {
                                    serde_json::Value::String(json_str.clone())
                                }
                            });
                        }
                    }
                }
                StreamEvent::Error { message } => {
                    return Err(ProviderError::StreamInterrupted { message });
                }
            }
        }

        Ok(CollectedResponse {
            id: self.id.unwrap_or_default(),
            model: self.model.unwrap_or_default(),
            content: self.collected_content,
            finish_reason: self.finish_reason.unwrap_or(FinishReason::Stop),
            usage: self.usage,
        })
    }
}

impl Stream for CompletionStream {
    type Item = ProviderResult<StreamEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        // Check for cancellation
        if this.state.load(Ordering::Acquire) == StreamState::Cancelled as u8 {
            return Poll::Ready(None);
        }

        // Update state to streaming on first poll
        let _ = this.state.compare_exchange(
            StreamState::NotStarted as u8,
            StreamState::Streaming as u8,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                // Update state on completion
                if matches!(event, StreamEvent::Stop { .. } | StreamEvent::Error { .. }) {
                    this.state
                        .store(StreamState::Completed as u8, Ordering::Release);
                }
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(Some(Err(e))) => {
                this.state
                    .store(StreamState::Error as u8, Ordering::Release);
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                // Ensure we mark as completed if stream ends without Stop event
                let _ = this.state.compare_exchange(
                    StreamState::Streaming as u8,
                    StreamState::Completed as u8,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A handle for cancelling a stream.
#[derive(Clone)]
pub struct CancellationHandle {
    state: Arc<AtomicU8>,
}

impl CancellationHandle {
    /// Cancel the associated stream.
    pub fn cancel(&self) {
        self.state
            .store(StreamState::Cancelled as u8, Ordering::Release);
    }

    /// Check if the stream has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == StreamState::Cancelled as u8
    }
}

/// A fully collected streaming response.
#[derive(Debug, Clone)]
pub struct CollectedResponse {
    /// The completion ID.
    pub id: String,
    /// The model that generated the response.
    pub model: String,
    /// The collected content blocks.
    pub content: Vec<ContentBlock>,
    /// The finish reason.
    pub finish_reason: FinishReason,
    /// Token usage.
    pub usage: Usage,
}

impl CollectedResponse {
    /// Get the text content of the response.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        let texts: Vec<&str> = self
            .content
            .iter()
            .filter_map(ContentBlock::as_text)
            .collect();
        if texts.is_empty() {
            None
        } else {
            Some(texts.join(""))
        }
    }

    /// Convert to a `CompletionResponse`.
    #[must_use]
    pub fn into_response(self) -> crate::types::CompletionResponse {
        crate::types::CompletionResponse {
            id: self.id,
            model: self.model,
            content: self.content,
            finish_reason: self.finish_reason,
            usage: self.usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    #[test]
    fn test_stream_state() {
        assert!(StreamState::NotStarted.is_active());
        assert!(StreamState::Streaming.is_active());
        assert!(!StreamState::Completed.is_active());

        assert!(!StreamState::NotStarted.is_finished());
        assert!(StreamState::Completed.is_finished());
        assert!(StreamState::Cancelled.is_finished());
        assert!(StreamState::Error.is_finished());
    }

    #[tokio::test]
    async fn test_cancellation() {
        let events = vec![
            Ok(StreamEvent::Start {
                id: "test".to_string(),
                model: "gpt-4".to_string(),
            }),
            Ok(StreamEvent::ContentDelta {
                index: 0,
                delta: crate::types::ContentDelta::Text {
                    text: "Hello".to_string(),
                },
            }),
        ];

        let stream = CompletionStream::new(stream::iter(events));
        let handle = stream.cancellation_handle();

        handle.cancel();
        assert!(stream.is_cancelled());
    }
}
