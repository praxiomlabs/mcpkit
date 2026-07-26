//! Spec-anchored protocol-version negotiation checks.
//!
//! A schema diff can confirm the four version *strings* exist; it cannot see
//! whether negotiation picks the right one. Per the 2025-11-25 initialization
//! flow, a server that supports the requested version MUST reply with that same
//! version, and otherwise MUST reply with one it does support (SHOULD be its
//! latest). These drive the real initialize handler, not the pure function.

use mcpkit_core::protocol::{Message, Request, RequestId, Response};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::{ServerBuilder, ServerRuntime};
use mcpkit_transport::{MemoryTransport, Transport};
use serde_json::json;
use std::time::Duration;
use tokio::time::timeout;

struct H;

impl mcpkit_server::ServerHandler for H {
    fn server_info(&self) -> mcpkit_core::capability::ServerInfo {
        mcpkit_core::capability::ServerInfo::new("negotiation", "1.0.0")
    }
}

/// Send one `initialize` to a fresh server and return the negotiated version.
async fn negotiate(requested: &str) -> String {
    let (client, server) = MemoryTransport::pair();
    let built = ServerBuilder::new(H).build();
    let runtime = ServerRuntime::new(built, server);
    let handle = tokio::spawn(async move {
        let _ = runtime.run().await;
    });

    client
        .send(Message::Request(Request::with_params(
            "initialize",
            RequestId::Number(1),
            json!({
                "protocolVersion": requested,
                "capabilities": {},
                "clientInfo": { "name": "c", "version": "1.0" }
            }),
        )))
        .await
        .expect("send");

    let msg = timeout(Duration::from_secs(5), client.recv())
        .await
        .expect("timed out")
        .expect("recv ok")
        .expect("some message");
    let Message::Response(Response { result, error, .. }) = msg else {
        panic!("expected a response");
    };
    assert!(error.is_none(), "initialize errored: {error:?}");

    let negotiated = result
        .as_ref()
        .and_then(|r| r.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("no protocolVersion in result: {result:?}"))
        .to_string();

    drop(client);
    let _ = timeout(Duration::from_secs(2), handle).await;
    negotiated
}

/// Every published version must be echoed back unchanged. A server that
/// silently upgraded the client to its own latest would break a client that
/// only speaks the older one.
#[tokio::test]
async fn every_published_version_is_accepted_without_downgrade() {
    for version in ProtocolVersion::ALL {
        let requested = version.as_str();
        let negotiated = negotiate(requested).await;
        assert_eq!(
            negotiated, requested,
            "requesting {requested} must negotiate {requested}, got {negotiated}"
        );
    }
}

/// All four are distinct strings, so the loop above is not four assertions
/// about the same value.
#[test]
fn the_four_published_versions_are_distinct() {
    let mut seen: Vec<&str> = ProtocolVersion::ALL
        .iter()
        .map(ProtocolVersion::as_str)
        .collect();
    let total = seen.len();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(total, 4, "expected exactly four published versions");
    assert_eq!(seen.len(), 4, "published versions must be distinct");
}

/// An unrecognized version is not an error: the server counter-offers a version
/// it does support, and the client decides whether it can proceed.
#[tokio::test]
async fn unknown_version_counter_offers_a_supported_version() {
    for requested in ["not-a-version", "1.0.0", ""] {
        let negotiated = negotiate(requested).await;
        assert!(
            ProtocolVersion::ALL
                .iter()
                .any(|v| v.as_str() == negotiated),
            "counter-offer {negotiated} for {requested:?} is not a supported version"
        );
    }
}

/// A version newer than anything this build knows must come back as the
/// server's latest, not as the unknown string echoed uncritically.
#[tokio::test]
async fn future_version_negotiates_down_to_latest() {
    let negotiated = negotiate("2099-01-01").await;
    assert_eq!(
        negotiated,
        ProtocolVersion::LATEST.as_str(),
        "a future version must negotiate to the server's latest"
    );
}
