//! Neither the inbound stream nor the ambient notification queue may starve
//! the other in `ServerRuntime::run`.
//!
//! `futures::future::select` polls its left argument first and returns as soon
//! as it is ready, so a source that always sits on the right only runs when
//! every source to its left is pending. Ambient notifications sat there, and a
//! client with a backlog pushed them behind the whole backlog.

use mcpkit_core::protocol::{Message, Notification, Request, RequestId};
use mcpkit_server::{ServerBuilder, ServerHandler, ServerRuntime};
use mcpkit_transport::{MemoryTransport, Transport};
use std::time::Duration;
use tokio::time::timeout;

struct Bare;
impl ServerHandler for Bare {
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo::new("fairness", "1.0.0")
    }
}

const FLOOD: usize = 200;

/// Set up a server with `FLOOD` requests already queued and `ambient`
/// notifications published before the loop starts, then report the index at
/// which each kind of message came back.
async fn drain(ambient: usize) -> (Vec<usize>, Vec<usize>) {
    let (client, server) = MemoryTransport::pair_with_capacity(FLOOD + 8);
    let built = ServerBuilder::new(Bare).build();
    let runtime = ServerRuntime::new(built, server);

    for _ in 0..ambient {
        runtime
            .state()
            .publish_notification(Notification::new("notifications/probe"));
    }
    for i in 0..FLOOD {
        client
            .send(Message::Request(Request::new(
                "ping",
                RequestId::Number(i as u64),
            )))
            .await
            .expect("preload");
    }

    tokio::spawn(async move {
        let _ = runtime.run().await;
    });

    let (mut notes, mut responses) = (Vec::new(), Vec::new());
    for i in 0..(FLOOD + ambient) {
        let Ok(Ok(Some(msg))) = timeout(Duration::from_secs(10), client.recv()).await else {
            break;
        };
        match msg {
            Message::Notification(_) => notes.push(i),
            Message::Response(_) => responses.push(i),
            Message::Request(_) => {}
        }
    }
    (notes, responses)
}

#[tokio::test]
async fn an_ambient_notification_is_not_starved_by_queued_requests() {
    let (notes, responses) = drain(1).await;

    assert_eq!(
        responses.len(),
        FLOOD,
        "every request must still be answered"
    );
    assert_eq!(notes.len(), 1, "the notification must be delivered");
    // Alternating gives the notification first refusal on every other
    // iteration, so it cannot land behind more than a couple of responses.
    assert!(
        notes[0] < 4,
        "notification came out at index {} of {}; it is starving behind the \
         inbound backlog again",
        notes[0],
        FLOOD
    );
}

#[tokio::test]
async fn queued_requests_are_not_starved_by_ambient_notifications() {
    // The inverse failure: giving ambient absolute priority instead of
    // alternating would push every response behind every notification.
    let (notes, responses) = drain(FLOOD).await;

    assert_eq!(
        responses.len(),
        FLOOD,
        "every request must still be answered"
    );
    assert_eq!(notes.len(), FLOOD, "every notification must be delivered");
    assert!(
        responses[0] < 4,
        "first response came out at index {}; requests are starving behind the \
         notification queue",
        responses[0]
    );
}
