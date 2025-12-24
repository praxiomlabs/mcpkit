//! Session recording and replay utilities.
//!
//! The session recorder captures entire MCP sessions for later
//! analysis, testing, or replay.

use crate::protocol::Message;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

/// A recorded MCP session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedSession {
    /// Session name/identifier.
    pub name: String,
    /// When the recording started.
    pub started_at: SystemTime,
    /// Total duration of the session.
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    /// Recorded events.
    pub events: Vec<SessionEvent>,
    /// Session metadata.
    pub metadata: SessionMetadata,
}

impl RecordedSession {
    /// Create a new empty session.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            started_at: SystemTime::now(),
            duration: Duration::ZERO,
            events: Vec::new(),
            metadata: SessionMetadata::default(),
        }
    }

    /// Get the number of events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Get only messages (excluding lifecycle events).
    #[must_use]
    pub fn messages(&self) -> Vec<&Message> {
        self.events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::MessageSent { message, .. }
                | SessionEvent::MessageReceived { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    /// Get events within a time range.
    #[must_use]
    pub fn events_in_range(&self, start: Duration, end: Duration) -> Vec<&SessionEvent> {
        self.events
            .iter()
            .filter(|e| {
                let offset = e.offset();
                offset >= start && offset <= end
            })
            .collect()
    }

    /// Export to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Import from JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Session metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Client name.
    pub client_name: Option<String>,
    /// Client version.
    pub client_version: Option<String>,
    /// Server name.
    pub server_name: Option<String>,
    /// Server version.
    pub server_version: Option<String>,
    /// Transport type.
    pub transport: Option<String>,
    /// Protocol version.
    pub protocol_version: Option<String>,
    /// Custom tags.
    pub tags: Vec<String>,
}

/// A session event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEvent {
    /// Session started.
    SessionStarted {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
    },
    /// Message sent by client.
    MessageSent {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
        /// The message.
        message: Message,
    },
    /// Message received by client.
    MessageReceived {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
        /// The message.
        message: Message,
    },
    /// Error occurred.
    Error {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
        /// Error message.
        error: String,
    },
    /// Session ended.
    SessionEnded {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
        /// Reason for ending.
        reason: Option<String>,
    },
    /// Custom event.
    Custom {
        /// Offset from session start.
        #[serde(with = "duration_serde")]
        offset: Duration,
        /// Event name.
        name: String,
        /// Event data.
        data: serde_json::Value,
    },
}

impl SessionEvent {
    /// Get the offset from session start.
    #[must_use]
    pub fn offset(&self) -> Duration {
        match self {
            Self::SessionStarted { offset }
            | Self::MessageSent { offset, .. }
            | Self::MessageReceived { offset, .. }
            | Self::Error { offset, .. }
            | Self::SessionEnded { offset, .. }
            | Self::Custom { offset, .. } => *offset,
        }
    }
}

/// Session recorder for capturing MCP sessions.
pub struct SessionRecorder {
    /// Session name.
    name: String,
    /// When recording started.
    started: Instant,
    /// Recording state.
    state: Arc<RwLock<RecorderState>>,
}

struct RecorderState {
    /// Recorded events.
    events: Vec<SessionEvent>,
    /// Whether recording is active.
    recording: bool,
    /// Session metadata.
    metadata: SessionMetadata,
}

impl SessionRecorder {
    /// Create a new session recorder.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            started: Instant::now(),
            state: Arc::new(RwLock::new(RecorderState {
                events: vec![SessionEvent::SessionStarted {
                    offset: Duration::ZERO,
                }],
                recording: true,
                metadata: SessionMetadata::default(),
            })),
        }
    }

    /// Set session metadata.
    pub fn set_metadata(&self, metadata: SessionMetadata) {
        if let Ok(mut state) = self.state.write() {
            state.metadata = metadata;
        }
    }

    /// Check if recording is active.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.state.read().map(|s| s.recording).unwrap_or(false)
    }

    /// Stop recording.
    pub fn stop(&self, reason: Option<String>) {
        if let Ok(mut state) = self.state.write() {
            if state.recording {
                state.events.push(SessionEvent::SessionEnded {
                    offset: self.started.elapsed(),
                    reason,
                });
                state.recording = false;
            }
        }
    }

    /// Record a sent message.
    pub fn record_sent(&self, message: Message) {
        if let Ok(mut state) = self.state.write() {
            if state.recording {
                state.events.push(SessionEvent::MessageSent {
                    offset: self.started.elapsed(),
                    message,
                });
            }
        }
    }

    /// Record a received message.
    pub fn record_received(&self, message: Message) {
        if let Ok(mut state) = self.state.write() {
            if state.recording {
                state.events.push(SessionEvent::MessageReceived {
                    offset: self.started.elapsed(),
                    message,
                });
            }
        }
    }

    /// Record an error.
    pub fn record_error(&self, error: impl Into<String>) {
        if let Ok(mut state) = self.state.write() {
            if state.recording {
                state.events.push(SessionEvent::Error {
                    offset: self.started.elapsed(),
                    error: error.into(),
                });
            }
        }
    }

    /// Record a custom event.
    pub fn record_custom(&self, name: impl Into<String>, data: serde_json::Value) {
        if let Ok(mut state) = self.state.write() {
            if state.recording {
                state.events.push(SessionEvent::Custom {
                    offset: self.started.elapsed(),
                    name: name.into(),
                    data,
                });
            }
        }
    }

    /// Get the number of recorded events.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.state.read().map(|s| s.events.len()).unwrap_or(0)
    }

    /// Finalize and return the recorded session.
    #[must_use]
    pub fn finalize(self) -> RecordedSession {
        self.stop(Some("finalized".to_string()));

        let (events, metadata) = self
            .state
            .read()
            .map(|s| (s.events.clone(), s.metadata.clone()))
            .unwrap_or_default();

        RecordedSession {
            name: self.name,
            started_at: SystemTime::now() - self.started.elapsed(),
            duration: self.started.elapsed(),
            events,
            metadata,
        }
    }
}

impl Clone for SessionRecorder {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            started: self.started,
            state: Arc::clone(&self.state),
        }
    }
}

/// Serde support for Duration.
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ms = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Request, RequestId, Response};

    #[test]
    fn test_session_recorder() {
        let recorder = SessionRecorder::new("test-session");

        recorder.record_sent(Message::Request(Request::new("test/method", 1)));
        recorder.record_received(Message::Response(Response::success(
            RequestId::from(1),
            serde_json::json!({}),
        )));

        assert_eq!(recorder.event_count(), 3); // start + 2 messages
        assert!(recorder.is_recording());

        let session = recorder.finalize();
        assert_eq!(session.name, "test-session");
        assert_eq!(session.events.len(), 4); // start + 2 messages + end
    }

    #[test]
    fn test_session_serialization() {
        let recorder = SessionRecorder::new("test");
        recorder.record_sent(Message::Request(Request::new("ping", 1)));
        let session = recorder.finalize();

        let json = session.to_json().expect("Failed to serialize");
        let restored = RecordedSession::from_json(&json).expect("Failed to deserialize");

        assert_eq!(restored.name, "test");
        assert_eq!(restored.events.len(), session.events.len());
    }

    #[test]
    fn test_session_metadata() {
        let recorder = SessionRecorder::new("test");

        let metadata = SessionMetadata {
            client_name: Some("test-client".to_string()),
            client_version: Some("1.0.0".to_string()),
            ..Default::default()
        };

        recorder.set_metadata(metadata);

        let session = recorder.finalize();
        assert_eq!(
            session.metadata.client_name,
            Some("test-client".to_string())
        );
    }
}
