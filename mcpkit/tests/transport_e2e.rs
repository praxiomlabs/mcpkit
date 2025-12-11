//! End-to-end transport tests for MCP SDK.
//!
//! These tests verify that client-server communication works correctly
//! over different transport layers.

use mcpkit::protocol::{Message, Notification, Request, RequestId, Response};
use mcpkit_transport::{MemoryTransport, Transport};
use serde_json::json;

/// Test that messages can be sent and received over a memory transport
#[tokio::test]
async fn test_memory_transport_basic() {
    let (client, server) = MemoryTransport::pair();

    // Both ends should be connected
    assert!(client.is_connected());
    assert!(server.is_connected());

    // Send a request from client
    let request = Request::new("tools/list", 1u64);
    client.send(Message::Request(request)).await.unwrap();

    // Receive on server
    let received = server.recv().await.unwrap().unwrap();
    assert!(matches!(received, Message::Request(_)));

    // Send a response from server
    let response = Response::success(1u64, json!({"tools": []}));
    server.send(Message::Response(response)).await.unwrap();

    // Receive on client
    let received = client.recv().await.unwrap().unwrap();
    assert!(matches!(received, Message::Response(_)));
}

/// Test request/response roundtrip
#[tokio::test]
async fn test_request_response_roundtrip() {
    let (client, server) = MemoryTransport::pair();

    // Client sends initialize request
    let init_request = Request::with_params(
        "initialize",
        1u64,
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }),
    );
    client.send(Message::Request(init_request)).await.unwrap();

    // Server receives
    let msg = server.recv().await.unwrap().unwrap();
    let request = msg.as_request().unwrap();
    assert_eq!(request.method, "initialize");
    assert_eq!(request.id, RequestId::Number(1));

    // Server sends response
    let init_response = Response::success(
        1u64,
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "test-server",
                "version": "1.0.0"
            }
        }),
    );
    server.send(Message::Response(init_response)).await.unwrap();

    // Client receives
    let msg = client.recv().await.unwrap().unwrap();
    let response = msg.as_response().unwrap();
    assert!(response.is_success());

    let result = response.result.as_ref().unwrap();
    assert_eq!(result["protocolVersion"], "2025-11-25");
    assert_eq!(result["serverInfo"]["name"], "test-server");
}

/// Test notification (no response expected)
#[tokio::test]
async fn test_notification_roundtrip() {
    let (client, server) = MemoryTransport::pair();

    // Client sends initialized notification
    let notification = Notification::new("notifications/initialized");
    client
        .send(Message::Notification(notification))
        .await
        .unwrap();

    // Server receives
    let msg = server.recv().await.unwrap().unwrap();
    let notification = msg.as_notification().unwrap();
    assert_eq!(notification.method, "notifications/initialized");
    assert!(msg.id().is_none()); // Notifications have no ID
}

/// Test multiple messages in sequence
#[tokio::test]
async fn test_message_sequence() {
    let (client, server) = MemoryTransport::pair();

    // Send multiple requests
    for i in 1..=5 {
        let request = Request::new("ping", i as u64);
        client.send(Message::Request(request)).await.unwrap();
    }

    // Receive and respond to each
    for i in 1..=5 {
        let msg = server.recv().await.unwrap().unwrap();
        let request = msg.as_request().unwrap();
        assert_eq!(request.id, RequestId::Number(i));

        let response = Response::success(i as u64, json!({"pong": i}));
        server.send(Message::Response(response)).await.unwrap();
    }

    // Client receives all responses
    for i in 1..=5 {
        let msg = client.recv().await.unwrap().unwrap();
        let response = msg.as_response().unwrap();
        assert_eq!(response.id, RequestId::Number(i));
        assert_eq!(response.result.as_ref().unwrap()["pong"], i);
    }
}

/// Test transport disconnect
#[tokio::test]
async fn test_transport_disconnect() {
    let (client, server) = MemoryTransport::pair();

    assert!(client.is_connected());
    assert!(server.is_connected());

    // Close the client
    client.close().await.unwrap();

    assert!(!client.is_connected());
    // Note: In memory transport, closing one end closes both
    assert!(!server.is_connected());
}

/// Test error response
#[tokio::test]
async fn test_error_response() {
    let (client, server) = MemoryTransport::pair();

    // Client sends request for unknown method
    let request = Request::new("unknown/method", 1u64);
    client.send(Message::Request(request)).await.unwrap();

    // Server receives
    let _ = server.recv().await.unwrap().unwrap();

    // Server sends error response
    let error = mcpkit::error::JsonRpcError::method_not_found("Method unknown/method not found");
    let response = Response::error(1u64, error);
    server.send(Message::Response(response)).await.unwrap();

    // Client receives error
    let msg = client.recv().await.unwrap().unwrap();
    let response = msg.as_response().unwrap();
    assert!(response.is_error());
    assert_eq!(response.error.as_ref().unwrap().code, -32601);
}

/// Test tool call lifecycle over transport
#[tokio::test]
async fn test_tool_call_lifecycle() {
    let (client, server) = MemoryTransport::pair();

    // 1. Initialize
    let init_request = Request::with_params(
        "initialize",
        1u64,
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }),
    );
    client.send(Message::Request(init_request)).await.unwrap();
    let _ = server.recv().await.unwrap();

    let init_response = Response::success(
        1u64,
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "test-server", "version": "1.0"}
        }),
    );
    server.send(Message::Response(init_response)).await.unwrap();
    let _ = client.recv().await.unwrap();

    // 2. Send initialized notification
    let initialized = Notification::new("notifications/initialized");
    client
        .send(Message::Notification(initialized))
        .await
        .unwrap();
    let _ = server.recv().await.unwrap();

    // 3. List tools
    let list_tools = Request::new("tools/list", 2u64);
    client.send(Message::Request(list_tools)).await.unwrap();
    let _ = server.recv().await.unwrap();

    let tools_response = Response::success(
        2u64,
        json!({
            "tools": [{
                "name": "echo",
                "description": "Echo back the input",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": {"type": "string"}
                    }
                }
            }]
        }),
    );
    server.send(Message::Response(tools_response)).await.unwrap();

    let msg = client.recv().await.unwrap().unwrap();
    let response = msg.as_response().unwrap();
    let tools = response.result.as_ref().unwrap()["tools"]
        .as_array()
        .unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "echo");

    // 4. Call tool
    let call_tool = Request::with_params(
        "tools/call",
        3u64,
        json!({
            "name": "echo",
            "arguments": {"message": "Hello, MCP!"}
        }),
    );
    client.send(Message::Request(call_tool)).await.unwrap();

    let msg = server.recv().await.unwrap().unwrap();
    let request = msg.as_request().unwrap();
    assert_eq!(request.method, "tools/call");

    // Server processes and responds
    let call_response = Response::success(
        3u64,
        json!({
            "content": [{"type": "text", "text": "Hello, MCP!"}],
            "isError": false
        }),
    );
    server.send(Message::Response(call_response)).await.unwrap();

    let msg = client.recv().await.unwrap().unwrap();
    let response = msg.as_response().unwrap();
    assert!(response.is_success());
    let result = response.result.as_ref().unwrap();
    assert_eq!(result["content"][0]["text"], "Hello, MCP!");
}

/// Test JSON-RPC wire format serialization
#[tokio::test]
async fn test_wire_format() {
    // Verify messages serialize to correct JSON-RPC format
    let request = Request::new("test/method", 1u64);
    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "test/method");
    assert_eq!(json["id"], 1);

    // Response
    let response = Response::success(1u64, json!({"result": "value"}));
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none());

    // Notification
    let notification = Notification::new("notifications/test");
    let json = serde_json::to_value(&notification).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["method"], "notifications/test");
    assert!(json.get("id").is_none());
}

/// Test message parsing from JSON
#[tokio::test]
async fn test_wire_format_parsing() {
    // Parse request
    let request_json = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "test/method",
        "params": {"key": "value"}
    });
    let msg: Message = serde_json::from_value(request_json).unwrap();
    assert!(msg.is_request());
    assert_eq!(msg.method(), Some("test/method"));
    assert_eq!(msg.id(), Some(&RequestId::Number(42)));

    // Parse response
    let response_json = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "result": {"data": "test"}
    });
    let msg: Message = serde_json::from_value(response_json).unwrap();
    assert!(msg.is_response());

    // Parse notification
    let notification_json = json!({
        "jsonrpc": "2.0",
        "method": "notifications/test"
    });
    let msg: Message = serde_json::from_value(notification_json).unwrap();
    assert!(msg.is_notification());
    assert!(msg.id().is_none());
}
