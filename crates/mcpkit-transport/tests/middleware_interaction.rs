//! Middleware interaction tests.
//!
//! Tests verifying correct behavior when multiple middleware layers
//! are composed together.

use mcpkit_transport::memory::MemoryTransport;
use mcpkit_transport::middleware::{
    IdentityLayer, LayerStack, LoggingLayer, MetricsLayer, TimeoutLayer,
};
use mcpkit_transport::traits::Transport;
use mcpkit_core::protocol::{Message, Notification};
use std::sync::Arc;
use std::time::Duration;
use tracing::Level;

// =============================================================================
// Layer Composition Tests
// =============================================================================

#[test]
fn test_identity_layer_composition() {
    let (client, _server) = MemoryTransport::pair();

    // Multiple identity layers should have no effect
    let stack = LayerStack::new(client)
        .with(IdentityLayer)
        .with(IdentityLayer)
        .with(IdentityLayer);

    let transport = stack.into_inner();
    assert!(transport.is_connected());
}

#[tokio::test]
async fn test_timeout_layer_applies() {
    let (client, _server) = MemoryTransport::pair();

    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)));

    let transport = stack.into_inner();
    assert!(transport.is_connected());

    // Verify timeout is configured
    assert_eq!(transport.send_timeout(), Some(Duration::from_secs(30)));
    assert_eq!(transport.recv_timeout(), Some(Duration::from_secs(30)));
}

#[tokio::test]
async fn test_logging_layer_applies() {
    let (client, _server) = MemoryTransport::pair();

    let stack = LayerStack::new(client)
        .with(LoggingLayer::new(Level::DEBUG));

    let transport = stack.into_inner();
    assert!(transport.is_connected());
}

#[tokio::test]
async fn test_metrics_layer_applies() {
    let (client, _server) = MemoryTransport::pair();

    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client).with(metrics_layer);

    let transport = stack.into_inner();
    assert!(transport.is_connected());

    // Verify initial stats are zero
    assert_eq!(handle.messages_sent(), 0);
    assert_eq!(handle.messages_received(), 0);
}

// =============================================================================
// Layer Order Tests
// =============================================================================

#[tokio::test]
async fn test_layer_order_preserved() {
    let (client, server) = MemoryTransport::pair();

    // Stack: metrics -> timeout -> transport
    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(10)))
        .with(metrics_layer);

    let transport = stack.into_inner();

    // Send a message
    let msg = Message::Notification(Notification::new("test"));
    transport.send(msg).await.unwrap();

    // Receive on server side
    let _ = server.recv().await.unwrap();

    // Metrics should have recorded the send
    assert_eq!(handle.messages_sent(), 1);
}

// =============================================================================
// Timeout Behavior Tests
// =============================================================================

#[tokio::test]
async fn test_timeout_on_slow_receive() {
    let (_client, server) = MemoryTransport::pair();

    // Very short timeout
    let stack = LayerStack::new(server)
        .with(TimeoutLayer::new(Duration::from_millis(10)));

    let transport = stack.into_inner();

    // No message sent, so recv should timeout
    let result = transport.recv().await;

    // Should get a timeout error
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    // The error message contains "timed out" which is good
    assert!(
        err_msg.to_lowercase().contains("timed out") ||
        err_msg.to_lowercase().contains("timeout") ||
        err_msg.to_lowercase().contains("elapsed"),
        "Error should indicate timeout: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_send_without_timeout_succeeds() {
    let (client, server) = MemoryTransport::pair();

    // No timeout layer
    let stack = LayerStack::new(client).with(IdentityLayer);
    let transport = stack.into_inner();

    let msg = Message::Notification(Notification::new("test"));
    let result = transport.send(msg).await;
    assert!(result.is_ok());

    // Verify message was received
    let received = server.recv().await.unwrap();
    assert!(received.is_some());
}

// =============================================================================
// Metrics Accuracy Tests
// =============================================================================

#[tokio::test]
async fn test_metrics_track_sends_and_receives() {
    let (client, server) = MemoryTransport::pair_with_capacity(100);

    let (client_metrics_layer, client_handle) = MetricsLayer::new_with_handle();
    let (server_metrics_layer, server_handle) = MetricsLayer::new_with_handle();

    let client_stack = LayerStack::new(client).with(client_metrics_layer);
    let server_stack = LayerStack::new(server).with(server_metrics_layer);

    let client_transport = client_stack.into_inner();
    let server_transport = server_stack.into_inner();

    // Send multiple messages
    for i in 0..5 {
        let msg = Message::Notification(Notification::with_params(
            "test",
            serde_json::json!({"seq": i}),
        ));
        client_transport.send(msg).await.unwrap();
    }

    // Client should show 5 sent
    assert_eq!(client_handle.messages_sent(), 5);
    assert_eq!(client_handle.messages_received(), 0);

    // Receive all on server
    for _ in 0..5 {
        let _ = server_transport.recv().await.unwrap();
    }

    // Server should show 5 received
    assert_eq!(server_handle.messages_received(), 5);
    assert_eq!(server_handle.messages_sent(), 0);
}

#[tokio::test]
async fn test_metrics_track_errors() {
    let (client, server) = MemoryTransport::pair();

    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client).with(metrics_layer);
    let transport = stack.into_inner();

    // Close server to cause send errors
    server.close().await.unwrap();

    // Attempt to send (may fail)
    let msg = Message::Notification(Notification::new("test"));
    let _ = transport.send(msg).await;

    // Check stats - either message went through or there was an error
    let sent = handle.messages_sent();
    let errors = handle.send_errors();
    assert!(sent > 0 || errors > 0, "Should have recorded either send or error");
}

// =============================================================================
// Combined Middleware Tests
// =============================================================================

#[tokio::test]
async fn test_full_middleware_stack() {
    let (client, server) = MemoryTransport::pair_with_capacity(100);

    // Full stack: logging -> metrics -> timeout -> transport
    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(metrics_layer)
        .with(LoggingLayer::new(Level::DEBUG));

    let transport = stack.into_inner();
    assert!(transport.is_connected());

    // Send messages through the full stack
    for i in 0..3 {
        let msg = Message::Notification(Notification::with_params(
            "full_stack_test",
            serde_json::json!({"iteration": i}),
        ));
        transport.send(msg).await.unwrap();
    }

    // Receive on server
    for _ in 0..3 {
        let result = server.recv().await.unwrap();
        assert!(result.is_some());
    }

    // Verify metrics through handle
    assert_eq!(handle.messages_sent(), 3);
}

// =============================================================================
// Close Propagation Tests
// =============================================================================

#[tokio::test]
async fn test_close_propagates_through_layers() {
    let (client, _server) = MemoryTransport::pair();

    let (metrics_layer, _handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(metrics_layer);

    let transport = stack.into_inner();
    assert!(transport.is_connected());

    // Close through the stack
    transport.close().await.unwrap();
    assert!(!transport.is_connected());
}

#[tokio::test]
async fn test_double_close_through_layers() {
    let (client, _server) = MemoryTransport::pair();

    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)));

    let transport = stack.into_inner();

    // Close multiple times should be safe
    transport.close().await.unwrap();
    transport.close().await.unwrap();
    transport.close().await.unwrap();
}

// =============================================================================
// Metadata Propagation Tests
// =============================================================================

#[tokio::test]
async fn test_metadata_propagates_through_layers() {
    let (client, _server) = MemoryTransport::pair();

    let (metrics_layer, _handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(metrics_layer)
        .with(LoggingLayer::new(Level::DEBUG));

    let transport = stack.into_inner();

    // Metadata should be accessible
    let metadata = transport.metadata();
    assert_eq!(metadata.transport_type, "memory");
}

// =============================================================================
// Concurrent Access Through Layers Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_sends_through_layers() {
    let (client, server) = MemoryTransport::pair_with_capacity(200);

    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let client = Arc::new(
        LayerStack::new(client)
            .with(metrics_layer)
            .into_inner(),
    );
    let server = Arc::new(server);

    // Spawn receiver
    let server_clone = server.clone();
    let receiver = tokio::spawn(async move {
        let mut count = 0;
        loop {
            match tokio::time::timeout(Duration::from_secs(2), server_clone.recv()).await {
                Ok(Ok(Some(_))) => count += 1,
                _ => break,
            }
        }
        count
    });

    // Spawn multiple senders
    let mut handles_vec = vec![];
    for _ in 0..5 {
        let client_clone = client.clone();
        handles_vec.push(tokio::spawn(async move {
            for j in 0..10 {
                let msg = Message::Notification(Notification::with_params(
                    "concurrent",
                    serde_json::json!({"seq": j}),
                ));
                let _ = client_clone.send(msg).await;
            }
        }));
    }

    // Wait for senders
    for h in handles_vec {
        h.await.unwrap();
    }

    // Give receiver time
    tokio::time::sleep(Duration::from_millis(500)).await;
    client.close().await.unwrap();

    let received = receiver.await.unwrap();
    assert!(received > 0, "Should have received some messages");

    // Check metrics
    assert_eq!(handle.messages_sent(), 50); // 5 tasks * 10 messages
}

// =============================================================================
// Error Propagation Tests
// =============================================================================

#[tokio::test]
async fn test_error_propagates_through_layers() {
    let (client, server) = MemoryTransport::pair();

    let (metrics_layer, _handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client)
        .with(TimeoutLayer::new(Duration::from_secs(30)))
        .with(metrics_layer);

    let transport = stack.into_inner();

    // Close server to cause errors
    server.close().await.unwrap();

    // Send should fail
    let msg = Message::Notification(Notification::new("test"));
    let result = transport.send(msg).await;

    // Error should propagate up
    assert!(result.is_err());
}

// =============================================================================
// Layer Stack Destruction Tests
// =============================================================================

#[test]
fn test_layer_stack_into_inner() {
    let (client, _server) = MemoryTransport::pair();

    let stack = LayerStack::new(client);
    let transport = stack.into_inner();

    assert!(transport.is_connected());
}

#[test]
fn test_layer_stack_inner_reference() {
    let (client, _server) = MemoryTransport::pair();

    let stack = LayerStack::new(client).with(IdentityLayer);

    // Should be able to get reference without consuming
    let inner_ref = stack.inner();
    assert!(inner_ref.is_connected());

    // Can still use the stack
    let transport = stack.into_inner();
    assert!(transport.is_connected());
}

// =============================================================================
// Metrics Bytes Tracking Tests
// =============================================================================

#[tokio::test]
async fn test_metrics_track_bytes() {
    let (client, server) = MemoryTransport::pair();

    let (metrics_layer, handle) = MetricsLayer::new_with_handle();
    let stack = LayerStack::new(client).with(metrics_layer);
    let transport = stack.into_inner();

    // Send a message
    let msg = Message::Notification(Notification::with_params(
        "test",
        serde_json::json!({"data": "some content"}),
    ));
    transport.send(msg).await.unwrap();

    // Bytes sent should be > 0
    assert!(handle.bytes_sent() > 0, "Should have tracked bytes sent");

    // Receive on server
    let _ = server.recv().await.unwrap();
}

// =============================================================================
// Default Timeout Tests
// =============================================================================

#[test]
fn test_timeout_layer_default_values() {
    let layer = TimeoutLayer::default();

    // Default should have reasonable timeouts
    // Just verify the layer can be created with defaults
    let _ = layer;
}

#[test]
fn test_timeout_layer_custom_send_recv() {
    let layer = TimeoutLayer::default()
        .send_timeout(Duration::from_secs(5))
        .recv_timeout(Duration::from_secs(120));

    let (client, _server) = MemoryTransport::pair();
    let stack = LayerStack::new(client).with(layer);
    let transport = stack.into_inner();

    assert_eq!(transport.send_timeout(), Some(Duration::from_secs(5)));
    assert_eq!(transport.recv_timeout(), Some(Duration::from_secs(120)));
}

#[test]
fn test_timeout_layer_disable_timeouts() {
    let layer = TimeoutLayer::default()
        .no_send_timeout()
        .no_recv_timeout();

    let (client, _server) = MemoryTransport::pair();
    let stack = LayerStack::new(client).with(layer);
    let transport = stack.into_inner();

    assert!(transport.send_timeout().is_none());
    assert!(transport.recv_timeout().is_none());
}
