//! Session testing utilities.
//!
//! This module provides utilities for testing MCP sessions,
//! integrating with the debug module for recording and validation.

use mcpkit_core::debug::{
    MessageInspector, MessageRecord, MessageStats, ProtocolValidator, RecordedSession,
    SessionRecorder, ValidationResult,
};
use mcpkit_core::protocol::Message;
use std::sync::Arc;

/// A test session wrapper that combines recording, inspection, and validation.
pub struct TestSession {
    /// Session name.
    name: String,
    /// Message inspector.
    inspector: Arc<MessageInspector>,
    /// Session recorder.
    recorder: SessionRecorder,
    /// Protocol validator.
    validator: Arc<std::sync::RwLock<ProtocolValidator>>,
    /// Whether to validate in strict mode.
    strict_mode: bool,
}

impl std::fmt::Debug for TestSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestSession")
            .field("name", &self.name)
            .field("inspector", &self.inspector)
            .field("strict_mode", &self.strict_mode)
            .finish_non_exhaustive()
    }
}

impl TestSession {
    /// Create a new test session.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            inspector: Arc::new(MessageInspector::new()),
            recorder: SessionRecorder::new(name),
            validator: Arc::new(std::sync::RwLock::new(ProtocolValidator::new())),
            strict_mode: false,
        }
    }

    /// Enable strict validation mode.
    #[must_use]
    pub fn strict(mut self) -> Self {
        self.strict_mode = true;
        if let Ok(mut validator) = self.validator.write() {
            *validator = ProtocolValidator::new().strict();
        }
        self
    }

    /// Get the session name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the message inspector.
    #[must_use]
    pub fn inspector(&self) -> &MessageInspector {
        &self.inspector
    }

    /// Get the session recorder.
    #[must_use]
    pub fn recorder(&self) -> &SessionRecorder {
        &self.recorder
    }

    /// Record an outbound message (sent by client).
    pub fn record_outbound(&self, message: Message) {
        self.inspector.record_outbound(message.clone());
        self.recorder.record_sent(message.clone());
        if let Ok(mut validator) = self.validator.write() {
            validator.validate(&message);
        }
    }

    /// Record an inbound message (received by client).
    pub fn record_inbound(&self, message: Message) {
        self.inspector.record_inbound(message.clone());
        self.recorder.record_received(message.clone());
        if let Ok(mut validator) = self.validator.write() {
            validator.validate(&message);
        }
    }

    /// Record an error.
    pub fn record_error(&self, error: impl Into<String>) {
        self.recorder.record_error(error);
    }

    /// Get message statistics.
    #[must_use]
    pub fn stats(&self) -> MessageStats {
        self.inspector.stats()
    }

    /// Get all message records.
    #[must_use]
    pub fn records(&self) -> Vec<MessageRecord> {
        self.inspector.records()
    }

    /// Get the current validation result.
    #[must_use]
    pub fn validation_result(&self) -> ValidationResult {
        self.validator
            .read()
            .map_or_else(|_| ValidationResult::pass(), |v| v.result())
    }

    /// Finalize the session and get the recorded session.
    #[must_use]
    pub fn finalize(self) -> TestSessionResult {
        let validation = self.validator.write().map_or_else(
            |_| ValidationResult::pass(),
            |mut v| {
                v.check_unmatched_requests();
                v.result()
            },
        );

        let session = self.recorder.finalize();

        TestSessionResult {
            name: self.name,
            session,
            stats: self.inspector.stats(),
            validation,
        }
    }

    /// Assert that the session is valid so far.
    ///
    /// # Panics
    ///
    /// Panics if there are validation errors.
    pub fn assert_valid(&self) {
        let result = self.validation_result();
        assert!(
            result.valid,
            "Session validation failed:\n{}",
            result
                .errors
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Assert specific statistics.
    ///
    /// # Panics
    ///
    /// Panics if the assertion fails.
    pub fn assert_stats(&self, check: impl FnOnce(&MessageStats) -> bool, message: &str) {
        let stats = self.stats();
        assert!(check(&stats), "{message}. Stats: {stats:?}");
    }
}

impl Clone for TestSession {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            inspector: Arc::clone(&self.inspector),
            recorder: self.recorder.clone(),
            validator: Arc::clone(&self.validator),
            strict_mode: self.strict_mode,
        }
    }
}

/// Result of a completed test session.
#[derive(Debug)]
pub struct TestSessionResult {
    /// Session name.
    pub name: String,
    /// Recorded session data.
    pub session: RecordedSession,
    /// Final statistics.
    pub stats: MessageStats,
    /// Validation result.
    pub validation: ValidationResult,
}

impl TestSessionResult {
    /// Check if the session is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.validation.valid
    }

    /// Get the number of messages.
    #[must_use]
    pub fn message_count(&self) -> usize {
        self.stats.total_messages
    }

    /// Get the number of errors.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.stats.errors
    }

    /// Get all validation errors.
    #[must_use]
    pub fn errors(&self) -> &[mcpkit_core::debug::ValidationError] {
        &self.validation.errors
    }

    /// Get all warnings.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.validation.warnings
    }

    /// Export the session to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        self.session.to_json()
    }

    /// Assert that the session is valid.
    ///
    /// # Panics
    ///
    /// Panics if there are validation errors.
    pub fn assert_valid(&self) {
        assert!(
            self.is_valid(),
            "Session '{}' validation failed:\n{}",
            self.name,
            self.validation
                .errors
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Assert message count.
    ///
    /// # Panics
    ///
    /// Panics if the count doesn't match.
    pub fn assert_message_count(&self, expected: usize) {
        assert_eq!(
            self.message_count(),
            expected,
            "Expected {} messages, got {}",
            expected,
            self.message_count()
        );
    }

    /// Assert no errors.
    ///
    /// # Panics
    ///
    /// Panics if there are errors.
    pub fn assert_no_errors(&self) {
        assert_eq!(
            self.error_count(),
            0,
            "Expected no errors, got {}",
            self.error_count()
        );
    }
}

/// Builder for creating test sessions with custom configuration.
#[derive(Debug, Default)]
pub struct TestSessionBuilder {
    name: Option<String>,
    strict_mode: bool,
    max_records: Option<usize>,
}

impl TestSessionBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the session name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Enable strict validation mode.
    #[must_use]
    pub fn strict(mut self) -> Self {
        self.strict_mode = true;
        self
    }

    /// Set maximum records to keep.
    #[must_use]
    pub fn max_records(mut self, max: usize) -> Self {
        self.max_records = Some(max);
        self
    }

    /// Build the test session.
    #[must_use]
    pub fn build(self) -> TestSession {
        let name = self.name.unwrap_or_else(|| "test-session".to_string());
        let mut session = TestSession::new(name);

        if self.strict_mode {
            session = session.strict();
        }

        session
    }
}

/// Compare two sessions for differences.
#[derive(Debug)]
pub struct SessionDiff {
    /// Messages only in session A.
    pub only_in_a: Vec<usize>,
    /// Messages only in session B.
    pub only_in_b: Vec<usize>,
    /// Messages that differ.
    pub different: Vec<(usize, String)>,
}

impl SessionDiff {
    /// Compare two recorded sessions.
    #[must_use]
    pub fn compare(a: &RecordedSession, b: &RecordedSession) -> Self {
        let mut diff = SessionDiff {
            only_in_a: Vec::new(),
            only_in_b: Vec::new(),
            different: Vec::new(),
        };

        let a_messages = a.messages();
        let b_messages = b.messages();

        for i in 0..a_messages.len().max(b_messages.len()) {
            match (a_messages.get(i), b_messages.get(i)) {
                (Some(_), None) => diff.only_in_a.push(i),
                (None, Some(_)) => diff.only_in_b.push(i),
                (Some(ma), Some(mb)) => {
                    if format!("{ma:?}") != format!("{mb:?}") {
                        diff.different.push((i, format!("Message {i} differs")));
                    }
                }
                (None, None) => {}
            }
        }

        diff
    }

    /// Check if sessions are identical.
    #[must_use]
    pub fn is_identical(&self) -> bool {
        self.only_in_a.is_empty() && self.only_in_b.is_empty() && self.different.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcpkit_core::protocol::{Request, RequestId, Response};

    #[test]
    fn test_test_session() {
        let session = TestSession::new("test");

        session.record_outbound(Message::Request(Request::new("ping", 1)));
        session.record_inbound(Message::Response(Response::success(
            RequestId::from(1),
            serde_json::json!({}),
        )));

        let stats = session.stats();
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.responses, 1);

        let result = session.finalize();
        assert!(result.is_valid());
        assert_eq!(result.message_count(), 2);
    }

    #[test]
    fn test_test_session_validation() {
        let session = TestSession::new("test");

        // Record orphan response (no matching request)
        session.record_inbound(Message::Response(Response::success(
            RequestId::from(999),
            serde_json::json!({}),
        )));

        let result = session.finalize();
        assert!(!result.is_valid());
        assert!(!result.errors().is_empty());
    }

    #[test]
    fn test_session_builder() {
        let session = TestSessionBuilder::new()
            .name("custom-session")
            .strict()
            .build();

        assert_eq!(session.name(), "custom-session");
    }

    #[test]
    fn test_session_diff() {
        let recorder_a = SessionRecorder::new("a");
        recorder_a.record_sent(Message::Request(Request::new("ping", 1)));
        let session_a = recorder_a.finalize();

        let recorder_b = SessionRecorder::new("b");
        recorder_b.record_sent(Message::Request(Request::new("ping", 1)));
        recorder_b.record_sent(Message::Request(Request::new("pong", 2)));
        let session_b = recorder_b.finalize();

        let diff = SessionDiff::compare(&session_a, &session_b);
        assert!(!diff.is_identical());
        assert!(!diff.only_in_b.is_empty());
    }
}
