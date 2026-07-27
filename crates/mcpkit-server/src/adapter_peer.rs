//! Server→client request/response peer for the HTTP adapters (#153).
//!
//! The stdio runtime has had server-initiated requests since #111 via
//! `TransportPeer`; the HTTP adapters could not make them at all (every
//! `Context` got a `NoOpPeer`). This module is the shared primitive that
//! closes the gap:
//!
//! - [`SessionOutbound`] — per-session outbound-request state: id
//!   allocation and response correlation. Also used by the stdio runtime's
//!   `ServerState`, so there is exactly one correlation implementation.
//! - [`OutboundOwner`] — the session map's exclusive owner token; dropping
//!   it fails all pending requests. Deliberately separate from the `Arc`
//!   the peers clone: a `Drop` on the shared `Arc` could only fire once no
//!   waiter exists, i.e. exactly when it has nothing to do.
//! - [`SessionSink`] — how a peer delivers a message to the session's SSE
//!   stream(s); implemented per adapter over the session's
//!   [`StreamRegistry`](crate::streams::StreamRegistry).
//! - [`SessionPeer`] — the [`Peer`] implementation handlers see:
//!   notifications are best-effort (stored for replay, never an error when
//!   no stream is open); requests fail fast on a missing stream after a
//!   bounded reconnect grace, and time out per method class.

// `clippy::option_if_let_else` (nursery): the flagged site guards a lock acquisition whose else branch is a
// no-op; the if-let states that more plainly.
#![allow(clippy::option_if_let_else)]

use crate::context::Peer;
use futures::channel::oneshot;
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::{Message, Notification, Request, RequestId, Response};
use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// How long a request tolerates the session having no live SSE stream.
///
/// Covers the ordinary reconnect blip before failing fast. Fixed by design:
/// above the ~3s conventional SSE retry, and the adapters emit `retry: 2000`,
/// so client cadence is dictated rather than guessed.
pub const RECONNECT_GRACE: Duration = Duration::from_secs(5);

/// How often the no-stream watcher re-checks for a live stream.
const GRACE_POLL: Duration = Duration::from_millis(100);

// ============================================================================
// Correlation registry
// ============================================================================

/// Per-session outbound-request state: id allocation + response correlation.
///
/// The same shape the stdio runtime's `ServerState` uses (which now delegates
/// here): plain incrementing numeric ids — JSON-RPC ids are per-sender, so
/// they cannot collide with client-chosen ids — and a oneshot per pending
/// request.
#[derive(Debug, Default)]
pub struct SessionOutbound {
    next_id: AtomicU64,
    pending: RwLock<HashMap<RequestId, oneshot::Sender<Response>>>,
}

impl SessionOutbound {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Allocate a unique id for a server-initiated request.
    #[must_use]
    pub fn next_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Register a pending request, returning the receiver that resolves when
    /// the matching response arrives.
    pub fn register(&self, id: RequestId) -> oneshot::Receiver<Response> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending.write() {
            pending.insert(id, tx);
        }
        rx
    }

    /// Drop a pending request (e.g. on timeout or cancellation).
    pub fn remove(&self, id: &RequestId) {
        if let Ok(mut pending) = self.pending.write() {
            pending.remove(id);
        }
    }

    /// Route an inbound response to the request waiting for it. Returns
    /// `false` when no pending request matches (late or unknown id — the
    /// caller logs and drops, matching the stdio runtime).
    pub fn resolve(&self, response: Response) -> bool {
        let sender = self
            .pending
            .write()
            .ok()
            .and_then(|mut pending| pending.remove(&response.id));
        match sender {
            Some(sender) => {
                let _ = sender.send(response);
                true
            }
            None => false,
        }
    }

    /// Fail every pending request (session terminated, expired, or the
    /// transport closed). Dropping the senders resolves the waiting
    /// receivers with an error.
    pub fn fail_all(&self) {
        if let Ok(mut pending) = self.pending.write() {
            pending.clear();
        }
    }
}

/// Exclusive owner token for a session's [`SessionOutbound`].
///
/// Held only by the session map; dropping it (session reap, DELETE, store
/// teardown) fails all pending requests so waiting hooks resolve immediately
/// instead of running out their timeout.
///
/// Peers clone the inner [`Arc`] — never the owner — so a waiter's own clone
/// cannot keep the failure from firing.
#[derive(Debug)]
pub struct OutboundOwner(Arc<SessionOutbound>);

impl OutboundOwner {
    /// Create an owner (and its registry).
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(SessionOutbound::new()))
    }

    /// The shared registry, for cloning into peers and response routing.
    #[must_use]
    pub const fn outbound(&self) -> &Arc<SessionOutbound> {
        &self.0
    }
}

impl Default for OutboundOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OutboundOwner {
    fn drop(&mut self) {
        self.0.fail_all();
    }
}

// ============================================================================
// Sink
// ============================================================================

/// Why a sink could not deliver a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkError {
    /// The session has no live SSE stream to deliver a request on.
    /// Matchable so hooks can degrade deliberately.
    NoClientStream,
    /// The message could not be serialized.
    Serialization(String),
}

impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClientStream => {
                write!(
                    f,
                    "client has no open SSE stream; server-initiated requests require one"
                )
            }
            Self::Serialization(e) => write!(f, "failed to serialize message: {e}"),
        }
    }
}

impl std::error::Error for SinkError {}

/// How a peer delivers a message to the session's client stream(s).
///
/// Implemented per adapter (a thin wrapper over the session's
/// [`StreamRegistry`](crate::streams::StreamRegistry)). Boxed futures,
/// deliberately matching [`Peer`]'s own shape: the trait must be dyn-able
/// because the peer is threaded through erased call paths, while each
/// adapter's sink is a per-crate type.
pub trait SessionSink: Send + Sync {
    /// Store-and-forward a notification. MUST NOT error when no stream is
    /// open (mirrors the runtime: a client without a stream simply misses
    /// best-effort notifications; the event is stored for replay).
    fn send_notification(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>>;

    /// Deliver a server-initiated request on the session's designated live
    /// stream. Pure predicate: fails immediately with
    /// [`SinkError::NoClientStream`] when no live stream is registered — the
    /// reconnect grace lives in [`SessionPeer`], which owns the deadline.
    fn send_request(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>>;

    /// Whether the session currently has a live SSE stream (drives the
    /// mid-flight reconnect grace).
    fn has_live_stream(&self) -> bool;
}

/// The standard adapter sink: delivers peer messages onto a session's
/// [`StreamRegistry`](crate::streams::StreamRegistry). Framework-free —
/// every HTTP adapter uses this same implementation.
#[cfg(feature = "tokio")]
pub struct StreamRegistrySink {
    registry: Arc<crate::streams::StreamRegistry>,
}

#[cfg(feature = "tokio")]
impl StreamRegistrySink {
    /// Create a sink over a session's stream registry.
    #[must_use]
    pub const fn new(registry: Arc<crate::streams::StreamRegistry>) -> Self {
        Self { registry }
    }
}

#[cfg(feature = "tokio")]
impl SessionSink for StreamRegistrySink {
    fn send_notification(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>> {
        Box::pin(async move {
            let json = serde_json::to_string(&message)
                .map_err(|e| SinkError::Serialization(e.to_string()))?;
            // Best-effort: with no live stream the notification is dropped
            // (runtime parity — a client without a stream misses it).
            let _ = self.registry.send("message", json);
            Ok(())
        })
    }

    fn send_request(
        &self,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>> {
        Box::pin(async move {
            let json = serde_json::to_string(&message)
                .map_err(|e| SinkError::Serialization(e.to_string()))?;
            self.registry
                .send("message", json)
                .map(|_| ())
                .ok_or(SinkError::NoClientStream)
        })
    }

    fn has_live_stream(&self) -> bool {
        self.registry.has_live_stream()
    }
}

// ============================================================================
// Peer
// ============================================================================

/// Request timeouts by method class.
///
/// One number cannot serve both: elicitation is human-in-the-loop (60s is
/// short), while `roots/list` is a machine round-trip (60s is long). The
/// timeout is resolved inside [`SessionPeer`] by method name because
/// `Context` funnels every server-initiated request through
/// `Peer::request(method, params)`, which has no timeout parameter.
#[derive(Debug, Clone, Copy)]
pub struct PeerTimeouts {
    /// Timeout for machine round-trips (everything but elicitation).
    pub default: Duration,
    /// Timeout for `elicitation/*` requests (a human answers these).
    pub elicitation: Duration,
}

impl Default for PeerTimeouts {
    fn default() -> Self {
        Self {
            default: Duration::from_secs(60),
            elicitation: Duration::from_secs(300),
        }
    }
}

impl PeerTimeouts {
    fn resolve(&self, method: &str) -> Duration {
        if method.starts_with("elicitation/") {
            self.elicitation
        } else {
            self.default
        }
    }
}

/// A request-capable [`Peer`] for one adapter session.
pub struct SessionPeer {
    sink: Arc<dyn SessionSink>,
    outbound: Arc<SessionOutbound>,
    timeouts: PeerTimeouts,
    grace: Duration,
}

impl SessionPeer {
    /// Create a peer over a session's sink and outbound registry.
    #[must_use]
    pub fn new(
        sink: Arc<dyn SessionSink>,
        outbound: Arc<SessionOutbound>,
        timeouts: PeerTimeouts,
    ) -> Self {
        Self {
            sink,
            outbound,
            timeouts,
            grace: RECONNECT_GRACE,
        }
    }

    /// Override the reconnect grace. Test hook — the grace is a fixed
    /// constant by design.
    #[doc(hidden)]
    #[must_use]
    pub const fn with_reconnect_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// Resolves when the session has had no live stream for `grace`
    /// continuously (checked every [`GRACE_POLL`]).
    async fn no_stream_for_grace(sink: Arc<dyn SessionSink>, grace: Duration) {
        let mut none_since: Option<Instant> = None;
        loop {
            if sink.has_live_stream() {
                none_since = None;
            } else {
                let since = *none_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= grace {
                    return;
                }
            }
            mcpkit_transport::runtime::sleep(GRACE_POLL).await;
        }
    }
}

impl std::fmt::Debug for SessionPeer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionPeer")
            .field("timeouts", &self.timeouts)
            .field("grace", &self.grace)
            .finish_non_exhaustive()
    }
}

impl Peer for SessionPeer {
    fn notify(
        &self,
        notification: Notification,
    ) -> Pin<Box<dyn Future<Output = Result<(), McpError>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        Box::pin(async move {
            sink.send_notification(Message::Notification(notification))
                .await
                .map_err(|e| McpError::internal(e.to_string()))
        })
    }

    fn request(
        &self,
        method: Cow<'static, str>,
        params: Option<serde_json::Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Response, McpError>> + Send + '_>> {
        let sink = Arc::clone(&self.sink);
        let outbound = Arc::clone(&self.outbound);
        let timeout = self.timeouts.resolve(&method);
        let grace = self.grace;
        Box::pin(async move {
            use futures::future::{Either, select};

            let started = Instant::now();
            let id = outbound.next_id();
            let rx = outbound.register(id.clone());
            let request = match params {
                Some(p) => Request::with_params(method, id.clone(), p),
                None => Request::new(method, id.clone()),
            };
            let message = Message::Request(request);

            // Send-time reconnect grace: a session whose stream dropped a
            // moment ago gets `grace` to come back before we fail fast.
            let send_deadline = grace.min(timeout);
            loop {
                match sink.send_request(message.clone()).await {
                    Ok(()) => break,
                    Err(SinkError::NoClientStream) if started.elapsed() < send_deadline => {
                        mcpkit_transport::runtime::sleep(GRACE_POLL).await;
                    }
                    Err(e) => {
                        outbound.remove(&id);
                        return Err(McpError::internal(e.to_string()));
                    }
                }
            }

            // Await the response, bounded by the per-method timeout, failing
            // early if the session goes streamless for a full grace window
            // mid-flight (the request may be sitting undelivered in a dead
            // stream's replay buffer; a client that already consumed it and
            // answers via POST resolves `rx` before the watcher can fire).
            let remaining = timeout.saturating_sub(started.elapsed());
            let deadline = mcpkit_transport::runtime::sleep(remaining);
            let watcher = Self::no_stream_for_grace(Arc::clone(&sink), grace);
            futures::pin_mut!(deadline);
            futures::pin_mut!(watcher);
            let interrupt = select(deadline, watcher);
            match select(rx, interrupt).await {
                Either::Left((Ok(response), _)) => Ok(response),
                Either::Left((Err(_canceled), _)) => {
                    outbound.remove(&id);
                    Err(McpError::internal("session closed before a reply arrived"))
                }
                Either::Right((Either::Left(((), _)), _)) => {
                    outbound.remove(&id);
                    Err(McpError::internal(format!(
                        "server-initiated request timed out after {timeout:?}"
                    )))
                }
                Either::Right((Either::Right(((), _)), _)) => {
                    outbound.remove(&id);
                    Err(McpError::internal(
                        "client has had no open SSE stream for the reconnect grace; \
                         server-initiated request abandoned",
                    ))
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;

    /// Mock sink: delivery recorded; liveness toggleable.
    struct MockSink {
        live: AtomicBool,
        sent: Mutex<Vec<Message>>,
    }

    impl MockSink {
        fn new(live: bool) -> Arc<Self> {
            Arc::new(Self {
                live: AtomicBool::new(live),
                sent: Mutex::new(Vec::new()),
            })
        }
    }

    impl SessionSink for MockSink {
        fn send_notification(
            &self,
            message: Message,
        ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>> {
            // Contract: notifications never fail on "no stream".
            self.sent.lock().unwrap().push(message);
            Box::pin(async { Ok(()) })
        }
        fn send_request(
            &self,
            message: Message,
        ) -> Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + '_>> {
            if self.live.load(Ordering::SeqCst) {
                self.sent.lock().unwrap().push(message);
                Box::pin(async { Ok(()) })
            } else {
                Box::pin(async { Err(SinkError::NoClientStream) })
            }
        }
        fn has_live_stream(&self) -> bool {
            self.live.load(Ordering::SeqCst)
        }
    }

    fn peer(sink: Arc<MockSink>) -> SessionPeer {
        SessionPeer::new(
            sink,
            Arc::new(SessionOutbound::new()),
            PeerTimeouts::default(),
        )
    }

    fn sent_request_id(sink: &MockSink) -> RequestId {
        let sent = sink.sent.lock().unwrap();
        match sent.first().expect("a request was sent") {
            Message::Request(r) => r.id.clone(),
            other => panic!("expected request, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn request_correlates_response() {
        let sink = MockSink::new(true);
        let outbound = Arc::new(SessionOutbound::new());
        let p = SessionPeer::new(sink.clone(), Arc::clone(&outbound), PeerTimeouts::default());

        let fut = p.request(Cow::Borrowed("roots/list"), None);
        futures::pin_mut!(fut);
        // Drive until the request is sent.
        assert!(futures::poll!(fut.as_mut()).is_pending());
        let id = sent_request_id(&sink);

        // The client answers via POST -> resolve.
        assert!(outbound.resolve(Response::success(id, serde_json::json!({"roots": []}))));
        let response = fut.await.expect("correlated");
        assert_eq!(response.result.unwrap()["roots"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn timeout_cleans_up_pending() {
        let sink = MockSink::new(true);
        let outbound = Arc::new(SessionOutbound::new());
        let p = SessionPeer::new(
            sink.clone(),
            Arc::clone(&outbound),
            PeerTimeouts {
                default: Duration::from_millis(50),
                elicitation: Duration::from_millis(50),
            },
        );

        let err = p
            .request(Cow::Borrowed("roots/list"), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        // Pending entry removed: a late response resolves nothing.
        let id = sent_request_id(&sink);
        assert!(!outbound.resolve(Response::success(id, serde_json::json!({}))));
    }

    #[tokio::test]
    async fn owner_drop_fails_pending_waiters() {
        let sink = MockSink::new(true);
        let owner = OutboundOwner::new();
        let p = SessionPeer::new(
            sink.clone(),
            Arc::clone(owner.outbound()),
            PeerTimeouts::default(),
        );

        let fut = p.request(Cow::Borrowed("roots/list"), None);
        futures::pin_mut!(fut);
        assert!(futures::poll!(fut.as_mut()).is_pending());

        // Session reaped/DELETEd: the map's exclusive owner drops.
        drop(owner);
        let err = fut.await.unwrap_err();
        assert!(err.to_string().contains("closed"), "{err}");
    }

    #[tokio::test]
    async fn notifications_never_fail_without_stream() {
        let sink = MockSink::new(false);
        let p = peer(sink.clone());
        p.notify(Notification::new("notifications/progress"))
            .await
            .expect("best-effort notification must not error");
        assert_eq!(sink.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn request_fails_fast_after_grace_without_stream() {
        let sink = MockSink::new(false);
        let p = peer(sink.clone()).with_reconnect_grace(Duration::from_millis(50));

        let started = Instant::now();
        let err = p
            .request(Cow::Borrowed("roots/list"), None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("SSE stream"),
            "expected no-stream error, got: {err}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must fail at the grace, not the request timeout"
        );
    }

    #[tokio::test]
    async fn request_survives_reconnect_within_grace() {
        let sink = MockSink::new(false);
        let outbound = Arc::new(SessionOutbound::new());
        let p = SessionPeer::new(sink.clone(), Arc::clone(&outbound), PeerTimeouts::default())
            .with_reconnect_grace(Duration::from_secs(2));

        let sink2 = sink.clone();
        let reconnect = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            sink2.live.store(true, Ordering::SeqCst);
        });

        let fut = p.request(Cow::Borrowed("roots/list"), None);
        futures::pin_mut!(fut);
        // Poll until the send goes through post-reconnect, then answer.
        loop {
            assert!(
                futures::poll!(fut.as_mut()).is_pending(),
                "request should still be awaiting its response"
            );
            if !sink.sent.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let id = sent_request_id(&sink);
        assert!(outbound.resolve(Response::success(id, serde_json::json!({}))));
        fut.await.expect("survived the blip");
        reconnect.await.unwrap();
    }

    #[tokio::test]
    async fn midflight_stream_loss_fails_after_grace() {
        let sink = MockSink::new(true);
        let p = peer(sink.clone()).with_reconnect_grace(Duration::from_millis(80));

        let sink2 = sink.clone();
        let killer = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            sink2.live.store(false, Ordering::SeqCst);
        });

        let started = Instant::now();
        let err = p
            .request(Cow::Borrowed("roots/list"), None)
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("reconnect grace"),
            "expected mid-flight grace failure, got: {err}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        killer.await.unwrap();
    }

    #[tokio::test]
    async fn cross_session_ids_do_not_collide() {
        // Per-session ids both start at 1: session B's response id 1 must not
        // resolve session A's pending id 1 (regression from review round 2).
        let a = Arc::new(SessionOutbound::new());
        let b = Arc::new(SessionOutbound::new());
        let id_a = a.next_id();
        let _rx_a = a.register(id_a.clone());
        let id_b = b.next_id();
        assert_eq!(id_a, id_b, "both sessions allocate id 1");

        assert!(!b.resolve(Response::success(id_b, serde_json::json!({})))); // B has no pending
        // A's pending entry is untouched by B's traffic.
        assert!(a.resolve(Response::success(id_a, serde_json::json!({}))));
    }

    #[test]
    fn elicitation_gets_the_longer_timeout() {
        let t = PeerTimeouts::default();
        assert_eq!(t.resolve("elicitation/create"), t.elicitation);
        assert_eq!(t.resolve("elicitation/createUrl"), t.elicitation);
        assert_eq!(t.resolve("roots/list"), t.default);
        assert_eq!(t.resolve("sampling/createMessage"), t.default);
    }
}
