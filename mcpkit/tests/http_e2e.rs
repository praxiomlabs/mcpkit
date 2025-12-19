//! End-to-end HTTP transport tests.
//!
//! These tests verify that MCP communication works correctly over HTTP.
//! They require the `http` feature flag to be enabled.

#![cfg(feature = "http")]

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use mcpkit::protocol::{Message, Request, RequestId, Response};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::timeout;

/// MCP Protocol version header (lowercase for HTTP/2 compatibility).
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";
const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Simple server state
#[derive(Clone, Default)]
struct TestServerState {
    request_count: Arc<RwLock<u64>>,
}

/// Helper to find an available port
async fn get_available_addr() -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    Ok(listener.local_addr()?)
}

/// Handle MCP POST requests
async fn handle_mcp_request(
    State(state): State<TestServerState>,
    headers: HeaderMap,
    body: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // Check protocol version header
    let _version = headers
        .get(MCP_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(MCP_PROTOCOL_VERSION);

    // Parse the JSON-RPC message
    let msg: Message = match serde_json::from_str(&body) {
        Ok(m) => m,
        Err(e) => {
            let error_response = json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {}", e)
                }
            });
            return (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                error_response.to_string(),
            )
                .into_response();
        }
    };

    // Update request count
    {
        let mut count = state.request_count.write().await;
        *count += 1;
    }

    // Handle the request
    let response = match msg {
        Message::Request(req) => {
            let resp = match req.method.as_ref() {
                "initialize" => Response::success(
                    req.id,
                    json!({
                        "protocolVersion": MCP_PROTOCOL_VERSION,
                        "serverInfo": {
                            "name": "test-http-server",
                            "version": "1.0.0"
                        },
                        "capabilities": {
                            "tools": {},
                            "resources": {}
                        }
                    }),
                ),
                "tools/list" => Response::success(req.id, json!({ "tools": [] })),
                "resources/list" => Response::success(req.id, json!({ "resources": [] })),
                "ping" => Response::success(req.id, json!({})),
                _ => Response::error(
                    req.id,
                    mcpkit::error::JsonRpcError::method_not_found(req.method.to_string()),
                ),
            };
            Message::Response(resp)
        }
        Message::Notification(_) => {
            // Notifications don't get a response
            return (
                StatusCode::ACCEPTED,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                String::new(),
            )
                .into_response();
        }
        Message::Response(_) => {
            // Servers don't receive responses
            return (
                StatusCode::BAD_REQUEST,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                json!({"error": "unexpected response"}).to_string(),
            )
                .into_response();
        }
    };

    let body = serde_json::to_string(&response).unwrap();

    // Build response with headers including session ID
    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert(
        axum::http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    resp_headers.insert(
        axum::http::header::HeaderName::from_static("mcp-session-id"),
        "test-session-id".parse().unwrap(),
    );

    (StatusCode::OK, resp_headers, body).into_response()
}

/// Spawn a test HTTP server
async fn spawn_test_server(addr: SocketAddr) -> tokio::task::JoinHandle<()> {
    let state = TestServerState::default();
    let app = Router::new()
        .route("/mcp", post(handle_mcp_request))
        .with_state(state);

    tokio::spawn(async move {
        let listener = TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    })
}

#[tokio::test]
async fn test_http_basic_request() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create HTTP client
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send ping request
    let request = Request::new("ping", 1u64);
    let body = serde_json::to_string(&Message::Request(request))?;

    let result = timeout(
        Duration::from_secs(5),
        client
            .post(&url)
            .header("Content-Type", "application/json")
            .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
            .body(body)
            .send(),
    )
    .await;

    assert!(result.is_ok());
    let response = result??;
    assert_eq!(response.status(), 200);

    let body = response.text().await?;
    let msg: Message = serde_json::from_str(&body)?;
    assert!(msg.is_response());
    assert!(msg.as_response().ok_or("Expected response")?.is_success());
    Ok(())
}

#[tokio::test]
async fn test_http_initialize_handshake() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send initialize request
    let init_request = Request::with_params(
        "initialize",
        1u64,
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }),
    );
    let body = serde_json::to_string(&Message::Request(init_request))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body(body)
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Check for session ID header
    let session_id = response.headers().get(MCP_SESSION_ID_HEADER);
    assert!(session_id.is_some());

    let body = response.text().await?;
    let msg: Message = serde_json::from_str(&body)?;
    let resp = msg.as_response().ok_or("Expected response")?;
    assert!(resp.is_success());
    assert_eq!(resp.id, RequestId::Number(1));
    assert_eq!(
        resp.result.as_ref().ok_or("Expected result")?["protocolVersion"],
        MCP_PROTOCOL_VERSION
    );
    Ok(())
}

#[tokio::test]
async fn test_http_tools_list() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    let request = Request::new("tools/list", 1u64);
    let body = serde_json::to_string(&Message::Request(request))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body(body)
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body = response.text().await?;
    let msg: Message = serde_json::from_str(&body)?;
    let resp = msg.as_response().ok_or("Expected response")?;
    assert!(resp.is_success());
    let tools = resp.result.as_ref().ok_or("Expected result")?["tools"]
        .as_array()
        .ok_or("Expected array")?;
    assert!(tools.is_empty()); // Test server returns empty list
    Ok(())
}

#[tokio::test]
async fn test_http_method_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    let request = Request::new("unknown/method", 1u64);
    let body = serde_json::to_string(&Message::Request(request))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body(body)
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body = response.text().await?;
    let msg: Message = serde_json::from_str(&body)?;
    let resp = msg.as_response().ok_or("Expected response")?;
    assert!(resp.is_error());
    assert_eq!(resp.error.as_ref().ok_or("Expected value")?.code, -32601);
    Ok(())
}

#[tokio::test]
async fn test_http_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send invalid JSON
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body("not valid json")
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    let body = response.text().await?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    assert_eq!(json["error"]["code"], -32700);
    Ok(())
}

#[tokio::test]
async fn test_http_multiple_requests() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send multiple sequential requests
    for i in 1..=5 {
        let request = Request::new("ping", i);
        let body = serde_json::to_string(&Message::Request(request))?;

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
            .body(body)
            .send()
            .await?;

        assert_eq!(response.status(), 200);

        let body = response.text().await?;
        let msg: Message = serde_json::from_str(&body)?;
        let resp = msg.as_response().ok_or("Expected response")?;
        assert!(resp.is_success());
        assert_eq!(resp.id, RequestId::Number(i));
    }
    Ok(())
}

#[tokio::test]
async fn test_http_notification() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send a notification (no response expected)
    let notification = mcpkit::protocol::Notification::new("notifications/initialized");
    let body = serde_json::to_string(&Message::Notification(notification))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body(body)
        .send()
        .await?;

    // Notifications get 202 Accepted with empty body
    assert_eq!(response.status(), 202);
    Ok(())
}

#[tokio::test]
async fn test_http_session_id_persistence() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // First request - get session ID
    let request = Request::new("ping", 1u64);
    let body = serde_json::to_string(&Message::Request(request))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .body(body)
        .send()
        .await?;

    assert_eq!(response.status(), 200);
    let session_id = response
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .ok_or("Expected header")?
        .to_str()?
        .to_string();

    // Second request - include session ID
    let request = Request::new("ping", 2u64);
    let body = serde_json::to_string(&Message::Request(request))?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
        .header(MCP_SESSION_ID_HEADER, &session_id)
        .body(body)
        .send()
        .await?;

    assert_eq!(response.status(), 200);

    // Session ID should be preserved in response
    let response_session_id = response
        .headers()
        .get(MCP_SESSION_ID_HEADER)
        .ok_or("Expected header")?
        .to_str()?;
    assert_eq!(session_id, response_session_id);
    Ok(())
}

#[tokio::test]
async fn test_http_concurrent_requests() -> Result<(), Box<dyn std::error::Error>> {
    let addr = get_available_addr().await?;
    let _server = spawn_test_server(addr).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let url = format!("http://{addr}/mcp");

    // Send multiple concurrent requests
    let mut handles = Vec::new();
    for i in 1..=10 {
        let client = client.clone();
        let url = url.clone();
        handles.push(tokio::spawn(async move {
            let request = Request::new("ping", i);
            let body = serde_json::to_string(&Message::Request(request)).unwrap();

            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .header(MCP_PROTOCOL_VERSION_HEADER, MCP_PROTOCOL_VERSION)
                .body(body)
                .send()
                .await
                .unwrap();

            assert_eq!(response.status(), 200);
            let body = response.text().await.unwrap();
            let msg: Message = serde_json::from_str(&body).unwrap();
            let resp = msg.as_response().unwrap();
            assert!(resp.is_success());
            i
        }));
    }

    // Wait for all requests to complete
    let mut completed = Vec::new();
    for handle in handles {
        completed.push(handle.await?);
    }

    assert_eq!(completed.len(), 10);
    Ok(())
}
