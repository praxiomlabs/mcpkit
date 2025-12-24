//! Mock client for testing MCP servers.
//!
//! This module provides a mock client that can be used to test
//! MCP server implementations.

use mcpkit_core::capability::{ClientCapabilities, ClientInfo, ServerCapabilities, ServerInfo};
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Notification, Request, RequestId, Response};
use mcpkit_core::types::{
    CallToolResult, GetPromptResult, Prompt, Resource, ResourceContents, Tool,
};
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// A mock MCP client for testing servers.
///
/// The mock client tracks all interactions and provides utilities
/// for verifying server behavior.
#[derive(Debug)]
pub struct MockClient {
    /// Client info.
    info: ClientInfo,
    /// Client capabilities.
    capabilities: ClientCapabilities,
    /// Next request ID.
    next_id: AtomicU64,
    /// Pending requests.
    pending: RwLock<HashMap<RequestId, String>>,
    /// Recorded requests.
    requests: RwLock<Vec<Request>>,
    /// Recorded responses.
    responses: RwLock<Vec<Response>>,
    /// Recorded notifications sent.
    notifications_sent: RwLock<Vec<Notification>>,
    /// Recorded notifications received.
    notifications_received: RwLock<Vec<Notification>>,
    /// Server info (after initialize).
    server_info: RwLock<Option<ServerInfo>>,
    /// Server capabilities (after initialize).
    server_capabilities: RwLock<Option<ServerCapabilities>>,
}

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClient {
    /// Create a new mock client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            info: ClientInfo::new("mock-client", "1.0.0"),
            capabilities: ClientCapabilities::new(),
            next_id: AtomicU64::new(1),
            pending: RwLock::new(HashMap::new()),
            requests: RwLock::new(Vec::new()),
            responses: RwLock::new(Vec::new()),
            notifications_sent: RwLock::new(Vec::new()),
            notifications_received: RwLock::new(Vec::new()),
            server_info: RwLock::new(None),
            server_capabilities: RwLock::new(None),
        }
    }

    /// Create a mock client with custom info.
    pub fn with_info(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.info = ClientInfo::new(name, version);
        self
    }

    /// Create a mock client with custom capabilities.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: ClientCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Get the client info.
    #[must_use]
    pub fn info(&self) -> &ClientInfo {
        &self.info
    }

    /// Get the client capabilities.
    #[must_use]
    pub fn capabilities(&self) -> &ClientCapabilities {
        &self.capabilities
    }

    /// Get the server info (after initialization).
    #[must_use]
    pub fn server_info(&self) -> Option<ServerInfo> {
        self.server_info.read().ok()?.clone()
    }

    /// Get the server capabilities (after initialization).
    #[must_use]
    pub fn server_capabilities(&self) -> Option<ServerCapabilities> {
        self.server_capabilities.read().ok()?.clone()
    }

    /// Create an initialize request.
    #[must_use]
    pub fn create_initialize_request(&self) -> Request {
        let id = self.next_request_id();
        Request::new("initialize", id).params(serde_json::json!({
            "protocolVersion": mcpkit_core::PROTOCOL_VERSION,
            "capabilities": self.capabilities,
            "clientInfo": self.info
        }))
    }

    /// Process an initialize response.
    pub fn process_initialize_response(&self, response: &Response) -> Result<(), McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: format!("Initialize failed: {}", error.message),
            });
        }

        if let Some(result) = &response.result {
            // Extract server info
            if let Some(server_info) = result.get("serverInfo") {
                let info: ServerInfo = serde_json::from_value(server_info.clone())?;
                if let Ok(mut lock) = self.server_info.write() {
                    *lock = Some(info);
                }
            }

            // Extract capabilities
            if let Some(caps) = result.get("capabilities") {
                let capabilities: ServerCapabilities = serde_json::from_value(caps.clone())?;
                if let Ok(mut lock) = self.server_capabilities.write() {
                    *lock = Some(capabilities);
                }
            }
        }

        Ok(())
    }

    /// Create an initialized notification.
    #[must_use]
    pub fn create_initialized_notification(&self) -> Notification {
        Notification::new("initialized")
    }

    /// Create a tools/list request.
    #[must_use]
    pub fn create_list_tools_request(&self) -> Request {
        let id = self.next_request_id();
        Request::new("tools/list", id)
    }

    /// Create a tools/call request.
    #[must_use]
    pub fn create_call_tool_request(&self, name: &str, arguments: serde_json::Value) -> Request {
        let id = self.next_request_id();
        Request::new("tools/call", id).params(serde_json::json!({
            "name": name,
            "arguments": arguments
        }))
    }

    /// Create a resources/list request.
    #[must_use]
    pub fn create_list_resources_request(&self) -> Request {
        let id = self.next_request_id();
        Request::new("resources/list", id)
    }

    /// Create a resources/read request.
    #[must_use]
    pub fn create_read_resource_request(&self, uri: &str) -> Request {
        let id = self.next_request_id();
        Request::new("resources/read", id).params(serde_json::json!({
            "uri": uri
        }))
    }

    /// Create a prompts/list request.
    #[must_use]
    pub fn create_list_prompts_request(&self) -> Request {
        let id = self.next_request_id();
        Request::new("prompts/list", id)
    }

    /// Create a prompts/get request.
    pub fn create_get_prompt_request(
        &self,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Request {
        let id = self.next_request_id();
        let mut params = serde_json::json!({ "name": name });
        if let Some(args) = arguments {
            params["arguments"] = serde_json::Value::Object(args);
        }
        Request::new("prompts/get", id).params(params)
    }

    /// Create a ping request.
    #[must_use]
    pub fn create_ping_request(&self) -> Request {
        let id = self.next_request_id();
        Request::new("ping", id)
    }

    /// Record a request.
    pub fn record_request(&self, request: Request) {
        if let Ok(mut pending) = self.pending.write() {
            pending.insert(request.id.clone(), request.method.to_string());
        }
        if let Ok(mut requests) = self.requests.write() {
            requests.push(request);
        }
    }

    /// Record a response.
    pub fn record_response(&self, response: Response) {
        if let Ok(mut pending) = self.pending.write() {
            pending.remove(&response.id);
        }
        if let Ok(mut responses) = self.responses.write() {
            responses.push(response);
        }
    }

    /// Record a sent notification.
    pub fn record_notification_sent(&self, notification: Notification) {
        if let Ok(mut notifications) = self.notifications_sent.write() {
            notifications.push(notification);
        }
    }

    /// Record a received notification.
    pub fn record_notification_received(&self, notification: Notification) {
        if let Ok(mut notifications) = self.notifications_received.write() {
            notifications.push(notification);
        }
    }

    /// Get all recorded requests.
    #[must_use]
    pub fn requests(&self) -> Vec<Request> {
        self.requests.read().map(|r| r.clone()).unwrap_or_default()
    }

    /// Get all recorded responses.
    #[must_use]
    pub fn responses(&self) -> Vec<Response> {
        self.responses.read().map(|r| r.clone()).unwrap_or_default()
    }

    /// Get all sent notifications.
    #[must_use]
    pub fn notifications_sent(&self) -> Vec<Notification> {
        self.notifications_sent
            .read()
            .map(|n| n.clone())
            .unwrap_or_default()
    }

    /// Get all received notifications.
    #[must_use]
    pub fn notifications_received(&self) -> Vec<Notification> {
        self.notifications_received
            .read()
            .map(|n| n.clone())
            .unwrap_or_default()
    }

    /// Get pending request count.
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.pending.read().map(|p| p.len()).unwrap_or(0)
    }

    /// Get the total request count.
    #[must_use]
    pub fn request_count(&self) -> usize {
        self.requests.read().map(|r| r.len()).unwrap_or(0)
    }

    /// Get the total response count.
    #[must_use]
    pub fn response_count(&self) -> usize {
        self.responses.read().map(|r| r.len()).unwrap_or(0)
    }

    /// Clear all recorded data.
    pub fn clear(&self) {
        if let Ok(mut pending) = self.pending.write() {
            pending.clear();
        }
        if let Ok(mut requests) = self.requests.write() {
            requests.clear();
        }
        if let Ok(mut responses) = self.responses.write() {
            responses.clear();
        }
        if let Ok(mut notifications) = self.notifications_sent.write() {
            notifications.clear();
        }
        if let Ok(mut notifications) = self.notifications_received.write() {
            notifications.clear();
        }
    }

    /// Parse a tool list response.
    pub fn parse_tool_list(&self, response: &Response) -> Result<Vec<Tool>, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        let tools = result
            .get("tools")
            .ok_or_else(|| McpError::InternalMessage {
                message: "No tools in response".to_string(),
            })?;

        Ok(serde_json::from_value(tools.clone())?)
    }

    /// Parse a tool call response.
    pub fn parse_tool_call(&self, response: &Response) -> Result<CallToolResult, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        Ok(serde_json::from_value(result.clone())?)
    }

    /// Parse a resource list response.
    pub fn parse_resource_list(&self, response: &Response) -> Result<Vec<Resource>, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        let resources = result
            .get("resources")
            .ok_or_else(|| McpError::InternalMessage {
                message: "No resources in response".to_string(),
            })?;

        Ok(serde_json::from_value(resources.clone())?)
    }

    /// Parse a resource read response.
    pub fn parse_resource_read(
        &self,
        response: &Response,
    ) -> Result<Vec<ResourceContents>, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        let contents = result
            .get("contents")
            .ok_or_else(|| McpError::InternalMessage {
                message: "No contents in response".to_string(),
            })?;

        Ok(serde_json::from_value(contents.clone())?)
    }

    /// Parse a prompt list response.
    pub fn parse_prompt_list(&self, response: &Response) -> Result<Vec<Prompt>, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        let prompts = result
            .get("prompts")
            .ok_or_else(|| McpError::InternalMessage {
                message: "No prompts in response".to_string(),
            })?;

        Ok(serde_json::from_value(prompts.clone())?)
    }

    /// Parse a prompt get response.
    pub fn parse_prompt_get(&self, response: &Response) -> Result<GetPromptResult, McpError> {
        if let Some(error) = &response.error {
            return Err(McpError::InternalMessage {
                message: error.message.clone(),
            });
        }

        let result = response
            .result
            .as_ref()
            .ok_or_else(|| McpError::InternalMessage {
                message: "No result in response".to_string(),
            })?;

        Ok(serde_json::from_value(result.clone())?)
    }

    fn next_request_id(&self) -> RequestId {
        RequestId::from(self.next_id.fetch_add(1, Ordering::SeqCst))
    }
}

impl Clone for MockClient {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            capabilities: self.capabilities.clone(),
            next_id: AtomicU64::new(self.next_id.load(Ordering::SeqCst)),
            pending: RwLock::new(self.pending.read().map(|p| p.clone()).unwrap_or_default()),
            requests: RwLock::new(self.requests.read().map(|r| r.clone()).unwrap_or_default()),
            responses: RwLock::new(self.responses.read().map(|r| r.clone()).unwrap_or_default()),
            notifications_sent: RwLock::new(
                self.notifications_sent
                    .read()
                    .map(|n| n.clone())
                    .unwrap_or_default(),
            ),
            notifications_received: RwLock::new(
                self.notifications_received
                    .read()
                    .map(|n| n.clone())
                    .unwrap_or_default(),
            ),
            server_info: RwLock::new(self.server_info.read().ok().and_then(|s| s.clone())),
            server_capabilities: RwLock::new(
                self.server_capabilities.read().ok().and_then(|s| s.clone()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_client_creation() {
        let client = MockClient::new().with_info("test-client", "2.0.0");

        assert_eq!(client.info().name, "test-client");
        assert_eq!(client.info().version, "2.0.0");
    }

    #[test]
    fn test_create_requests() {
        let client = MockClient::new();

        let init = client.create_initialize_request();
        assert_eq!(init.method.as_ref(), "initialize");

        let ping = client.create_ping_request();
        assert_eq!(ping.method.as_ref(), "ping");

        let list_tools = client.create_list_tools_request();
        assert_eq!(list_tools.method.as_ref(), "tools/list");

        let call_tool = client.create_call_tool_request("test", serde_json::json!({}));
        assert_eq!(call_tool.method.as_ref(), "tools/call");
    }

    #[test]
    fn test_record_requests() {
        let client = MockClient::new();

        let request = client.create_ping_request();
        client.record_request(request);

        assert_eq!(client.request_count(), 1);
        assert_eq!(client.pending_count(), 1);

        let response = Response::success(RequestId::from(1), serde_json::json!({}));
        client.record_response(response);

        assert_eq!(client.response_count(), 1);
        assert_eq!(client.pending_count(), 0);
    }

    #[test]
    fn test_parse_tool_list() {
        let client = MockClient::new();

        let response = Response::success(
            RequestId::from(1),
            serde_json::json!({
                "tools": [
                    {"name": "test", "inputSchema": {"type": "object"}}
                ]
            }),
        );

        let tools = client.parse_tool_list(&response).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test");
    }

    #[test]
    fn test_parse_resource_list() {
        let client = MockClient::new();

        let response = Response::success(
            RequestId::from(1),
            serde_json::json!({
                "resources": [
                    {"uri": "test://resource", "name": "Test"}
                ]
            }),
        );

        let resources = client.parse_resource_list(&response).unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "test://resource");
    }
}
