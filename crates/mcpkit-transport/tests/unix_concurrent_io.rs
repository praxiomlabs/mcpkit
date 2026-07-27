//! `UnixTransport` must support sending while a receive is outstanding.
//!
//! A server run loop is always parked in `recv` awaiting the client's next
//! message at the moment it needs to write a response. When both directions
//! shared one mutex, that write could never acquire it and every session
//! deadlocked on its first request.

#![cfg(all(unix, feature = "tokio-runtime"))]

use mcpkit_core::protocol::{Message, Request, RequestId};
use mcpkit_transport::{Transport, TransportListener, UnixListener, UnixTransport};
use std::sync::Arc;
use std::time::Duration;

/// A socket path unique to this test binary and case.
fn sock_path(case: &str) -> String {
    format!("/tmp/mcp-unix-{}-{case}.sock", std::process::id())
}

#[tokio::test]
async fn send_completes_while_recv_is_parked() {
    let path = sock_path("concurrent");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).await.expect("bind");

    let client_path = path.clone();
    let client = tokio::spawn(async move {
        let t = UnixTransport::connect(&client_path).await.expect("connect");
        // Hold the connection open without sending, so the server's `recv`
        // stays parked for the duration of the test.
        tokio::time::sleep(Duration::from_secs(30)).await;
        drop(t);
    });

    let server = Arc::new(listener.accept().await.expect("accept"));

    // Park a receive. No data is coming, so it remains pending.
    let parked = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.recv().await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The write must not wait on the reader.
    let sent = tokio::time::timeout(
        Duration::from_secs(5),
        server.send(Message::Request(Request::new("ping", RequestId::Number(1)))),
    )
    .await;

    assert!(
        sent.is_ok(),
        "send() blocked while recv() was parked — the read and write halves are \
         contending on one lock again"
    );
    sent.expect("not timed out").expect("send succeeded");

    parked.abort();
    client.abort();
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn dropping_a_parked_recv_leaves_the_transport_usable() {
    let path = sock_path("cancel");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).await.expect("bind");

    let client_path = path.clone();
    let client = tokio::spawn(async move {
        let t = UnixTransport::connect(&client_path).await.expect("connect");
        // Send only after the server has polled and dropped its first `recv`.
        tokio::time::sleep(Duration::from_millis(300)).await;
        t.send(Message::Request(Request::new("ping", RequestId::Number(1))))
            .await
            .expect("send");
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let server = listener.accept().await.expect("accept");

    // Poll a receive to the point where it parks, then drop it. The server run
    // loop does exactly this whenever another arm of its `select` wins.
    {
        let recv = server.recv();
        tokio::pin!(recv);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut recv)
                .await
                .is_err(),
            "recv() should still be parked; the test cannot exercise a drop otherwise"
        );
    }

    let received = tokio::time::timeout(Duration::from_secs(5), server.recv())
        .await
        .expect("recv() did not return after the earlier one was dropped")
        .expect("recv() errored");

    assert!(
        received.is_some(),
        "recv() returned Ok(None) after a dropped receive — the reader was lost, \
         and the run loop reads this as a closed connection"
    );

    client.abort();
    let _ = std::fs::remove_file(&path);
}
