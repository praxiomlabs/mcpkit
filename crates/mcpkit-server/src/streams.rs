//! Per-session, per-stream SSE delivery (shared by the HTTP adapters).
//!
//! The Streamable HTTP spec requires that the server *"MUST send each of its
//! JSON-RPC messages on only one of the connected streams"*, that SSE event
//! ids be *"globally unique across all streams within that session"* and
//! *"SHOULD encode sufficient information to identify the originating
//! stream"*, and that the server *"MUST NOT replay messages that would have
//! been delivered on a different stream"*.
//!
//! [`StreamRegistry`] implements those rules once, so the four adapters share
//! one delivery/replay implementation instead of four drifting copies:
//!
//! - each GET opens (or resumes) one stream with its own bounded `mpsc`
//!   channel and its own replay buffer;
//! - every outbound message is stored on, and delivered to, exactly **one**
//!   stream — the *designated* stream (the oldest live one, stable while it
//!   lives; a resumed stream keeps its identity and therefore its
//!   designation);
//! - event ids are `{stream_id}-{seq}`, allocated **once at store time**, so
//!   the id on the wire always equals the id in the buffer and
//!   `Last-Event-ID` replay works;
//! - replay serves only events buffered on the stream the cursor names;
//! - a full channel kills its stream (never a silent skip): the buffer is
//!   retained for [`StreamConfig::max_age`], so a reconnecting client
//!   resumes and replays what the dead channel missed.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// An event stored for delivery and replay on one stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Event id (`{stream_id}-{seq}`), allocated at store time.
    pub id: String,
    /// SSE event type (e.g. `connected`, `message`).
    pub event_type: String,
    /// Event payload.
    pub data: String,
}

/// Configuration for a session's stream registry.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StreamConfig {
    /// Maximum buffered events retained per stream for replay.
    pub max_events_per_stream: usize,
    /// How long a dead stream's replay buffer is retained (spec resumability
    /// window). Also bounds the age of buffered events on live streams.
    pub max_age: Duration,
    /// Per-stream delivery channel capacity. A stream whose channel is full
    /// is killed explicitly (client resumes via `Last-Event-ID`).
    pub channel_capacity: usize,
}

impl StreamConfig {
    /// A stream configuration with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Maximum buffered events retained per stream for replay.
    #[must_use]
    pub const fn max_events_per_stream(mut self, max: usize) -> Self {
        self.max_events_per_stream = max;
        self
    }

    /// How long a dead stream's replay buffer is retained.
    #[must_use]
    pub const fn max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }

    /// Per-stream delivery channel capacity.
    #[must_use]
    pub const fn channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_events_per_stream: 1000,
            max_age: Duration::from_secs(300),
            channel_capacity: 100,
        }
    }
}

#[derive(Debug)]
struct StreamSlot {
    id: u64,
    /// Next sequence number to allocate on this stream.
    seq: u64,
    buffer: VecDeque<(Instant, StoredEvent)>,
    /// Live delivery channel; `None` once the stream has died.
    sender: Option<mpsc::Sender<StoredEvent>>,
    opened: Instant,
    died: Option<Instant>,
}

impl StreamSlot {
    fn store(&mut self, event_type: &str, data: String, config: &StreamConfig) -> StoredEvent {
        let event = StoredEvent {
            id: format!("{}-{}", self.id, self.seq),
            event_type: event_type.to_string(),
            data,
        };
        self.seq += 1;
        self.buffer.push_back((Instant::now(), event.clone()));
        while self.buffer.len() > config.max_events_per_stream {
            self.buffer.pop_front();
        }
        while self
            .buffer
            .front()
            .is_some_and(|(at, _)| at.elapsed() > config.max_age)
        {
            self.buffer.pop_front();
        }
        event
    }
}

/// Per-session registry of SSE streams. See the module docs for the rules it
/// enforces.
#[derive(Debug)]
pub struct StreamRegistry {
    inner: Mutex<Inner>,
    config: StreamConfig,
}

#[derive(Debug)]
struct Inner {
    streams: Vec<StreamSlot>,
    next_stream_id: u64,
}

/// One live stream: the receiving half consumed by the adapter's SSE loop.
///
/// Dropping the handle marks the stream dead in the registry (its replay
/// buffer is retained for [`StreamConfig::max_age`]).
#[derive(Debug)]
pub struct StreamHandle {
    stream_id: u64,
    rx: mpsc::Receiver<StoredEvent>,
    registry: Arc<StreamRegistry>,
}

impl StreamHandle {
    /// This stream's id (the `{stream_id}` half of its event ids).
    #[must_use]
    pub const fn stream_id(&self) -> u64 {
        self.stream_id
    }

    /// Receive the next event queued for this stream. `None` when the stream
    /// has been killed (e.g. channel overflow) or the registry dropped.
    pub async fn recv(&mut self) -> Option<StoredEvent> {
        self.rx.recv().await
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.registry.mark_dead(self.stream_id);
    }
}

impl StreamRegistry {
    /// Create a registry with the given configuration.
    #[must_use]
    pub fn new(config: StreamConfig) -> Self {
        Self {
            inner: Mutex::new(Inner {
                streams: Vec::new(),
                next_stream_id: 1,
            }),
            config,
        }
    }

    /// Open a new stream, storing and queueing a priming event (spec: the
    /// server SHOULD immediately send an event with an id so the client can
    /// reconnect with `Last-Event-ID`). Returns the handle and the priming
    /// event.
    pub fn open(
        self: &Arc<Self>,
        prime_event_type: &str,
        prime_data: String,
    ) -> (StreamHandle, StoredEvent) {
        let (tx, rx) = mpsc::channel(self.config.channel_capacity);
        let mut inner = self.inner.lock().expect("stream registry lock");
        Self::reap(&mut inner, &self.config);
        let id = inner.next_stream_id;
        inner.next_stream_id += 1;
        let mut slot = StreamSlot {
            id,
            seq: 0,
            buffer: VecDeque::new(),
            sender: Some(tx),
            opened: Instant::now(),
            died: None,
        };
        let prime = slot.store(prime_event_type, prime_data, &self.config);
        inner.streams.push(slot);
        drop(inner);
        (
            StreamHandle {
                stream_id: id,
                rx,
                registry: Arc::clone(self),
            },
            prime,
        )
    }

    /// Resume the stream named by `last_event_id` (`{stream_id}-{seq}`),
    /// returning a fresh handle for the SAME stream identity plus the
    /// buffered events after the cursor. `None` if the id does not parse, the
    /// stream is unknown, or its buffer has been reaped.
    ///
    /// A resumed stream keeps its id — and therefore its designation if it
    /// was the designated stream.
    pub fn resume(
        self: &Arc<Self>,
        last_event_id: &str,
    ) -> Option<(StreamHandle, Vec<StoredEvent>)> {
        let (stream_id, seq) = parse_event_id(last_event_id)?;
        let (tx, rx) = mpsc::channel(self.config.channel_capacity);
        let mut inner = self.inner.lock().expect("stream registry lock");
        Self::reap(&mut inner, &self.config);
        let slot = inner.streams.iter_mut().find(|s| s.id == stream_id)?;
        slot.sender = Some(tx);
        slot.died = None;
        let replay = slot
            .buffer
            .iter()
            .filter(|(_, e)| parse_event_id(&e.id).is_some_and(|(_, s)| s > seq))
            .map(|(_, e)| e.clone())
            .collect();
        drop(inner);
        Some((
            StreamHandle {
                stream_id,
                rx,
                registry: Arc::clone(self),
            },
            replay,
        ))
    }

    /// Store `data` on the designated stream (the oldest live one) and queue
    /// it for delivery, returning the allocated event id. `None` when the
    /// session has no live stream.
    ///
    /// A full channel kills the stream (explicitly, never a silent skip); the
    /// event stays in that stream's buffer, so the client's resumption GET
    /// replays it. The event is NOT re-sent on another stream — it belongs to
    /// the stream it was stored on (spec: no cross-stream replay).
    #[must_use]
    pub fn send(&self, event_type: &str, data: String) -> Option<String> {
        let mut inner = self.inner.lock().expect("stream registry lock");
        Self::reap(&mut inner, &self.config);
        let config = &self.config;
        let slot = inner
            .streams
            .iter_mut()
            .filter(|s| s.sender.is_some())
            .min_by_key(|s| s.opened)?;
        let event = slot.store(event_type, data, config);
        if let Some(sender) = &slot.sender {
            match sender.try_send(event.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                    // Kill the stream; the client resumes and replays from
                    // the retained buffer.
                    slot.sender = None;
                    slot.died = Some(Instant::now());
                }
            }
        }
        Some(event.id)
    }

    /// Whether any stream is currently live.
    #[must_use]
    pub fn has_live_stream(&self) -> bool {
        self.inner
            .lock()
            .expect("stream registry lock")
            .streams
            .iter()
            .any(|s| s.sender.is_some())
    }

    fn mark_dead(&self, stream_id: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(slot) = inner.streams.iter_mut().find(|s| s.id == stream_id) {
                slot.sender = None;
                slot.died = Some(Instant::now());
            }
        }
    }

    /// Drop dead streams whose retention window has passed.
    fn reap(inner: &mut Inner, config: &StreamConfig) {
        inner.streams.retain(|s| {
            s.sender.is_some() || s.died.is_none_or(|at| at.elapsed() < config.max_age)
        });
    }
}

/// Parse a `{stream_id}-{seq}` event id.
fn parse_event_id(id: &str) -> Option<(u64, u64)> {
    let (stream, seq) = id.split_once('-')?;
    Some((stream.parse().ok()?, seq.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Arc<StreamRegistry> {
        Arc::new(StreamRegistry::new(StreamConfig::default()))
    }

    #[tokio::test]
    async fn send_delivers_to_exactly_one_stream() {
        let reg = registry();
        let (mut a, _) = reg.open("connected", "sid".into());
        let (mut b, _) = reg.open("connected", "sid".into());

        let id = reg.send("message", "hello".into()).expect("live stream");
        // Designated = oldest live = stream a.
        let got = a.recv().await.expect("delivered");
        assert_eq!(got.id, id);
        assert_eq!(got.data, "hello");
        // Stream b must NOT receive it (spec MUST-NOT broadcast).
        assert!(
            tokio::time::timeout(Duration::from_millis(50), b.recv())
                .await
                .is_err(),
            "second stream must not receive the message"
        );
    }

    #[tokio::test]
    async fn event_ids_encode_stream_and_sequence() {
        let reg = registry();
        let (_a, prime) = reg.open("connected", "sid".into());
        assert_eq!(prime.id, "1-0");
        let id1 = reg.send("message", "x".into()).unwrap();
        let id2 = reg.send("message", "y".into()).unwrap();
        assert_eq!(id1, "1-1");
        assert_eq!(id2, "1-2");
    }

    #[tokio::test]
    async fn resume_replays_only_same_stream_events_after_cursor() {
        let reg = registry();
        let (a, _) = reg.open("connected", "sid".into());
        let id1 = reg.send("message", "one".into()).unwrap();
        let _id2 = reg.send("message", "two".into()).unwrap();
        drop(a); // stream dies

        // A different stream's traffic must not appear in stream 1's replay.
        let (_b, _) = reg.open("connected", "sid".into());
        let _ = reg.send("message", "other-stream".into()).unwrap();

        let (_a2, replay) = reg.resume(&id1).expect("resumable");
        assert_eq!(replay.len(), 1, "only events after the cursor: {replay:?}");
        assert_eq!(replay[0].data, "two");
    }

    #[tokio::test]
    async fn resumed_stream_keeps_designation() {
        let reg = registry();
        let (a, prime) = reg.open("connected", "sid".into());
        let (_b, _) = reg.open("connected", "sid".into());
        drop(a);

        // Stream 1 resumes; as the oldest it is designated again.
        let (mut a2, _) = reg.resume(&prime.id).expect("resumable");
        let id = reg.send("message", "after-resume".into()).unwrap();
        assert!(
            id.starts_with("1-"),
            "designated must still be stream 1: {id}"
        );
        assert_eq!(a2.recv().await.unwrap().data, "after-resume");
    }

    #[tokio::test]
    async fn overflow_kills_stream_and_replay_recovers() {
        let reg = Arc::new(StreamRegistry::new(StreamConfig {
            channel_capacity: 2,
            ..StreamConfig::default()
        }));
        let (a, prime) = reg.open("connected", "sid".into());

        // Fill the channel (capacity 2) without a reader, then overflow.
        let _ = reg.send("message", "m1".into()).unwrap();
        let _ = reg.send("message", "m2".into()).unwrap();
        let id3 = reg.send("message", "m3".into()).unwrap();
        assert!(!reg.has_live_stream(), "overflow must kill the stream");

        // The overflowed event is in the buffer: resume from the prime id
        // replays everything, including m3.
        drop(a);
        let (_a2, replay) = reg.resume(&prime.id).expect("resumable");
        assert_eq!(replay.last().map(|e| e.id.as_str()), Some(id3.as_str()));
        assert_eq!(replay.len(), 3);
    }

    #[tokio::test]
    async fn no_live_stream_returns_none() {
        let reg = registry();
        assert!(reg.send("message", "x".into()).is_none());
        let (a, _) = reg.open("connected", "sid".into());
        drop(a);
        assert!(
            reg.send("message", "x".into()).is_none(),
            "a dead stream is not a delivery target"
        );
    }

    #[tokio::test]
    async fn dead_stream_buffer_is_reaped_after_max_age() {
        let reg = Arc::new(StreamRegistry::new(StreamConfig {
            max_age: Duration::from_millis(10),
            ..StreamConfig::default()
        }));
        let (a, prime) = reg.open("connected", "sid".into());
        drop(a);
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Access triggers the reap; the stream identity is gone.
        let _ = reg.send("message", "x".into());
        assert!(
            reg.resume(&prime.id).is_none(),
            "expired dead stream must not be resumable"
        );
    }

    #[tokio::test]
    async fn handle_drop_marks_stream_dead() {
        let reg = registry();
        let (a, _) = reg.open("connected", "sid".into());
        assert!(reg.has_live_stream());
        drop(a);
        assert!(!reg.has_live_stream());
    }
}
