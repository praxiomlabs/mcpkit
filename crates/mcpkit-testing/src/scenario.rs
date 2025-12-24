//! Test scenario runner for MCP protocol testing.
//!
//! This module provides a way to define and execute test scenarios
//! that consist of multiple steps with expected outcomes.

use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Type alias for custom response matcher functions.
pub type ResponseMatcherFn = Arc<dyn Fn(&Response) -> Result<(), String> + Send + Sync>;

/// Type alias for custom notification matcher functions.
pub type NotificationMatcherFn = Arc<dyn Fn(&Notification) -> Result<(), String> + Send + Sync>;

/// A test step in a scenario.
#[derive(Clone)]
pub enum TestStep {
    /// Send a request and expect a response.
    RequestResponse {
        /// Request to send.
        request: Request,
        /// Expected response matcher.
        expected: ResponseMatcher,
    },
    /// Send a notification (no response expected).
    SendNotification(Notification),
    /// Expect to receive a notification.
    ExpectNotification(NotificationMatcher),
    /// Wait for a duration.
    Wait(Duration),
    /// Custom assertion.
    Assert {
        /// Description of the assertion.
        description: String,
        /// Assertion function.
        check: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
    },
}

impl fmt::Debug for TestStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestResponse { request, expected } => f
                .debug_struct("RequestResponse")
                .field("request", request)
                .field("expected", expected)
                .finish(),
            Self::SendNotification(notif) => {
                f.debug_tuple("SendNotification").field(notif).finish()
            }
            Self::ExpectNotification(matcher) => {
                f.debug_tuple("ExpectNotification").field(matcher).finish()
            }
            Self::Wait(duration) => f.debug_tuple("Wait").field(duration).finish(),
            Self::Assert { description, .. } => f
                .debug_struct("Assert")
                .field("description", description)
                .field("check", &"<fn>")
                .finish(),
        }
    }
}

/// Matcher for response validation.
#[derive(Clone)]
pub struct ResponseMatcher {
    /// Expected success or error.
    pub expect_success: Option<bool>,
    /// JSON path assertions.
    pub json_assertions: Vec<JsonAssertion>,
    /// Custom matcher.
    pub custom: Option<ResponseMatcherFn>,
}

impl fmt::Debug for ResponseMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponseMatcher")
            .field("expect_success", &self.expect_success)
            .field("json_assertions", &self.json_assertions)
            .field("custom", &self.custom.is_some())
            .finish()
    }
}

impl Default for ResponseMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseMatcher {
    /// Create a new response matcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            expect_success: None,
            json_assertions: Vec::new(),
            custom: None,
        }
    }

    /// Expect a successful response.
    #[must_use]
    pub fn success() -> Self {
        Self {
            expect_success: Some(true),
            ..Default::default()
        }
    }

    /// Expect an error response.
    #[must_use]
    pub fn error() -> Self {
        Self {
            expect_success: Some(false),
            ..Default::default()
        }
    }

    /// Add a JSON path assertion.
    #[must_use]
    pub fn with_json(mut self, path: impl Into<String>, expected: serde_json::Value) -> Self {
        self.json_assertions.push(JsonAssertion {
            path: path.into(),
            expected,
        });
        self
    }

    /// Add a custom matcher function.
    pub fn with_custom<F>(mut self, f: F) -> Self
    where
        F: Fn(&Response) -> Result<(), String> + Send + Sync + 'static,
    {
        self.custom = Some(Arc::new(f));
        self
    }

    /// Validate a response against this matcher.
    pub fn validate(&self, response: &Response) -> Result<(), String> {
        // Check success/error expectation
        if let Some(expect_success) = self.expect_success {
            let is_success = response.error.is_none();
            if expect_success && !is_success {
                return Err(format!(
                    "Expected successful response, got error: {:?}",
                    response.error
                ));
            }
            if !expect_success && is_success {
                return Err("Expected error response, got success".to_string());
            }
        }

        // Run JSON assertions
        if let Some(result) = &response.result {
            for assertion in &self.json_assertions {
                assertion.validate(result)?;
            }
        } else if !self.json_assertions.is_empty() {
            return Err("Expected result in response, but got none".to_string());
        }

        // Run custom matcher
        if let Some(custom) = &self.custom {
            custom(response)?;
        }

        Ok(())
    }
}

/// JSON path assertion.
#[derive(Debug, Clone)]
pub struct JsonAssertion {
    /// JSON path (simplified dot notation).
    pub path: String,
    /// Expected value.
    pub expected: serde_json::Value,
}

impl JsonAssertion {
    /// Validate against a JSON value.
    pub fn validate(&self, value: &serde_json::Value) -> Result<(), String> {
        let actual = get_json_path(value, &self.path)
            .ok_or_else(|| format!("Path '{}' not found in response", self.path))?;

        if *actual != self.expected {
            return Err(format!(
                "Path '{}': expected {:?}, got {:?}",
                self.path, self.expected, actual
            ));
        }

        Ok(())
    }
}

/// Get a value from JSON using dot notation path.
fn get_json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        // Handle array index
        if let Some(index_str) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Ok(index) = index_str.parse::<usize>() {
                current = current.get(index)?;
                continue;
            }
        }
        current = current.get(part)?;
    }
    Some(current)
}

/// Matcher for notification validation.
#[derive(Clone)]
pub struct NotificationMatcher {
    /// Expected method.
    pub method: Option<String>,
    /// Custom matcher.
    pub custom: Option<NotificationMatcherFn>,
}

impl fmt::Debug for NotificationMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NotificationMatcher")
            .field("method", &self.method)
            .field("custom", &self.custom.is_some())
            .finish()
    }
}

impl Default for NotificationMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationMatcher {
    /// Create a new notification matcher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            method: None,
            custom: None,
        }
    }

    /// Match a specific method.
    pub fn method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    /// Add a custom matcher.
    pub fn with_custom<F>(mut self, f: F) -> Self
    where
        F: Fn(&Notification) -> Result<(), String> + Send + Sync + 'static,
    {
        self.custom = Some(Arc::new(f));
        self
    }

    /// Validate a notification against this matcher.
    pub fn validate(&self, notification: &Notification) -> Result<(), String> {
        if let Some(expected_method) = &self.method {
            if notification.method.as_ref() != expected_method {
                return Err(format!(
                    "Expected notification method '{}', got '{}'",
                    expected_method, notification.method
                ));
            }
        }

        if let Some(custom) = &self.custom {
            custom(notification)?;
        }

        Ok(())
    }
}

/// A test scenario consisting of multiple steps.
#[derive(Debug)]
pub struct TestScenario {
    /// Scenario name.
    pub name: String,
    /// Scenario description.
    pub description: Option<String>,
    /// Steps to execute.
    pub steps: Vec<TestStep>,
}

impl TestScenario {
    /// Create a new test scenario.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            steps: Vec::new(),
        }
    }

    /// Set the description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a request-response step.
    #[must_use]
    pub fn request(mut self, request: Request, expected: ResponseMatcher) -> Self {
        self.steps
            .push(TestStep::RequestResponse { request, expected });
        self
    }

    /// Add a send notification step.
    #[must_use]
    pub fn send_notification(mut self, notification: Notification) -> Self {
        self.steps.push(TestStep::SendNotification(notification));
        self
    }

    /// Add an expect notification step.
    #[must_use]
    pub fn expect_notification(mut self, matcher: NotificationMatcher) -> Self {
        self.steps.push(TestStep::ExpectNotification(matcher));
        self
    }

    /// Add a wait step.
    #[must_use]
    pub fn wait(mut self, duration: Duration) -> Self {
        self.steps.push(TestStep::Wait(duration));
        self
    }

    /// Add a custom assertion step.
    pub fn assert<F>(mut self, description: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> Result<(), String> + Send + Sync + 'static,
    {
        self.steps.push(TestStep::Assert {
            description: description.into(),
            check: Arc::new(check),
        });
        self
    }

    /// Add an MCP initialize handshake.
    #[must_use]
    pub fn initialize(self, client_name: &str, client_version: &str) -> Self {
        let request = Request::new("initialize", RequestId::from(1)).params(serde_json::json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": client_name,
                "version": client_version
            }
        }));

        self.request(request, ResponseMatcher::success())
            .send_notification(Notification::new("initialized"))
    }
}

/// Result of running a test scenario.
#[derive(Debug)]
pub struct ScenarioResult {
    /// Whether all steps passed.
    pub success: bool,
    /// Results for each step.
    pub step_results: Vec<StepResult>,
    /// Overall error message if failed.
    pub error: Option<String>,
}

impl ScenarioResult {
    /// Create a successful result.
    #[must_use]
    pub fn pass(step_results: Vec<StepResult>) -> Self {
        Self {
            success: true,
            step_results,
            error: None,
        }
    }

    /// Create a failed result.
    #[must_use]
    pub fn fail(step_results: Vec<StepResult>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            step_results,
            error: Some(error.into()),
        }
    }
}

/// Result of a single step.
#[derive(Debug)]
pub struct StepResult {
    /// Step index.
    pub index: usize,
    /// Step description.
    pub description: String,
    /// Whether the step passed.
    pub passed: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Duration of the step.
    pub duration: Duration,
}

/// Message queue for testing scenarios.
#[derive(Debug, Default)]
pub struct MessageQueue {
    /// Outgoing messages (requests/notifications sent).
    outgoing: RwLock<VecDeque<Message>>,
    /// Incoming messages (responses/notifications received).
    incoming: RwLock<VecDeque<Message>>,
}

impl MessageQueue {
    /// Create a new message queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue an outgoing message.
    pub fn queue_outgoing(&self, message: Message) {
        if let Ok(mut queue) = self.outgoing.write() {
            queue.push_back(message);
        }
    }

    /// Queue an incoming message.
    pub fn queue_incoming(&self, message: Message) {
        if let Ok(mut queue) = self.incoming.write() {
            queue.push_back(message);
        }
    }

    /// Take the next outgoing message.
    pub fn take_outgoing(&self) -> Option<Message> {
        self.outgoing.write().ok()?.pop_front()
    }

    /// Take the next incoming message.
    pub fn take_incoming(&self) -> Option<Message> {
        self.incoming.write().ok()?.pop_front()
    }

    /// Check if there are pending outgoing messages.
    #[must_use]
    pub fn has_outgoing(&self) -> bool {
        self.outgoing.read().map(|q| !q.is_empty()).unwrap_or(false)
    }

    /// Check if there are pending incoming messages.
    #[must_use]
    pub fn has_incoming(&self) -> bool {
        self.incoming.read().map(|q| !q.is_empty()).unwrap_or(false)
    }

    /// Get count of outgoing messages.
    #[must_use]
    pub fn outgoing_count(&self) -> usize {
        self.outgoing.read().map(|q| q.len()).unwrap_or(0)
    }

    /// Get count of incoming messages.
    #[must_use]
    pub fn incoming_count(&self) -> usize {
        self.incoming.read().map(|q| q.len()).unwrap_or(0)
    }

    /// Clear all messages.
    pub fn clear(&self) {
        if let Ok(mut queue) = self.outgoing.write() {
            queue.clear();
        }
        if let Ok(mut queue) = self.incoming.write() {
            queue.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_matcher_success() {
        let matcher = ResponseMatcher::success();
        let response = Response::success(RequestId::from(1), serde_json::json!({}));
        assert!(matcher.validate(&response).is_ok());
    }

    #[test]
    fn test_response_matcher_error() {
        let matcher = ResponseMatcher::error();
        let response = Response::error(
            RequestId::from(1),
            mcpkit_core::JsonRpcError::invalid_request("Invalid"),
        );
        assert!(matcher.validate(&response).is_ok());
    }

    #[test]
    fn test_response_matcher_json_path() {
        let matcher = ResponseMatcher::success()
            .with_json("name", serde_json::json!("test"))
            .with_json("count", serde_json::json!(42));

        let response = Response::success(
            RequestId::from(1),
            serde_json::json!({
                "name": "test",
                "count": 42
            }),
        );
        assert!(matcher.validate(&response).is_ok());
    }

    #[test]
    fn test_json_path_nested() {
        let value = serde_json::json!({
            "user": {
                "profile": {
                    "name": "Alice"
                }
            }
        });

        let result = get_json_path(&value, "user.profile.name");
        assert_eq!(result, Some(&serde_json::json!("Alice")));
    }

    #[test]
    fn test_notification_matcher() {
        let matcher = NotificationMatcher::new().method("test/notify");
        let notification = Notification::new("test/notify");
        assert!(matcher.validate(&notification).is_ok());
    }

    #[test]
    fn test_scenario_builder() {
        let scenario = TestScenario::new("test-scenario")
            .description("A test scenario")
            .request(Request::new("ping", 1), ResponseMatcher::success());

        assert_eq!(scenario.name, "test-scenario");
        assert_eq!(scenario.steps.len(), 1);
    }

    #[test]
    fn test_message_queue() {
        let queue = MessageQueue::new();

        queue.queue_outgoing(Message::Request(Request::new("test", 1)));
        queue.queue_incoming(Message::Response(Response::success(
            RequestId::from(1),
            serde_json::json!({}),
        )));

        assert!(queue.has_outgoing());
        assert!(queue.has_incoming());
        assert_eq!(queue.outgoing_count(), 1);
        assert_eq!(queue.incoming_count(), 1);

        let _ = queue.take_outgoing();
        let _ = queue.take_incoming();

        assert!(!queue.has_outgoing());
        assert!(!queue.has_incoming());
    }
}
