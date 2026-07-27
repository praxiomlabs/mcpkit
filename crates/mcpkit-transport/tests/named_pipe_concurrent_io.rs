//! `NamedPipeTransport` must support sending while a receive is outstanding.
//!
//! A named pipe is a single bidirectional handle, so both directions used to
//! share one mutex, and `recv` held it across the read that waits for the peer.
//! `send` needed that same mutex, so a transport that answers requests
//! deadlocked. `tokio::io::split` takes the shared lock only for the duration
//! of one poll, never across a pending read.
//!
//! Windows-only: the transport does not exist on other platforms.

#![cfg(all(windows, feature = "tokio-runtime"))]

use mcpkit_core::protocol::{Message, Request, RequestId};
use mcpkit_transport::windows::{NamedPipeServer, NamedPipeTransport};
use mcpkit_transport::{Transport, TransportListener};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn send_completes_while_recv_is_parked() {
    let name = format!("mcpkit-test-{}", std::process::id());
    let listener = NamedPipeServer::new(&name).expect("create server");

    let client_name = name.clone();
    let client = tokio::spawn(async move {
        // Give the server a moment to post its first pipe instance.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let t = NamedPipeTransport::connect(client_name)
            .await
            .expect("connect");
        // Stay connected and silent so the server's receive remains parked.
        tokio::time::sleep(Duration::from_secs(10)).await;
        drop(t);
    });

    let server = Arc::new(listener.accept().await.expect("accept"));

    let parked = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.recv().await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;

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
}
