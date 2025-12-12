//! Elicitation capability implementation.
//!
//! This module provides support for user input elicitation
//! in MCP servers.

use crate::context::Context;
use mcpkit_core::error::McpError;
use mcpkit_core::types::elicitation::{
    ElicitAction, ElicitRequest, ElicitResult, ElicitationSchema,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// A boxed async function for handling elicitation requests.
pub type BoxedElicitationFn = Box<
    dyn for<'a> Fn(
            ElicitRequest,
            &'a Context<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<ElicitResult, McpError>> + Send + 'a>>
        + Send
        + Sync,
>;

/// Service for handling elicitation requests.
///
/// Elicitation allows servers to request input from users
/// through the client interface.
pub struct ElicitationService {
    handler: Option<BoxedElicitationFn>,
}

impl Default for ElicitationService {
    fn default() -> Self {
        Self::new()
    }
}

impl ElicitationService {
    /// Create a new elicitation service without a handler.
    #[must_use]
    pub fn new() -> Self {
        Self { handler: None }
    }

    /// Set the elicitation handler.
    pub fn with_handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(ElicitRequest, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ElicitResult, McpError>> + Send + 'static,
    {
        self.handler = Some(Box::new(move |req, ctx| Box::pin(handler(req, ctx))));
        self
    }

    /// Check if elicitation is supported.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        self.handler.is_some()
    }

    /// Create an elicitation request.
    pub async fn elicit(
        &self,
        request: ElicitRequest,
        ctx: &Context<'_>,
    ) -> Result<ElicitResult, McpError> {
        let handler = self
            .handler
            .as_ref()
            .ok_or_else(|| McpError::invalid_request("Elicitation not supported"))?;

        (handler)(request, ctx).await
    }
}

/// Builder for creating elicitation requests.
pub struct ElicitationRequestBuilder {
    message: String,
    schema: Option<ElicitationSchema>,
}

impl ElicitationRequestBuilder {
    /// Create a new request builder.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            schema: None,
        }
    }

    /// Set the response schema.
    #[must_use]
    pub fn schema(mut self, schema: ElicitationSchema) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Request a text response.
    pub fn text_response(self, field_name: impl Into<String>) -> Self {
        self.schema(ElicitationSchema::object().property(
            field_name,
            mcpkit_core::types::elicitation::PropertySchema::string(),
        ))
    }

    /// Request a boolean response.
    pub fn boolean_response(self, field_name: impl Into<String>) -> Self {
        self.schema(ElicitationSchema::object().property(
            field_name,
            mcpkit_core::types::elicitation::PropertySchema::boolean(),
        ))
    }

    /// Build the request.
    pub fn build(self) -> ElicitRequest {
        ElicitRequest {
            message: self.message,
            requested_schema: self.schema.unwrap_or_else(ElicitationSchema::object),
        }
    }
}

/// Builder for creating elicitation results.
pub struct ElicitationResultBuilder {
    action: ElicitAction,
    content: Option<serde_json::Map<String, Value>>,
}

impl ElicitationResultBuilder {
    /// Create a new result builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            action: ElicitAction::Accept,
            content: None,
        }
    }

    /// Set the result as accepted with content.
    #[must_use]
    pub fn accepted(mut self, content: Value) -> Self {
        self.action = ElicitAction::Accept;
        // Convert Value to Map if it's an object, otherwise wrap it
        self.content = match content {
            Value::Object(map) => Some(map),
            other => {
                let mut map = serde_json::Map::new();
                map.insert("value".to_string(), other);
                Some(map)
            }
        };
        self
    }

    /// Set the result as accepted with a map.
    #[must_use]
    pub fn accepted_map(mut self, content: serde_json::Map<String, Value>) -> Self {
        self.action = ElicitAction::Accept;
        self.content = Some(content);
        self
    }

    /// Set the result as declined.
    #[must_use]
    pub fn declined(mut self) -> Self {
        self.action = ElicitAction::Decline;
        self.content = None;
        self
    }

    /// Set the result as cancelled.
    #[must_use]
    pub fn cancelled(mut self) -> Self {
        self.action = ElicitAction::Cancel;
        self.content = None;
        self
    }

    /// Build the result.
    #[must_use]
    pub fn build(self) -> ElicitResult {
        ElicitResult {
            action: self.action,
            content: self.content,
        }
    }
}

impl Default for ElicitationResultBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elicitation_request_builder() {
        let request = ElicitationRequestBuilder::new("Please enter your name")
            .text_response("name")
            .build();

        assert_eq!(request.message, "Please enter your name");
    }

    #[test]
    fn test_elicitation_result_builder() {
        let result = ElicitationResultBuilder::new()
            .accepted(serde_json::json!({"name": "John Doe"}))
            .build();

        assert_eq!(result.action, ElicitAction::Accept);
        assert!(result.content.is_some());
    }

    #[test]
    fn test_elicitation_service_default() {
        let service = ElicitationService::new();
        assert!(!service.is_supported());
    }

    #[tokio::test]
    async fn test_elicitation_service_with_handler() {
        let service = ElicitationService::new().with_handler(|_req, _ctx| async {
            Ok(ElicitationResultBuilder::new()
                .accepted(serde_json::json!({"response": "test"}))
                .build())
        });

        assert!(service.is_supported());
    }
}
