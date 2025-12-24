//! Protocol validation utilities.
//!
//! The protocol validator checks MCP message sequences for correctness,
//! helping identify protocol violations during development.

use crate::protocol::{Message, Notification, Request, RequestId, Response};
use std::collections::{HashMap, HashSet};

/// Protocol validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    /// Response without matching request.
    #[error("orphan response: no request found for ID {id:?}")]
    OrphanResponse {
        /// The orphan response ID.
        id: RequestId,
    },

    /// Duplicate request ID.
    #[error("duplicate request ID: {id:?}")]
    DuplicateRequestId {
        /// The duplicate request ID.
        id: RequestId,
    },

    /// Request without response (timed out).
    #[error("unmatched request: {method} (ID: {id:?})")]
    UnmatchedRequest {
        /// The unmatched request ID.
        id: RequestId,
        /// The method name.
        method: String,
    },

    /// Unknown method.
    #[error("unknown method: {method}")]
    UnknownMethod {
        /// The unknown method name.
        method: String,
    },

    /// Invalid message sequence.
    #[error("invalid sequence: {message}")]
    InvalidSequence {
        /// Description of the sequence error.
        message: String,
    },

    /// Missing required initialization.
    #[error("missing initialization: {message}")]
    MissingInitialization {
        /// Description of what is missing.
        message: String,
    },
}

/// Result of protocol validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed.
    pub valid: bool,
    /// Validation errors found.
    pub errors: Vec<ValidationError>,
    /// Warnings (non-fatal issues).
    pub warnings: Vec<String>,
    /// Summary statistics.
    pub stats: ValidationStats,
}

impl ValidationResult {
    /// Create a passing result.
    #[must_use]
    pub fn pass() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: ValidationStats::default(),
        }
    }

    /// Create a failing result.
    #[must_use]
    pub fn fail(errors: Vec<ValidationError>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: Vec::new(),
            stats: ValidationStats::default(),
        }
    }

    /// Add a warning.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// Validation statistics.
#[derive(Debug, Clone, Default)]
pub struct ValidationStats {
    /// Total messages validated.
    pub total_messages: usize,
    /// Requests validated.
    pub requests: usize,
    /// Responses validated.
    pub responses: usize,
    /// Notifications validated.
    pub notifications: usize,
    /// Matched request-response pairs.
    pub matched_pairs: usize,
}

/// Protocol validator for checking MCP message sequences.
///
/// The validator tracks the protocol state and checks for:
/// - Request-response matching
/// - Duplicate request IDs
/// - Proper initialization sequence
/// - Known methods
#[derive(Debug)]
pub struct ProtocolValidator {
    /// Known request methods.
    known_request_methods: HashSet<String>,
    /// Known notification methods.
    known_notification_methods: HashSet<String>,
    /// Pending requests (waiting for response).
    pending_requests: HashMap<RequestId, String>,
    /// Seen request IDs (for duplicate detection).
    seen_request_ids: HashSet<RequestId>,
    /// Whether initialization is complete.
    initialized: bool,
    /// Collected errors.
    errors: Vec<ValidationError>,
    /// Collected warnings.
    warnings: Vec<String>,
    /// Stats.
    stats: ValidationStats,
    /// Strict mode (unknown methods are errors).
    strict_mode: bool,
}

impl Default for ProtocolValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtocolValidator {
    /// Create a new validator with default MCP methods.
    #[must_use]
    pub fn new() -> Self {
        let mut validator = Self {
            known_request_methods: HashSet::new(),
            known_notification_methods: HashSet::new(),
            pending_requests: HashMap::new(),
            seen_request_ids: HashSet::new(),
            initialized: false,
            errors: Vec::new(),
            warnings: Vec::new(),
            stats: ValidationStats::default(),
            strict_mode: false,
        };

        // Add standard MCP methods
        validator.register_request_methods(&[
            "initialize",
            "ping",
            "tools/list",
            "tools/call",
            "resources/list",
            "resources/read",
            "resources/subscribe",
            "resources/unsubscribe",
            "prompts/list",
            "prompts/get",
            "logging/setLevel",
            "completion/complete",
            "sampling/createMessage",
            "roots/list",
        ]);

        validator.register_notification_methods(&[
            "initialized",
            "notifications/cancelled",
            "notifications/progress",
            "notifications/message",
            "notifications/resources/updated",
            "notifications/resources/list_changed",
            "notifications/tools/list_changed",
            "notifications/prompts/list_changed",
            "notifications/roots/list_changed",
        ]);

        validator
    }

    /// Enable strict mode (unknown methods are errors).
    #[must_use]
    pub fn strict(mut self) -> Self {
        self.strict_mode = true;
        self
    }

    /// Register additional request methods.
    pub fn register_request_methods(&mut self, methods: &[&str]) {
        for method in methods {
            self.known_request_methods.insert((*method).to_string());
        }
    }

    /// Register additional notification methods.
    pub fn register_notification_methods(&mut self, methods: &[&str]) {
        for method in methods {
            self.known_notification_methods
                .insert((*method).to_string());
        }
    }

    /// Validate a single message.
    pub fn validate(&mut self, message: &Message) {
        self.stats.total_messages += 1;

        match message {
            Message::Request(req) => self.validate_request(req),
            Message::Response(res) => self.validate_response(res),
            Message::Notification(notif) => self.validate_notification(notif),
        }
    }

    fn validate_request(&mut self, request: &Request) {
        self.stats.requests += 1;

        // Check for duplicate ID
        if self.seen_request_ids.contains(&request.id) {
            self.errors.push(ValidationError::DuplicateRequestId {
                id: request.id.clone(),
            });
        }
        self.seen_request_ids.insert(request.id.clone());

        // Track pending request
        self.pending_requests
            .insert(request.id.clone(), request.method.to_string());

        // Check method
        let method = request.method.as_ref();
        if !self.known_request_methods.contains(method) {
            if self.strict_mode {
                self.errors.push(ValidationError::UnknownMethod {
                    method: method.to_string(),
                });
            } else {
                self.warnings
                    .push(format!("Unknown request method: {method}"));
            }
        }

        // Check initialization
        if method == "initialize" {
            if self.initialized {
                self.warnings
                    .push("Duplicate initialize request".to_string());
            }
        } else if !self.initialized && method != "ping" {
            self.warnings
                .push(format!("Request before initialization: {method}"));
        }
    }

    fn validate_response(&mut self, response: &Response) {
        self.stats.responses += 1;

        // Check for matching request
        if self.pending_requests.remove(&response.id).is_some() {
            self.stats.matched_pairs += 1;
        } else {
            self.errors.push(ValidationError::OrphanResponse {
                id: response.id.clone(),
            });
        }
    }

    fn validate_notification(&mut self, notification: &Notification) {
        self.stats.notifications += 1;

        let method = notification.method.as_ref();

        // Check method
        if !self.known_notification_methods.contains(method) {
            if self.strict_mode {
                self.errors.push(ValidationError::UnknownMethod {
                    method: method.to_string(),
                });
            } else {
                self.warnings
                    .push(format!("Unknown notification method: {method}"));
            }
        }

        // Track initialization
        if method == "initialized" {
            self.initialized = true;
        }
    }

    /// Check for unmatched requests (call after all messages are validated).
    pub fn check_unmatched_requests(&mut self) {
        for (id, method) in self.pending_requests.drain() {
            self.errors
                .push(ValidationError::UnmatchedRequest { id, method });
        }
    }

    /// Get the validation result.
    #[must_use]
    pub fn result(&self) -> ValidationResult {
        ValidationResult {
            valid: self.errors.is_empty(),
            errors: self.errors.clone(),
            warnings: self.warnings.clone(),
            stats: self.stats.clone(),
        }
    }

    /// Finalize validation and get result.
    #[must_use]
    pub fn finalize(mut self) -> ValidationResult {
        self.check_unmatched_requests();
        self.result()
    }

    /// Reset the validator state.
    pub fn reset(&mut self) {
        self.pending_requests.clear();
        self.seen_request_ids.clear();
        self.initialized = false;
        self.errors.clear();
        self.warnings.clear();
        self.stats = ValidationStats::default();
    }
}

/// Validate a sequence of messages.
#[must_use]
pub fn validate_message_sequence(messages: &[Message]) -> ValidationResult {
    let mut validator = ProtocolValidator::new();

    for msg in messages {
        validator.validate(msg);
    }

    validator.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_sequence() {
        let messages = vec![
            Message::Request(Request::new("initialize", 1)),
            Message::Response(Response::success(RequestId::from(1), serde_json::json!({}))),
            Message::Notification(Notification::new("initialized")),
            Message::Request(Request::new("tools/list", 2)),
            Message::Response(Response::success(
                RequestId::from(2),
                serde_json::json!({ "tools": [] }),
            )),
        ];

        let result = validate_message_sequence(&messages);
        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.stats.matched_pairs, 2);
    }

    #[test]
    fn test_orphan_response() {
        let messages = vec![Message::Response(Response::success(
            RequestId::from(999),
            serde_json::json!({}),
        ))];

        let result = validate_message_sequence(&messages);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::OrphanResponse { .. }))
        );
    }

    #[test]
    fn test_duplicate_request_id() {
        let messages = vec![
            Message::Request(Request::new("ping", 1)),
            Message::Request(Request::new("ping", 1)), // Duplicate!
        ];

        let result = validate_message_sequence(&messages);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::DuplicateRequestId { .. }))
        );
    }

    #[test]
    fn test_unmatched_request() {
        let messages = vec![
            Message::Request(Request::new("ping", 1)),
            // No response!
        ];

        let result = validate_message_sequence(&messages);
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::UnmatchedRequest { .. }))
        );
    }

    #[test]
    fn test_strict_mode() {
        let mut validator = ProtocolValidator::new().strict();

        validator.validate(&Message::Request(Request::new("unknown/method", 1)));

        let result = validator.result();
        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::UnknownMethod { .. }))
        );
    }
}
