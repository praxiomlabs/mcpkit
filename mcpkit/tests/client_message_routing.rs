//! Client message routing integration tests.
//!
//! These tests verify that the client's message router correctly correlates
//! requests and responses without spurious warnings.
//!
//! Run with `RUST_LOG=mcpkit_client=warn` to see any warning messages.

use mcpkit::protocol::{Message, Response};
use mcpkit_client::ClientBuilder;
use mcpkit_transport::{MemoryTransport, Transport};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Test that the client message router correctly handles request/response correlation.
/// This test previously failed due to warnings about "Received response for unknown request"
/// because responses weren't being properly correlated with pending requests.
#[tokio::test]
async fn test_client_request_response_correlation() {
    // Create a memory transport pair
    let (client_transport, server_transport) = MemoryTransport::pair();

    // Spawn a fake server that handles requests
    let server_transport = Arc::new(Mutex::new(server_transport));
    let server_clone = Arc::clone(&server_transport);

    // Server task to handle initialization and requests
    let server_handle = tokio::spawn(async move {
        let transport = server_clone.lock().await;

        // 1. Handle initialize request
        let msg = transport.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        assert_eq!(request.method, "initialize");

        let init_response = Response::success(
            request.id.clone(),
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "test-server", "version": "1.0.0"}
            }),
        );
        transport
            .send(Message::Response(init_response))
            .await
            .unwrap();

        // 2. Handle initialized notification
        let msg = transport.recv().await.unwrap().unwrap();
        assert!(msg.is_notification());

        // 3. Handle tools/list request
        let msg = transport.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        assert_eq!(request.method, "tools/list");

        let tools_response = Response::success(
            request.id.clone(),
            json!({
                "tools": [{"name": "test_tool", "description": "A test tool", "inputSchema": {"type": "object"}}]
            }),
        );
        transport
            .send(Message::Response(tools_response))
            .await
            .unwrap();

        // 4. Handle tools/call request
        let msg = transport.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        assert_eq!(request.method, "tools/call");

        let call_response = Response::success(
            request.id.clone(),
            json!({
                "content": [{"type": "text", "text": "result"}],
                "isError": false
            }),
        );
        transport
            .send(Message::Response(call_response))
            .await
            .unwrap();

        // 5. Handle another tools/call request
        let msg = transport.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        assert_eq!(request.method, "tools/call");

        let call_response2 = Response::success(
            request.id.clone(),
            json!({
                "content": [{"type": "text", "text": "result2"}],
                "isError": false
            }),
        );
        transport
            .send(Message::Response(call_response2))
            .await
            .unwrap();
    });

    // Build client and make requests
    let client = ClientBuilder::new()
        .name("test-client")
        .version("1.0.0")
        .build(client_transport)
        .await
        .expect("Client should connect successfully");

    // Make multiple requests - these should all succeed without warnings
    let tools = client
        .list_tools()
        .await
        .expect("list_tools should succeed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_tool");

    let result1 = client
        .call_tool("test_tool", json!({"arg": "value1"}))
        .await
        .expect("call_tool should succeed");
    assert!(!result1.is_error.unwrap_or(false));

    let result2 = client
        .call_tool("test_tool", json!({"arg": "value2"}))
        .await
        .expect("call_tool should succeed");
    assert!(!result2.is_error.unwrap_or(false));

    // Wait for server to finish
    server_handle.await.unwrap();
}

/// Test that multiple sequential requests work without warnings
#[tokio::test]
async fn test_sequential_requests() {
    let (client_transport, server_transport) = MemoryTransport::pair();
    let server_transport = Arc::new(Mutex::new(server_transport));
    let server_clone = Arc::clone(&server_transport);

    let server_handle = tokio::spawn(async move {
        let transport = server_clone.lock().await;

        // Handle initialize
        let msg = transport.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        transport
            .send(Message::Response(Response::success(
                request.id.clone(),
                json!({
                    "protocolVersion": "2025-11-25",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "test", "version": "1.0"}
                }),
            )))
            .await
            .unwrap();

        // Handle initialized
        let _ = transport.recv().await.unwrap().unwrap();

        // Handle 10 sequential requests
        for _ in 0..10 {
            let msg = transport.recv().await.unwrap().unwrap();
            let request = msg.as_request().unwrap();
            transport
                .send(Message::Response(Response::success(
                    request.id.clone(),
                    json!({"tools": []}),
                )))
                .await
                .unwrap();
        }
    });

    let client = ClientBuilder::new()
        .name("test")
        .version("1.0")
        .build(client_transport)
        .await
        .unwrap();

    // Make 10 sequential requests
    for _ in 0..10 {
        let tools = client.list_tools().await.expect("request should succeed");
        assert!(tools.is_empty());
    }

    server_handle.await.unwrap();
}
