//! Server-Sent Events (SSE) parsing utilities.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use mcpkit_core::protocol::Message;

use crate::error::TransportError;

/// HTTP transport state for tracking session and messages.
#[derive(Debug)]
pub struct HttpTransportState {
    /// Current session ID.
    pub session_id: Option<String>,
    /// Queue of received messages.
    pub message_queue: VecDeque<Message>,
    /// Last event ID for SSE reconnection.
    pub last_event_id: Option<String>,
    /// Current SSE buffer for parsing.
    pub sse_buffer: String,
}

impl HttpTransportState {
    /// Create a new HTTP transport state.
    #[must_use]
    pub fn new(session_id: Option<String>) -> Self {
        Self {
            session_id,
            message_queue: VecDeque::new(),
            last_event_id: None,
            sse_buffer: String::new(),
        }
    }
}

/// Process the SSE buffer and extract complete events.
///
/// This function parses SSE events from the buffer, extracts JSON-RPC messages,
/// and queues them for processing.
pub fn process_sse_buffer(
    state: &mut HttpTransportState,
    messages_received: &AtomicU64,
    max_message_size: usize,
) -> Result<(), TransportError> {
    // SSE events are delimited by double newlines
    while let Some(event_end) = state.sse_buffer.find("\n\n") {
        let event_str = state.sse_buffer[..event_end].to_string();
        state.sse_buffer = state.sse_buffer[event_end + 2..].to_string();

        // Parse the SSE event
        let mut event_id = None;
        let mut data_lines = Vec::new();

        for line in event_str.lines() {
            if let Some(id) = line.strip_prefix("id:") {
                event_id = Some(id.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                data_lines.push(data.trim_start().to_string());
            }
            // Ignore other fields (event:, retry:, etc.) for now
        }

        // Update last event ID
        if let Some(id) = event_id {
            state.last_event_id = Some(id);
        }

        // Join data lines and parse as JSON-RPC message
        if !data_lines.is_empty() {
            let data = data_lines.join("\n");
            if !data.is_empty() {
                // Check message size limit
                if data.len() > max_message_size {
                    return Err(TransportError::MessageTooLarge {
                        size: data.len(),
                        max: max_message_size,
                    });
                }

                match serde_json::from_str::<Message>(&data) {
                    Ok(msg) => {
                        state.message_queue.push_back(msg);
                        messages_received.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse SSE data as JSON-RPC: {e}");
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_buffer_parsing() -> Result<(), Box<dyn std::error::Error>> {
        let messages_received = AtomicU64::new(0);
        let mut state = HttpTransportState::new(None);
        state.sse_buffer =
            String::from("id: evt-001\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n");

        process_sse_buffer(&mut state, &messages_received, 16 * 1024 * 1024)?;

        assert_eq!(state.last_event_id, Some("evt-001".to_string()));
        assert_eq!(state.message_queue.len(), 1);
        assert!(state.sse_buffer.is_empty());
        assert_eq!(messages_received.load(Ordering::Relaxed), 1);
        Ok(())
    }

    #[test]
    fn test_sse_buffer_multiple_events() -> Result<(), Box<dyn std::error::Error>> {
        let messages_received = AtomicU64::new(0);
        let mut state = HttpTransportState::new(None);
        state.sse_buffer = String::from(
            "id: 1\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n\
             id: 2\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n\n",
        );

        process_sse_buffer(&mut state, &messages_received, 16 * 1024 * 1024)?;

        assert_eq!(state.last_event_id, Some("2".to_string()));
        assert_eq!(state.message_queue.len(), 2);
        assert_eq!(messages_received.load(Ordering::Relaxed), 2);
        Ok(())
    }

    #[test]
    fn test_sse_buffer_message_too_large() {
        let messages_received = AtomicU64::new(0);
        let mut state = HttpTransportState::new(None);
        state.sse_buffer = String::from("data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n");

        let result = process_sse_buffer(&mut state, &messages_received, 10); // Very small limit

        assert!(matches!(
            result,
            Err(TransportError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn test_sse_buffer_incomplete_event() -> Result<(), Box<dyn std::error::Error>> {
        let messages_received = AtomicU64::new(0);
        let mut state = HttpTransportState::new(None);
        state.sse_buffer = String::from("id: evt-001\ndata: {\"jsonrpc\":\"2.0\""); // No double newline

        process_sse_buffer(&mut state, &messages_received, 16 * 1024 * 1024)?;

        // Should not process incomplete event
        assert!(state.last_event_id.is_none());
        assert!(state.message_queue.is_empty());
        assert!(!state.sse_buffer.is_empty());
        Ok(())
    }
}
