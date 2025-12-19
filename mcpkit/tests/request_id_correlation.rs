//! Test to verify request ID correlation in the client.
//!
//! This test simulates the exact flow of request/response correlation
//! and prints diagnostic information to identify the race condition.

use mcpkit::protocol::{Message, RequestId, Response};
use mcpkit_transport::{Transport, TransportError, TransportMetadata};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A transport that logs all operations and allows inspection
struct DiagnosticTransport {
    /// Queue for client->server messages
    outbound: Arc<Mutex<VecDeque<Message>>>,
    /// Queue for server->client messages
    inbound: Arc<Mutex<VecDeque<Message>>>,
    name: &'static str,
}

impl DiagnosticTransport {
    fn pair() -> (Self, Self) {
        let c2s = Arc::new(Mutex::new(VecDeque::new()));
        let s2c = Arc::new(Mutex::new(VecDeque::new()));

        let client = Self {
            outbound: Arc::clone(&c2s),
            inbound: Arc::clone(&s2c),
            name: "client",
        };

        let server = Self {
            outbound: Arc::clone(&s2c),
            inbound: Arc::clone(&c2s),
            name: "server",
        };

        (client, server)
    }
}

impl Transport for DiagnosticTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let id = match &msg {
            Message::Request(r) => format!("Request(id={:?}, method={})", r.id, r.method),
            Message::Response(r) => format!("Response(id={:?})", r.id),
            Message::Notification(n) => format!("Notification(method={})", n.method),
        };
        println!("[{}] SEND: {}", self.name, id);
        self.outbound.lock().await.push_back(msg);
        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        loop {
            if let Some(msg) = self.inbound.lock().await.pop_front() {
                let id = match &msg {
                    Message::Request(r) => format!("Request(id={:?}, method={})", r.id, r.method),
                    Message::Response(r) => format!("Response(id={:?})", r.id),
                    Message::Notification(n) => format!("Notification(method={})", n.method),
                };
                println!("[{}] RECV: {}", self.name, id);
                return Ok(Some(msg));
            }
            tokio::time::sleep(tokio::time::Duration::from_micros(100)).await;
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        println!("[{}] CLOSE", self.name);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::default()
    }
}

#[tokio::test]
async fn test_request_id_correlation_with_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    use mcpkit_client::ClientBuilder;
    use serde_json::json;

    let (client_transport, server_transport) = DiagnosticTransport::pair();
    let server_transport = Arc::new(server_transport);
    let server_clone = Arc::clone(&server_transport);

    // Spawn fake server
    let server_handle = tokio::spawn(async move {
        println!("\n=== SERVER: Waiting for initialize ===");

        // Handle initialize
        let msg = server_clone.recv().await?.ok_or("No message received")?;
        let req = msg.as_request().ok_or("Expected request")?;
        println!("SERVER: Got initialize request id={:?}", req.id);

        let resp = Response::success(
            req.id.clone(),
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "test", "version": "1.0"}
            }),
        );
        println!("SERVER: Sending initialize response id={:?}", resp.id);
        server_clone.send(Message::Response(resp)).await?;

        // Handle initialized notification
        let msg = server_clone.recv().await?.ok_or("No message received")?;
        println!(
            "SERVER: Got notification: {:?}",
            msg.as_notification().map(|n| &n.method)
        );

        // Handle tools/list
        let msg = server_clone.recv().await?.ok_or("No message received")?;
        let req = msg.as_request().ok_or("Expected request")?;
        println!("SERVER: Got tools/list request id={:?}", req.id);

        let resp = Response::success(req.id.clone(), json!({"tools": []}));
        println!("SERVER: Sending tools/list response id={:?}", resp.id);
        server_clone.send(Message::Response(resp)).await?;

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    });

    println!("\n=== CLIENT: Building client ===");

    // Build client
    let client = ClientBuilder::new()
        .name("diagnostic-client")
        .version("1.0")
        .build(client_transport)
        .await?;

    println!("\n=== CLIENT: Calling list_tools ===");

    // Make a request
    let tools = client.list_tools().await?;
    println!("\n=== CLIENT: Got tools: {tools:?} ===");

    match server_handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string().into()),
        Err(e) => Err(format!("Join error: {e}").into()),
    }
}

/// Test that `RequestId` equality works correctly after JSON serialization
#[test]
fn test_request_id_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::{from_str, to_string};

    // Test numeric ID
    let original = RequestId::Number(42);
    let json = to_string(&original)?;
    println!("Number ID JSON: {json}");
    let roundtrip: RequestId = from_str(&json)?;
    assert_eq!(original, roundtrip, "Number ID should roundtrip");

    // Test string ID
    let original = RequestId::String("req-001".to_string());
    let json = to_string(&original)?;
    println!("String ID JSON: {json}");
    let roundtrip: RequestId = from_str(&json)?;
    assert_eq!(original, roundtrip, "String ID should roundtrip");

    // Test within Response
    let response = Response::success(RequestId::Number(5), serde_json::json!({"foo": "bar"}));
    let json = to_string(&response)?;
    println!("Response JSON: {json}");
    let roundtrip: Response = from_str(&json)?;
    assert_eq!(response.id, roundtrip.id, "Response ID should roundtrip");
    Ok(())
}

/// Test that `HashMap` lookup works with `RequestId`
#[test]
fn test_request_id_hashmap_lookup() -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;

    let mut map: HashMap<RequestId, &str> = HashMap::new();

    // Insert with Number
    map.insert(RequestId::Number(1), "first");
    map.insert(RequestId::Number(2), "second");

    // Lookup should work
    assert_eq!(map.get(&RequestId::Number(1)), Some(&"first"));
    assert_eq!(map.get(&RequestId::Number(2)), Some(&"second"));
    assert_eq!(map.get(&RequestId::Number(3)), None);

    // After JSON roundtrip
    let key = RequestId::Number(1);
    let json = serde_json::to_string(&key)?;
    let roundtrip: RequestId = serde_json::from_str(&json)?;
    assert_eq!(
        map.get(&roundtrip),
        Some(&"first"),
        "Lookup after roundtrip should work"
    );
    Ok(())
}
