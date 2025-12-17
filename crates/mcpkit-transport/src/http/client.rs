//! HTTP transport client implementation.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use mcpkit_core::protocol::Message;

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};

use super::config::{HttpTransportBuilder, HttpTransportConfig};
use super::sse::{HttpTransportState, process_sse_buffer};

#[cfg(feature = "http")]
use {
    super::config::{MCP_PROTOCOL_VERSION_HEADER, MCP_SESSION_ID_HEADER},
    bytes::Bytes,
    futures::StreamExt,
    reqwest::{
        Client, Response, StatusCode,
        header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue},
    },
};

/// HTTP transport with SSE streaming support.
///
/// This transport implements the MCP Streamable HTTP transport specification.
/// It sends messages via HTTP POST and receives responses either as direct
/// JSON or via Server-Sent Events (SSE) streaming.
pub struct HttpTransport {
    config: HttpTransportConfig,
    state: AsyncMutex<HttpTransportState>,
    connected: AtomicBool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    #[cfg(feature = "http")]
    client: Client,
}

impl HttpTransport {
    /// Create a new HTTP transport with the given configuration.
    ///
    /// This creates the HTTP client but does not connect to the server.
    /// Connection is established on first send.
    #[cfg(feature = "http")]
    pub fn new(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to create HTTP client: {e}"),
            })?;

        let session_id = config.session_id.clone();
        Ok(Self {
            config,
            state: AsyncMutex::new(HttpTransportState::new(session_id)),
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            client,
        })
    }

    /// Create a new HTTP transport without the http feature (stub).
    #[cfg(not(feature = "http"))]
    pub fn new(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let session_id = config.session_id.clone();
        Ok(Self {
            config,
            state: AsyncMutex::new(HttpTransportState::new(session_id)),
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        })
    }

    /// Connect to the MCP server and establish a session.
    ///
    /// This is a convenience method that creates the transport and
    /// marks it as connected. The actual HTTP connection is made
    /// on the first send operation.
    pub async fn connect(config: HttpTransportConfig) -> Result<Self, TransportError> {
        let transport = Self::new(config)?;
        transport.connected.store(true, Ordering::Release);
        Ok(transport)
    }

    /// Get the current session ID, if any.
    pub async fn session_id(&self) -> Option<String> {
        self.state.lock().await.session_id.clone()
    }

    /// Set the session ID.
    pub async fn set_session_id(&self, session_id: impl Into<String>) {
        self.state.lock().await.session_id = Some(session_id.into());
    }

    /// Get the number of messages sent.
    #[must_use]
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    #[must_use]
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Get the last event ID for SSE resumption.
    #[must_use]
    pub async fn last_event_id(&self) -> Option<String> {
        self.state.lock().await.last_event_id.clone()
    }

    /// Build headers for requests.
    #[cfg(feature = "http")]
    fn build_headers(&self, session_id: Option<&str>) -> Result<HeaderMap, TransportError> {
        let mut headers = HeaderMap::new();

        // Required headers per MCP spec
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert(
            MCP_PROTOCOL_VERSION_HEADER,
            HeaderValue::from_str(&self.config.protocol_version).map_err(|e| {
                TransportError::Connection {
                    message: format!("Invalid protocol version header: {e}"),
                }
            })?,
        );

        // Session ID if available
        if let Some(sid) = session_id {
            headers.insert(
                MCP_SESSION_ID_HEADER,
                HeaderValue::from_str(sid).map_err(|e| TransportError::Connection {
                    message: format!("Invalid session ID header: {e}"),
                })?,
            );
        }

        // Custom headers
        for (name, value) in &self.config.headers {
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|e| {
                    TransportError::Connection {
                        message: format!("Invalid header name '{name}': {e}"),
                    }
                })?,
                HeaderValue::from_str(value).map_err(|e| TransportError::Connection {
                    message: format!("Invalid header value for '{name}': {e}"),
                })?,
            );
        }

        Ok(headers)
    }

    /// Send a message and handle the response.
    #[cfg(feature = "http")]
    async fn send_post(&self, msg: &Message) -> Result<(), TransportError> {
        let body = serde_json::to_string(msg).map_err(|e| TransportError::Serialization {
            message: format!("Failed to serialize message: {e}"),
        })?;

        // Check message size limit
        if body.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: body.len(),
                max: self.config.max_message_size,
            });
        }

        let session_id = self.state.lock().await.session_id.clone();
        let headers = self.build_headers(session_id.as_deref())?;

        let response = self
            .client
            .post(&self.config.base_url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("HTTP POST failed: {e}"),
            })?;

        self.handle_response(response).await?;
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.connected.store(true, Ordering::Release);

        Ok(())
    }

    /// Handle the HTTP response, which may be JSON or SSE.
    #[cfg(feature = "http")]
    async fn handle_response(&self, response: Response) -> Result<(), TransportError> {
        let status = response.status();

        // Check for session ID in response headers
        if let Some(session_id) = response.headers().get(MCP_SESSION_ID_HEADER) {
            if let Ok(sid) = session_id.to_str() {
                self.state.lock().await.session_id = Some(sid.to_string());
            }
        }

        match status {
            StatusCode::OK => {
                let content_type = response
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/json");

                if content_type.starts_with("text/event-stream") {
                    // Handle SSE stream
                    self.process_sse_stream(response).await
                } else {
                    // Handle direct JSON response
                    self.process_json_response(response).await
                }
            }
            StatusCode::ACCEPTED => {
                // 202 Accepted - no response body (for notifications)
                Ok(())
            }
            StatusCode::BAD_REQUEST => {
                let body = response.text().await.unwrap_or_default();
                Err(TransportError::Protocol {
                    message: format!("Bad request: {body}"),
                })
            }
            StatusCode::NOT_FOUND => {
                // Session expired
                self.state.lock().await.session_id = None;
                Err(TransportError::Connection {
                    message: "Session expired or not found".to_string(),
                })
            }
            _ => Err(TransportError::Protocol {
                message: format!("Unexpected status code: {status}"),
            }),
        }
    }

    /// Process a direct JSON response.
    #[cfg(feature = "http")]
    async fn process_json_response(&self, response: Response) -> Result<(), TransportError> {
        let body = response
            .text()
            .await
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to read response body: {e}"),
            })?;

        if body.is_empty() {
            return Ok(());
        }

        // Check message size limit
        if body.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: body.len(),
                max: self.config.max_message_size,
            });
        }

        let msg: Message =
            serde_json::from_str(&body).map_err(|e| TransportError::Serialization {
                message: format!("Failed to parse response: {e}"),
            })?;

        self.state.lock().await.message_queue.push_back(msg);
        self.messages_received.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Process an SSE stream.
    #[cfg(feature = "http")]
    async fn process_sse_stream(&self, response: Response) -> Result<(), TransportError> {
        let mut stream = response.bytes_stream();
        let mut state = self.state.lock().await;

        while let Some(chunk_result) = stream.next().await {
            let chunk: Bytes = chunk_result.map_err(|e| TransportError::Connection {
                message: format!("SSE stream error: {e}"),
            })?;

            let chunk_str = std::str::from_utf8(&chunk).map_err(|e| TransportError::Protocol {
                message: format!("Invalid UTF-8 in SSE stream: {e}"),
            })?;

            state.sse_buffer.push_str(chunk_str);

            // Process complete events
            process_sse_buffer(
                &mut state,
                &self.messages_received,
                self.config.max_message_size,
            )?;
        }

        Ok(())
    }

    /// Stub for `send_post` when http feature is disabled.
    #[cfg(not(feature = "http"))]
    async fn send_post(&self, _msg: &Message) -> Result<(), TransportError> {
        Err(TransportError::Connection {
            message: "HTTP transport requires the 'http' feature".to_string(),
        })
    }

    /// Process SSE buffer (for non-http feature builds).
    #[cfg(not(feature = "http"))]
    pub fn process_sse_buffer_internal(
        &self,
        state: &mut HttpTransportState,
    ) -> Result<(), TransportError> {
        process_sse_buffer(state, &self.messages_received, self.config.max_message_size)
    }
}

impl Transport for HttpTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        self.send_post(&msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        let mut state = self.state.lock().await;

        // Return queued messages first
        if let Some(msg) = state.message_queue.pop_front() {
            return Ok(Some(msg));
        }

        // If no queued messages and not connected, return None
        if !self.connected.load(Ordering::Acquire) {
            return Ok(None);
        }

        Ok(None)
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::Release);

        #[cfg(feature = "http")]
        {
            // Send DELETE to terminate session if we have a session ID
            let session_id = self.state.lock().await.session_id.clone();
            if let Some(_sid) = session_id {
                let headers = self.build_headers(None)?;
                let _ = self
                    .client
                    .delete(&self.config.base_url)
                    .headers(headers)
                    .send()
                    .await;
            }
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("http").remote_addr(&self.config.base_url)
    }
}

impl HttpTransportBuilder {
    /// Build the transport.
    pub fn build(self) -> Result<HttpTransport, TransportError> {
        HttpTransport::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_config_builder() {
        let config = HttpTransportConfig::new("http://example.com/mcp")
            .with_session_id("session-123")
            .with_connect_timeout(Duration::from_secs(10))
            .with_header("X-Custom", "value")
            .with_protocol_version("2025-06-18");

        assert_eq!(config.base_url, "http://example.com/mcp");
        assert_eq!(config.session_id, Some("session-123".to_string()));
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.headers.len(), 1);
        assert_eq!(config.protocol_version, "2025-06-18");
    }

    #[test]
    fn test_transport_builder() {
        let transport = HttpTransportBuilder::new("http://example.com/mcp")
            .session_id("test-session")
            .connect_timeout(Duration::from_secs(5))
            .header("Authorization", "Bearer token")
            .build()
            .unwrap();

        assert!(!transport.is_connected());
        assert_eq!(transport.messages_sent(), 0);
        assert_eq!(transport.messages_received(), 0);
    }

    #[tokio::test]
    async fn test_transport_metadata() {
        let transport =
            HttpTransport::new(HttpTransportConfig::new("http://localhost:8080")).unwrap();
        let metadata = transport.metadata();

        assert_eq!(metadata.transport_type, "http");
        assert_eq!(
            metadata.remote_addr,
            Some("http://localhost:8080".to_string())
        );
    }

    #[tokio::test]
    async fn test_session_id_management() {
        let transport =
            HttpTransport::new(HttpTransportConfig::new("http://localhost:8080")).unwrap();

        assert!(transport.session_id().await.is_none());

        transport.set_session_id("test-session-123").await;
        assert_eq!(
            transport.session_id().await,
            Some("test-session-123".to_string())
        );
    }
}
