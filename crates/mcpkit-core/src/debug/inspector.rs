//! Message inspection utilities.
//!
//! The message inspector captures protocol messages for analysis
//! and debugging.

use crate::protocol::{Message, RequestId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// A record of a captured message.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    /// Timestamp when the message was captured.
    pub timestamp: Instant,
    /// The direction of the message.
    pub direction: MessageDirection,
    /// The captured message.
    pub message: Message,
    /// Size in bytes (approximate).
    pub size_bytes: usize,
    /// Optional context/tags.
    pub tags: Vec<String>,
}

impl MessageRecord {
    /// Create a new message record.
    #[must_use]
    pub fn new(direction: MessageDirection, message: Message) -> Self {
        let size_bytes = serde_json::to_string(&message)
            .map(|s| s.len())
            .unwrap_or(0);

        Self {
            timestamp: Instant::now(),
            direction,
            message,
            size_bytes,
            tags: Vec::new(),
        }
    }

    /// Add a tag to the record.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Get the method name if applicable.
    #[must_use]
    pub fn method(&self) -> Option<&str> {
        self.message.method()
    }

    /// Get the request ID if applicable.
    #[must_use]
    pub fn request_id(&self) -> Option<&RequestId> {
        match &self.message {
            Message::Request(req) => Some(&req.id),
            Message::Response(res) => Some(&res.id),
            Message::Notification(_) => None,
        }
    }

    /// Check if this is a request message.
    #[must_use]
    pub fn is_request(&self) -> bool {
        matches!(self.message, Message::Request(_))
    }

    /// Check if this is a response message.
    #[must_use]
    pub fn is_response(&self) -> bool {
        matches!(self.message, Message::Response(_))
    }

    /// Check if this is a notification message.
    #[must_use]
    pub fn is_notification(&self) -> bool {
        matches!(self.message, Message::Notification(_))
    }

    /// Check if the response indicates an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(&self.message, Message::Response(r) if r.error.is_some())
    }
}

/// Direction of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDirection {
    /// Message sent by client.
    Outbound,
    /// Message received by client.
    Inbound,
}

/// Statistics about captured messages.
#[derive(Debug, Clone, Default)]
pub struct MessageStats {
    /// Total messages captured.
    pub total_messages: usize,
    /// Total requests.
    pub requests: usize,
    /// Total responses.
    pub responses: usize,
    /// Total notifications.
    pub notifications: usize,
    /// Total errors (error responses).
    pub errors: usize,
    /// Total bytes transferred.
    pub total_bytes: usize,
    /// Messages by method.
    pub by_method: HashMap<String, usize>,
    /// Average response time per method.
    pub avg_response_time: HashMap<String, Duration>,
}

impl MessageStats {
    /// Calculate error rate.
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.responses == 0 {
            0.0
        } else {
            self.errors as f64 / self.responses as f64
        }
    }
}

/// Message inspector for capturing and analyzing protocol traffic.
///
/// The inspector can be used as a debugging aid during development
/// to understand the message flow between client and server.
#[derive(Debug)]
pub struct MessageInspector {
    /// Captured messages.
    records: Arc<RwLock<Vec<MessageRecord>>>,
    /// Maximum number of records to keep (0 = unlimited).
    max_records: usize,
    /// Whether capturing is enabled.
    enabled: Arc<RwLock<bool>>,
    /// Pending requests for response time tracking.
    pending_requests: Arc<RwLock<HashMap<RequestId, (Instant, String)>>>,
    /// Response times by method.
    response_times: Arc<RwLock<HashMap<String, Vec<Duration>>>>,
}

impl Default for MessageInspector {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageInspector {
    /// Create a new message inspector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(Vec::new())),
            max_records: 10000,
            enabled: Arc::new(RwLock::new(true)),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            response_times: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an inspector with a maximum record limit.
    #[must_use]
    pub fn with_max_records(mut self, max: usize) -> Self {
        self.max_records = max;
        self
    }

    /// Enable or disable message capture.
    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut flag) = self.enabled.write() {
            *flag = enabled;
        }
    }

    /// Check if capturing is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.read().map(|e| *e).unwrap_or(false)
    }

    /// Record an outbound message.
    pub fn record_outbound(&self, message: Message) {
        self.record(MessageDirection::Outbound, message);
    }

    /// Record an inbound message.
    pub fn record_inbound(&self, message: Message) {
        self.record(MessageDirection::Inbound, message);
    }

    /// Record a message.
    fn record(&self, direction: MessageDirection, message: Message) {
        if !self.is_enabled() {
            return;
        }

        let record = MessageRecord::new(direction, message.clone());

        // Track pending requests for response time calculation
        if let Message::Request(ref req) = message {
            if let Ok(mut pending) = self.pending_requests.write() {
                pending.insert(req.id.clone(), (Instant::now(), req.method.to_string()));
            }
        }

        // Calculate response time for completed requests
        if let Message::Response(ref res) = message {
            if let Ok(mut pending) = self.pending_requests.write() {
                if let Some((start, method)) = pending.remove(&res.id) {
                    let duration = start.elapsed();
                    if let Ok(mut times) = self.response_times.write() {
                        times.entry(method).or_default().push(duration);
                    }
                }
            }
        }

        // Store the record
        if let Ok(mut records) = self.records.write() {
            records.push(record);

            // Trim if over limit
            if self.max_records > 0 && records.len() > self.max_records {
                let excess = records.len() - self.max_records;
                records.drain(0..excess);
            }
        }
    }

    /// Get all captured records.
    #[must_use]
    pub fn records(&self) -> Vec<MessageRecord> {
        self.records.read().map(|r| r.clone()).unwrap_or_default()
    }

    /// Get the number of captured records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.read().map(|r| r.len()).unwrap_or(0)
    }

    /// Check if there are no captured records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all captured records.
    pub fn clear(&self) {
        if let Ok(mut records) = self.records.write() {
            records.clear();
        }
        if let Ok(mut pending) = self.pending_requests.write() {
            pending.clear();
        }
        if let Ok(mut times) = self.response_times.write() {
            times.clear();
        }
    }

    /// Get message statistics.
    #[must_use]
    #[allow(clippy::field_reassign_with_default)]
    pub fn stats(&self) -> MessageStats {
        let records = self.records();
        let mut stats = MessageStats::default();

        stats.total_messages = records.len();

        for record in &records {
            stats.total_bytes += record.size_bytes;

            match &record.message {
                Message::Request(req) => {
                    stats.requests += 1;
                    *stats.by_method.entry(req.method.to_string()).or_insert(0) += 1;
                }
                Message::Response(res) => {
                    stats.responses += 1;
                    if res.error.is_some() {
                        stats.errors += 1;
                    }
                }
                Message::Notification(notif) => {
                    stats.notifications += 1;
                    *stats.by_method.entry(notif.method.to_string()).or_insert(0) += 1;
                }
            }
        }

        // Calculate average response times
        if let Ok(times) = self.response_times.read() {
            for (method, durations) in times.iter() {
                if !durations.is_empty() {
                    let total: Duration = durations.iter().sum();
                    let avg = total / durations.len() as u32;
                    stats.avg_response_time.insert(method.clone(), avg);
                }
            }
        }

        stats
    }

    /// Find requests without responses.
    #[must_use]
    pub fn pending_requests(&self) -> Vec<(RequestId, String, Duration)> {
        self.pending_requests
            .read()
            .map(|pending| {
                pending
                    .iter()
                    .map(|(id, (start, method))| (id.clone(), method.clone(), start.elapsed()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Filter records by method.
    #[must_use]
    pub fn filter_by_method(&self, method: &str) -> Vec<MessageRecord> {
        self.records()
            .into_iter()
            .filter(|r| r.method() == Some(method))
            .collect()
    }

    /// Filter records by direction.
    #[must_use]
    pub fn filter_by_direction(&self, direction: MessageDirection) -> Vec<MessageRecord> {
        self.records()
            .into_iter()
            .filter(|r| r.direction == direction)
            .collect()
    }

    /// Get only error responses.
    #[must_use]
    pub fn errors(&self) -> Vec<MessageRecord> {
        self.records()
            .into_iter()
            .filter(MessageRecord::is_error)
            .collect()
    }

    /// Export records to JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let records: Vec<_> = self
            .records()
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "direction": format!("{:?}", r.direction),
                    "message": r.message,
                    "size_bytes": r.size_bytes,
                    "tags": r.tags,
                })
            })
            .collect();

        serde_json::to_string_pretty(&records)
    }
}

impl Clone for MessageInspector {
    fn clone(&self) -> Self {
        Self {
            records: Arc::clone(&self.records),
            max_records: self.max_records,
            enabled: Arc::clone(&self.enabled),
            pending_requests: Arc::clone(&self.pending_requests),
            response_times: Arc::clone(&self.response_times),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Request, Response};

    #[test]
    fn test_message_inspector() {
        let inspector = MessageInspector::new();

        // Record some messages
        let req = Message::Request(Request::new("test/method", 1));
        inspector.record_outbound(req);

        let res = Message::Response(Response::success(RequestId::from(1), serde_json::json!({})));
        inspector.record_inbound(res);

        assert_eq!(inspector.len(), 2);

        let stats = inspector.stats();
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.responses, 1);
        assert_eq!(stats.errors, 0);
    }

    #[test]
    fn test_filter_by_method() {
        let inspector = MessageInspector::new();

        inspector.record_outbound(Message::Request(Request::new("method/a", 1)));
        inspector.record_outbound(Message::Request(Request::new("method/b", 2)));
        inspector.record_outbound(Message::Request(Request::new("method/a", 3)));

        let filtered = inspector.filter_by_method("method/a");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_max_records() {
        let inspector = MessageInspector::new().with_max_records(5);

        for i in 0..10 {
            inspector.record_outbound(Message::Request(Request::new("test", i)));
        }

        assert_eq!(inspector.len(), 5);
    }

    #[test]
    fn test_enable_disable() {
        let inspector = MessageInspector::new();

        inspector.record_outbound(Message::Request(Request::new("test", 1)));
        assert_eq!(inspector.len(), 1);

        inspector.set_enabled(false);
        inspector.record_outbound(Message::Request(Request::new("test", 2)));
        assert_eq!(inspector.len(), 1); // Still 1, not recorded
    }
}
