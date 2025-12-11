//! End-to-end WebSocket transport tests.
//!
//! These tests verify that MCP communication works correctly over WebSocket.
//! They require the `websocket` feature flag to be enabled.

#![cfg(feature = "websocket")]

use futures::{SinkExt, StreamExt};
use mcpkit::protocol::{Message, Request, RequestId, Response};
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message as WsMessage};

/// Helper to find an available port
async fn get_available_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap()
}

/// Helper to send a text message over WebSocket
fn ws_text(s: String) -> WsMessage {
    WsMessage::Text(s)
}

/// Helper to extract text from a WebSocket message
fn extract_text(msg: WsMessage) -> Option<String> {
    match msg {
        WsMessage::Text(text) => Some(text),
        _ => None,
    }
}

/// Simple WebSocket server that echoes JSON-RPC responses
async fn spawn_test_server(listener: TcpListener) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Ok((stream, _addr)) = listener.accept().await {
            if let Ok(ws_stream) = accept_async(stream).await {
                let (mut tx, mut rx) = ws_stream.split();

                while let Some(Ok(msg)) = rx.next().await {
                    if let Some(text) = extract_text(msg.clone()) {
                        // Parse as MCP message
                        if let Ok(mcp_msg) = serde_json::from_str::<Message>(&text) {
                            let response = match mcp_msg {
                                Message::Request(req) => {
                                    let resp = match req.method.as_ref() {
                                        "initialize" => Response::success(
                                            req.id,
                                            json!({
                                                "protocolVersion": "2025-11-25",
                                                "serverInfo": {
                                                    "name": "test-ws-server",
                                                    "version": "1.0.0"
                                                },
                                                "capabilities": {}
                                            }),
                                        ),
                                        "tools/list" => Response::success(
                                            req.id,
                                            json!({ "tools": [] }),
                                        ),
                                        "ping" => Response::success(req.id, json!({})),
                                        _ => Response::error(
                                            req.id,
                                            mcpkit::error::JsonRpcError::method_not_found(
                                                req.method.to_string(),
                                            ),
                                        ),
                                    };
                                    Some(Message::Response(resp))
                                }
                                Message::Notification(_) => None,
                                Message::Response(_) => None,
                            };

                            if let Some(resp) = response {
                                let json = serde_json::to_string(&resp).unwrap();
                                if tx.send(ws_text(json)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    } else if let WsMessage::Close(_) = msg {
                        break;
                    }
                }
            }
        }
    })
}

#[tokio::test]
async fn test_websocket_connect() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect as client
    let url = format!("ws://{}", addr);
    let result = timeout(Duration::from_secs(5), connect_async(&url)).await;

    assert!(result.is_ok(), "WebSocket connection should succeed");
    let (ws_stream, _) = result.unwrap().unwrap();
    // Stream is connected - just verify the stream exists
    let _ = ws_stream;
}

#[tokio::test]
async fn test_websocket_initialize_handshake() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send initialize request
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
    let json = serde_json::to_string(&Message::Request(init_request)).unwrap();
    tx.send(ws_text(json)).await.unwrap();

    // Receive response
    let response = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream ended")
        .expect("WebSocket error");

    let text = extract_text(response).expect("Expected text message");
    let msg: Message = serde_json::from_str(&text).unwrap();
    assert!(msg.is_response());
    let resp = msg.as_response().unwrap();
    assert!(resp.is_success());
    assert_eq!(resp.id, RequestId::Number(1));
    assert_eq!(
        resp.result.as_ref().unwrap()["protocolVersion"],
        "2025-11-25"
    );
}

#[tokio::test]
async fn test_websocket_request_response_cycle() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send multiple requests
    for i in 1..=3 {
        let request = Request::new("ping", i as u64);
        let json = serde_json::to_string(&Message::Request(request)).unwrap();
        tx.send(ws_text(json)).await.unwrap();
    }

    // Receive all responses
    for i in 1..=3 {
        let response = timeout(Duration::from_secs(5), rx.next())
            .await
            .expect("Timeout")
            .expect("Stream ended")
            .expect("WebSocket error");

        let text = extract_text(response).expect("Expected text");
        let msg: Message = serde_json::from_str(&text).unwrap();
        let resp = msg.as_response().unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.id, RequestId::Number(i));
    }
}

#[tokio::test]
async fn test_websocket_tools_list() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send tools/list request
    let request = Request::new("tools/list", 1u64);
    let json = serde_json::to_string(&Message::Request(request)).unwrap();
    tx.send(ws_text(json)).await.unwrap();

    // Receive response
    let response = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("Timeout")
        .expect("Stream ended")
        .expect("WebSocket error");

    let text = extract_text(response).expect("Expected text");
    let msg: Message = serde_json::from_str(&text).unwrap();
    let resp = msg.as_response().unwrap();
    assert!(resp.is_success());
    let tools = resp.result.as_ref().unwrap()["tools"].as_array().unwrap();
    assert!(tools.is_empty()); // Test server returns empty tools list
}

#[tokio::test]
async fn test_websocket_method_not_found() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send request for unknown method
    let request = Request::new("unknown/method", 1u64);
    let json = serde_json::to_string(&Message::Request(request)).unwrap();
    tx.send(ws_text(json)).await.unwrap();

    // Receive error response
    let response = timeout(Duration::from_secs(5), rx.next())
        .await
        .expect("Timeout")
        .expect("Stream ended")
        .expect("WebSocket error");

    let text = extract_text(response).expect("Expected text");
    let msg: Message = serde_json::from_str(&text).unwrap();
    let resp = msg.as_response().unwrap();
    assert!(resp.is_error());
    assert_eq!(resp.error.as_ref().unwrap().code, -32601);
}

#[tokio::test]
async fn test_websocket_bidirectional() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send and receive interleaved
    let request1 = Request::new("ping", 1u64);
    tx.send(ws_text(serde_json::to_string(&Message::Request(request1)).unwrap()))
        .await
        .unwrap();

    let resp1 = timeout(Duration::from_secs(5), rx.next()).await;
    assert!(resp1.is_ok());

    let request2 = Request::new("tools/list", 2u64);
    tx.send(ws_text(serde_json::to_string(&Message::Request(request2)).unwrap()))
        .await
        .unwrap();

    let resp2 = timeout(Duration::from_secs(5), rx.next()).await;
    assert!(resp2.is_ok());
}

#[tokio::test]
async fn test_websocket_graceful_close() {
    let addr = get_available_addr().await;
    let listener = TcpListener::bind(addr).await.unwrap();
    let _server = spawn_test_server(listener).await;

    // Connect
    let url = format!("ws://{}", addr);
    let (ws_stream, _) = connect_async(&url).await.unwrap();
    let (mut tx, _rx) = ws_stream.split();

    // Send close frame
    let result = tx.send(WsMessage::Close(None)).await;
    assert!(result.is_ok());
}
