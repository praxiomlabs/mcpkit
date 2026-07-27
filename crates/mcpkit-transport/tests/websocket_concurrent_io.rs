//! `WebSocketTransport` must support sending while a receive is outstanding.
//!
//! Both directions used to share one mutex, and `recv` holds its guard across
//! the await that waits for the peer. Any client that sends while a receive is
//! in flight — the normal shape of a request/response client — deadlocked.

#![cfg(feature = "websocket")]

use futures::StreamExt;
use mcpkit_core::protocol::{Message, Request, RequestId};
use mcpkit_transport::websocket::WebSocketConfig;
use mcpkit_transport::{Transport, WebSocketTransport};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn send_completes_while_recv_is_parked() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    // Accept, then stay silent so the client's receive stays parked.
    let server = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await
            && let Ok(ws) = tokio_tungstenite::accept_async(stream).await
        {
            let (_tx, mut rx) = ws.split();
            while let Some(Ok(_)) = rx.next().await {}
        }
    });

    let transport = Arc::new(
        WebSocketTransport::connect(WebSocketConfig::new(format!("ws://{addr}/mcp")))
            .await
            .expect("connect"),
    );

    let parked = {
        let transport = Arc::clone(&transport);
        tokio::spawn(async move { transport.recv().await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;

    let sent = tokio::time::timeout(
        Duration::from_secs(5),
        transport.send(Message::Request(Request::new("ping", RequestId::Number(1)))),
    )
    .await;

    assert!(
        sent.is_ok(),
        "send() blocked while recv() was parked — the read and write halves are \
         contending on one lock again"
    );
    sent.expect("not timed out").expect("send succeeded");

    parked.abort();
    server.abort();
}
