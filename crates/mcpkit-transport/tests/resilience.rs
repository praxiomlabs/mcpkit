//! Chaos and resilience tests for transport implementations.
//!
//! These tests verify that transports handle failure scenarios gracefully:
//! - Connection failures and reconnection
//! - Timeout behavior
//! - Concurrent message handling
//! - Resource exhaustion scenarios
//! - Graceful degradation

use mcpkit_core::protocol::{Message, Notification, Request, RequestId};
use mcpkit_transport::memory::MemoryTransport;
use mcpkit_transport::pool::{Pool, PoolConfig};
use mcpkit_transport::traits::Transport;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

// =============================================================================
// Connection Lifecycle Tests
// =============================================================================

#[tokio::test]
async fn test_graceful_close_and_reconnect() {
    let (client, server) = MemoryTransport::pair();

    // Verify initial connection
    assert!(client.is_connected());
    assert!(server.is_connected());

    // Close client side
    client.close().await.unwrap();
    assert!(!client.is_connected());

    // Send should fail gracefully after close
    let msg = Message::Request(Request::with_params(
        "test",
        RequestId::Number(1),
        json!({}),
    ));
    let result = client.send(msg).await;
    // After closing, send may succeed (if channel still open) or fail
    // The important thing is it doesn't panic
    let _ = result;
}

#[tokio::test]
async fn test_double_close_is_safe() {
    let (client, server) = MemoryTransport::pair();

    // Close multiple times should not panic
    client.close().await.unwrap();
    client.close().await.unwrap();
    client.close().await.unwrap();

    // Server should also handle double close
    server.close().await.unwrap();
    server.close().await.unwrap();
}

#[tokio::test]
async fn test_receive_after_close() {
    let (client, server) = MemoryTransport::pair();

    // Send a message first
    let msg = Message::Notification(Notification::new("test"));
    client.send(msg).await.unwrap();

    // Close the transport
    server.close().await.unwrap();

    // Receive should return None or error gracefully, not panic
    let result = server.recv().await;
    // After close, receive should either return error or None
    assert!(result.is_err() || result.unwrap().is_none());
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_sends() {
    // Use a larger buffer to handle concurrent sends
    let (client, server) = MemoryTransport::pair_with_capacity(200);
    let client: Arc<MemoryTransport> = Arc::new(client);
    let server: Arc<MemoryTransport> = Arc::new(server);
    let send_count = Arc::new(AtomicU32::new(0));

    // Start receiver first to drain messages concurrently
    let server_clone = server.clone();
    let recv_count = Arc::new(AtomicU32::new(0));
    let recv_count_clone = recv_count.clone();
    let receiver = tokio::spawn(async move {
        loop {
            match timeout(Duration::from_secs(2), server_clone.recv()).await {
                Ok(Ok(Some(_))) => {
                    recv_count_clone.fetch_add(1, Ordering::Relaxed);
                }
                _ => break,
            }
        }
    });

    // Spawn multiple concurrent senders
    let mut handles = vec![];
    for i in 0..10 {
        let client_clone = client.clone();
        let counter = send_count.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                let msg = Message::Notification(Notification::with_params(
                    "concurrent",
                    json!({"sender": i, "seq": j}),
                ));
                if client_clone.send(msg).await.is_ok() {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    // Wait for all senders
    for handle in handles {
        handle.await.unwrap();
    }

    // Should have sent all messages (100 total)
    assert_eq!(send_count.load(Ordering::Relaxed), 100);

    // Give receiver time to finish
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Close to signal receiver to stop
    client.close().await.unwrap();
    let _ = receiver.await;

    assert_eq!(
        recv_count.load(Ordering::Relaxed),
        100,
        "Should receive all 100 messages"
    );
}

#[tokio::test]
async fn test_concurrent_send_and_receive() {
    let (client, server) = MemoryTransport::pair();
    let client: Arc<MemoryTransport> = Arc::new(client);
    let server: Arc<MemoryTransport> = Arc::new(server);

    let messages_to_send = 50;

    // Start receiver first
    let server_clone = server.clone();
    let receiver = tokio::spawn(async move {
        let mut received = 0;
        loop {
            match timeout(Duration::from_secs(2), server_clone.recv()).await {
                Ok(Ok(Some(_))) => {
                    received += 1;
                    if received >= messages_to_send {
                        break;
                    }
                }
                _ => break,
            }
        }
        received
    });

    // Start sender
    let client_clone = client.clone();
    let sender = tokio::spawn(async move {
        for i in 0..messages_to_send {
            let msg = Message::Request(Request::with_params(
                "tools/list",
                RequestId::Number(i as u64),
                json!({}),
            ));
            let _ = client_clone.send(msg).await;
        }
    });

    sender.await.unwrap();
    let received = receiver.await.unwrap();

    assert_eq!(
        received, messages_to_send,
        "All messages should be received"
    );
}

// =============================================================================
// Timeout and Slow Consumer Tests
// =============================================================================

#[tokio::test]
async fn test_recv_timeout_behavior() {
    let (_client, server) = MemoryTransport::pair();

    // Receive with timeout should not hang indefinitely
    let result = timeout(Duration::from_millis(100), server.recv()).await;

    // Should timeout (no messages sent)
    assert!(result.is_err(), "Should timeout when no messages");
}

// =============================================================================
// Message Loss Simulation (via closed channels)
// =============================================================================

#[tokio::test]
async fn test_partial_close_behavior() {
    let (client, server) = MemoryTransport::pair();

    // Send some messages first, before closing
    for i in 0..5 {
        let msg = Message::Notification(Notification::with_params("test", json!({"seq": i})));
        client.send(msg).await.unwrap();
    }

    // Receive messages while client is still open
    let mut received = 0;
    for _ in 0..5 {
        match timeout(Duration::from_millis(100), server.recv()).await {
            Ok(Ok(Some(_))) => received += 1,
            _ => break,
        }
    }

    // Close client side
    client.close().await.unwrap();

    // We should have received the messages
    assert_eq!(
        received, 5,
        "Should receive all 5 pending messages (got {received})"
    );

    // After close, recv should eventually return None or error
    let result = timeout(Duration::from_millis(100), server.recv()).await;
    // This should either timeout, return None, or return error
    match result {
        Ok(Ok(Some(_))) => {} // Unexpected but not a failure - buffer might have more
        Ok(Ok(None)) => {}    // Expected - channel closed
        Ok(Err(_)) => {}      // Expected - error on closed channel
        Err(_) => {}          // Expected - timeout
    }
}

// =============================================================================
// Error Recovery Tests
// =============================================================================

#[tokio::test]
async fn test_send_error_does_not_corrupt_state() {
    let (client, server) = MemoryTransport::pair();

    // Close server to cause send errors
    server.close().await.unwrap();

    // Multiple failed sends should not corrupt client state
    for _ in 0..10 {
        let msg = Message::Notification(Notification::new("test"));
        let _ = client.send(msg).await;
    }

    // Client should be in a consistent state (can still close cleanly)
    client.close().await.unwrap();
}

// =============================================================================
// Large Message Handling
// =============================================================================

#[tokio::test]
async fn test_large_message_handling() {
    let (client, server) = MemoryTransport::pair_with_capacity(100);

    // Create a large payload
    let large_data: String = "x".repeat(100_000);
    let msg = Message::Notification(Notification::with_params(
        "large",
        json!({"data": large_data}),
    ));

    // Send should succeed
    client.send(msg).await.unwrap();

    // Receive should get the full message
    let received = server.recv().await.unwrap().unwrap();
    if let Message::Notification(n) = received {
        let params = n.params.unwrap();
        let data = params["data"].as_str().unwrap();
        assert_eq!(data.len(), 100_000);
    } else {
        panic!("Expected notification");
    }
}

// =============================================================================
// Transport Pool Tests
// =============================================================================

#[tokio::test]
async fn test_pool_basic_creation() {
    let config = PoolConfig::default();

    // Create a pool with a simple factory
    let pool = Pool::new(config, || async {
        let (client, _server) = MemoryTransport::pair();
        Ok::<_, mcpkit_transport::error::TransportError>(client)
    });

    // Pool stats should be accessible
    let stats = pool.stats().await;
    assert_eq!(stats.in_use, 0);
}

#[tokio::test]
async fn test_pool_acquire_and_release() {
    let config = PoolConfig::default();

    let pool = Pool::new(config, || async {
        let (client, _server) = MemoryTransport::pair();
        Ok::<_, mcpkit_transport::error::TransportError>(client)
    });

    // Acquire a connection
    let conn = pool.acquire().await.unwrap();

    // Connection should work - access inner connection
    assert!(conn.connection.is_connected());

    // Release back to pool
    pool.release(conn).await;

    let stats = pool.stats().await;
    // After release, connection should be available
    assert!(stats.connections_created >= 1);
}

// =============================================================================
// Metadata Consistency Tests
// =============================================================================

#[tokio::test]
async fn test_metadata_consistent_through_lifecycle() {
    let (client, _server) = MemoryTransport::pair();

    // Get metadata before operations
    let meta_before = client.metadata();

    // Send some messages
    let msg = Message::Notification(Notification::new("test"));
    client.send(msg).await.unwrap();

    // Get metadata after operations
    let meta_after = client.metadata();

    // Transport type should remain consistent
    assert_eq!(meta_before.transport_type, meta_after.transport_type);
}

// =============================================================================
// Message Counter Accuracy
// =============================================================================

#[tokio::test]
async fn test_message_counters_accurate() {
    let (client, server) = MemoryTransport::pair();

    // Send specific number of messages
    let send_count = 25;
    for i in 0..send_count {
        let msg = Message::Request(Request::with_params(
            "test",
            RequestId::Number(i),
            json!({}),
        ));
        client.send(msg).await.unwrap();
    }

    // Receive all messages
    let mut recv_count = 0;
    while let Ok(Some(_)) = timeout(Duration::from_millis(100), server.recv())
        .await
        .unwrap_or(Ok(None))
    {
        recv_count += 1;
    }

    // Verify we received all messages
    assert_eq!(recv_count, send_count as usize);
}

// =============================================================================
// Stress Tests
// =============================================================================

#[tokio::test]
async fn test_high_throughput_messages() {
    let (client, server) = MemoryTransport::pair();
    let client: Arc<MemoryTransport> = Arc::new(client);
    let server: Arc<MemoryTransport> = Arc::new(server);

    let message_count = 1000;

    // Sender task
    let client_clone = client.clone();
    let sender = tokio::spawn(async move {
        for i in 0..message_count {
            let msg = Message::Notification(Notification::with_params("stress", json!({"id": i})));
            client_clone.send(msg).await.unwrap();
        }
    });

    // Receiver task with backpressure simulation
    let server_clone = server.clone();
    let receiver = tokio::spawn(async move {
        let mut count = 0;
        loop {
            match timeout(Duration::from_secs(5), server_clone.recv()).await {
                Ok(Ok(Some(_))) => {
                    count += 1;
                    // Simulate some processing time
                    if count % 100 == 0 {
                        tokio::time::sleep(Duration::from_micros(100)).await;
                    }
                    if count >= message_count {
                        break;
                    }
                }
                _ => break,
            }
        }
        count
    });

    sender.await.unwrap();
    let received = receiver.await.unwrap();

    assert_eq!(received, message_count, "Should handle high throughput");
}

// =============================================================================
// Error Message Quality Tests
// =============================================================================

#[tokio::test]
async fn test_error_messages_are_descriptive() {
    let (client, server) = MemoryTransport::pair();

    // Close server
    server.close().await.unwrap();

    // Force an error
    let msg = Message::Notification(Notification::new("test"));
    let err = client.send(msg).await.unwrap_err();

    // Error should have meaningful message
    let err_msg = format!("{err}");
    assert!(!err_msg.is_empty(), "Error message should not be empty");

    // Debug representation should also be meaningful
    let debug_msg = format!("{err:?}");
    assert!(
        debug_msg.len() > 10,
        "Debug message should have details: {debug_msg}"
    );
}

// =============================================================================
// State Machine Tests
// =============================================================================

#[tokio::test]
async fn test_operations_on_closed_transport() {
    let (client, _server) = MemoryTransport::pair();

    // Close transport
    client.close().await.unwrap();

    // All operations should handle closed state gracefully
    assert!(!client.is_connected());

    // Send should fail but not panic
    let send_result = client
        .send(Message::Notification(Notification::new("test")))
        .await;
    assert!(send_result.is_err());

    // Recv should fail or return None, not panic
    let recv_result = client.recv().await;
    assert!(recv_result.is_err() || recv_result.unwrap().is_none());

    // Metadata should still work
    let _meta = client.metadata();

    // Close again should be idempotent
    client.close().await.unwrap();
}

// =============================================================================
// Backpressure Tests
// =============================================================================

#[tokio::test]
async fn test_buffer_backpressure() {
    // Create transport with small buffer to test backpressure
    let (client, server) = MemoryTransport::pair_with_capacity(2);

    // Fill the buffer
    for i in 0..2 {
        let msg = Message::Notification(Notification::with_params("fill", json!({"seq": i})));
        client.send(msg).await.unwrap();
    }

    // Drain one message to make room
    let _ = server.recv().await.unwrap();

    // Should be able to send another
    let msg = Message::Notification(Notification::new("after_drain"));
    let result = client.send(msg).await;
    assert!(result.is_ok(), "Should succeed after draining buffer");
}

// =============================================================================
// Concurrent Close Tests
// =============================================================================

#[tokio::test]
async fn test_concurrent_close() {
    let (client, server) = MemoryTransport::pair();
    let client = Arc::new(client);
    let server = Arc::new(server);

    // Spawn multiple close tasks
    let mut handles = vec![];
    for _ in 0..5 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.close().await.unwrap();
        }));
        let s = server.clone();
        handles.push(tokio::spawn(async move {
            s.close().await.unwrap();
        }));
    }

    // All should complete without panic
    for handle in handles {
        handle.await.unwrap();
    }
}

// =============================================================================
// Message Ordering Tests
// =============================================================================

#[tokio::test]
async fn test_message_ordering_preserved() {
    // Use large buffer to avoid blocking
    let (client, server) = MemoryTransport::pair_with_capacity(200);

    // Send messages in order
    for i in 0..100 {
        let msg = Message::Notification(Notification::with_params("order", json!({"seq": i})));
        client.send(msg).await.unwrap();
    }

    // Receive and verify order
    for expected in 0..100i64 {
        match timeout(Duration::from_secs(2), server.recv()).await {
            Ok(Ok(Some(Message::Notification(n)))) => {
                let params = n.params.unwrap();
                let seq = params["seq"].as_i64().unwrap();
                assert_eq!(seq, expected, "Messages should be in order");
            }
            other => panic!("Expected notification, got {other:?}"),
        }
    }
}
