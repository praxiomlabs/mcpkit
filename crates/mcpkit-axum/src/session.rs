//! Session management for MCP HTTP connections.

use dashmap::DashMap;
use mcpkit_core::auth::{SessionBindingError, VerifiedUser, check_session_binding};
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol_version::ProtocolVersion;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::sync::broadcast;

/// A single MCP session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// When the session was created.
    pub created_at: Instant,
    /// When the session was last active.
    pub last_active: Instant,
    /// Whether the session has been initialized.
    pub initialized: bool,
    /// Client capabilities from initialization.
    pub client_capabilities: Option<ClientCapabilities>,
    /// Protocol version negotiated during initialization.
    pub protocol_version: Option<ProtocolVersion>,
    /// The verified user this session is bound to, if any. Once bound, the
    /// session may only be used by the same user (see [`SessionBindingError`]).
    pub user: Option<VerifiedUser>,
}

impl Session {
    /// Create a new anonymous session.
    #[must_use]
    pub fn new(id: String) -> Self {
        Self::with_user(id, None)
    }

    /// Create a new session bound to an optional verified user.
    #[must_use]
    pub fn with_user(id: String, user: Option<VerifiedUser>) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_active: now,
            initialized: false,
            client_capabilities: None,
            protocol_version: None,
            user,
        }
    }

    /// Check if the session has expired.
    #[must_use]
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_active.elapsed() >= timeout
    }

    /// Check whether the session should be reaped, given idle and
    /// initialization timeouts.
    ///
    /// A session is reaped when it has been idle longer than `idle_timeout`, or
    /// when it has not completed initialization within `init_timeout` of being
    /// created. The latter bounds resources held by half-open sessions that are
    /// created but never initialized.
    #[must_use]
    pub fn is_reapable(&self, idle_timeout: Duration, init_timeout: Duration) -> bool {
        self.is_expired(idle_timeout)
            || (!self.initialized && self.created_at.elapsed() >= init_timeout)
    }

    /// Mark the session as active.
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    /// Mark the session as initialized, recording the negotiated protocol
    /// version and the client's capabilities.
    pub fn mark_initialized(
        &mut self,
        protocol_version: ProtocolVersion,
        capabilities: Option<ClientCapabilities>,
    ) {
        self.initialized = true;
        self.protocol_version = Some(protocol_version);
        self.client_capabilities = capabilities;
    }
}

/// A stored SSE event for replay support.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    /// The event ID (globally unique within the session stream).
    pub id: String,
    /// The event type (e.g., "message", "connected").
    pub event_type: String,
    /// The event data.
    pub data: String,
    /// When the event was stored.
    pub stored_at: Instant,
}

impl StoredEvent {
    /// Create a new stored event.
    #[must_use]
    pub fn new(id: String, event_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            id,
            event_type: event_type.into(),
            data: data.into(),
            stored_at: Instant::now(),
        }
    }
}

/// Configuration for event store retention.
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    /// Maximum number of events to retain per stream.
    pub max_events: usize,
    /// Maximum age of events to retain.
    pub max_age: Duration,
}

impl Default for EventStoreConfig {
    fn default() -> Self {
        Self {
            max_events: 1000,
            max_age: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl EventStoreConfig {
    /// Create a new event store configuration.
    #[must_use]
    pub const fn new(max_events: usize, max_age: Duration) -> Self {
        Self {
            max_events,
            max_age,
        }
    }

    /// Set the maximum number of events to retain.
    #[must_use]
    pub const fn with_max_events(mut self, max_events: usize) -> Self {
        self.max_events = max_events;
        self
    }

    /// Set the maximum age of events to retain.
    #[must_use]
    pub const fn with_max_age(mut self, max_age: Duration) -> Self {
        self.max_age = max_age;
        self
    }
}

/// Event store for SSE message resumability.
///
/// Per the MCP Streamable HTTP specification, servers MAY store events
/// with IDs to support client reconnection with `Last-Event-ID`.
///
/// # Example
///
/// ```rust
/// use mcpkit_axum::{EventStore, EventStoreConfig};
/// use std::time::Duration;
///
/// let config = EventStoreConfig::new(500, Duration::from_secs(120));
/// let store = EventStore::new(config);
///
/// // Store an event
/// store.store("evt-001", "message", r#"{"jsonrpc":"2.0",...}"#);
///
/// // Get events after a specific ID for replay (async)
/// // let events = store.get_events_after("evt-000").await;
/// ```
#[derive(Debug)]
pub struct EventStore {
    events: RwLock<VecDeque<StoredEvent>>,
    config: EventStoreConfig,
    next_id: AtomicU64,
}

impl EventStore {
    /// Create a new event store with the given configuration.
    #[must_use]
    pub fn new(config: EventStoreConfig) -> Self {
        Self {
            events: RwLock::new(VecDeque::with_capacity(config.max_events)),
            config,
            next_id: AtomicU64::new(1),
        }
    }

    /// Create a new event store with default configuration.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(EventStoreConfig::default())
    }

    /// Generate the next event ID.
    #[must_use]
    pub fn next_event_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        format!("evt-{id}")
    }

    /// Store an event with automatic ID generation.
    ///
    /// Returns the generated event ID.
    pub fn store_auto_id(&self, event_type: impl Into<String>, data: impl Into<String>) -> String {
        let id = self.next_event_id();
        self.store(id.clone(), event_type, data);
        id
    }

    /// Store an event with a specific ID.
    pub fn store(
        &self,
        id: impl Into<String>,
        event_type: impl Into<String>,
        data: impl Into<String>,
    ) {
        let event = StoredEvent::new(id.into(), event_type, data);

        // Use blocking write since we can't use async in this sync method
        // In production, consider using parking_lot::RwLock for better sync performance
        let mut events = futures::executor::block_on(self.events.write());

        // Add the new event
        events.push_back(event);

        // Enforce max_events limit
        while events.len() > self.config.max_events {
            events.pop_front();
        }

        // Enforce max_age limit
        let now = Instant::now();
        while let Some(front) = events.front() {
            if now.duration_since(front.stored_at) > self.config.max_age {
                events.pop_front();
            } else {
                break;
            }
        }
    }

    /// Store an event asynchronously.
    pub async fn store_async(
        &self,
        id: impl Into<String>,
        event_type: impl Into<String>,
        data: impl Into<String>,
    ) {
        let event = StoredEvent::new(id.into(), event_type, data);
        let mut events = self.events.write().await;

        events.push_back(event);

        // Enforce limits
        while events.len() > self.config.max_events {
            events.pop_front();
        }

        let now = Instant::now();
        while let Some(front) = events.front() {
            if now.duration_since(front.stored_at) > self.config.max_age {
                events.pop_front();
            } else {
                break;
            }
        }
    }

    /// Get all events after the specified event ID.
    ///
    /// Used for replaying events when a client reconnects with `Last-Event-ID`.
    /// Returns events in chronological order.
    pub async fn get_events_after(&self, last_event_id: &str) -> Vec<StoredEvent> {
        let events = self.events.read().await;

        // Find the index of the last event ID
        // Start from the next event after last_event_id, or 0 if not found
        let start_idx = events
            .iter()
            .position(|e| e.id == last_event_id)
            .map_or(0, |i| i + 1);

        events.iter().skip(start_idx).cloned().collect()
    }

    /// Get all stored events.
    pub async fn get_all_events(&self) -> Vec<StoredEvent> {
        let events = self.events.read().await;
        events.iter().cloned().collect()
    }

    /// Get the number of stored events.
    pub async fn len(&self) -> usize {
        self.events.read().await.len()
    }

    /// Check if the store is empty.
    pub async fn is_empty(&self) -> bool {
        self.events.read().await.is_empty()
    }

    /// Clear all stored events.
    pub async fn clear(&self) {
        self.events.write().await.clear();
    }

    /// Clean up expired events.
    pub async fn cleanup_expired(&self) {
        let mut events = self.events.write().await;
        let now = Instant::now();
        while let Some(front) = events.front() {
            if now.duration_since(front.stored_at) > self.config.max_age {
                events.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Session manager for SSE connections.
///
/// Manages broadcast channels for pushing messages to SSE clients,
/// with optional event storage for message resumability.
#[derive(Debug)]
pub struct SessionManager {
    sessions: DashMap<String, broadcast::Sender<String>>,
    /// Event stores for each session (for SSE resumability).
    event_stores: DashMap<String, Arc<EventStore>>,
    /// Configuration for event stores.
    event_store_config: EventStoreConfig,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Create a new session manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            event_stores: DashMap::new(),
            event_store_config: EventStoreConfig::default(),
        }
    }

    /// Create a new session manager with custom event store configuration.
    #[must_use]
    pub fn with_event_store_config(config: EventStoreConfig) -> Self {
        Self {
            sessions: DashMap::new(),
            event_stores: DashMap::new(),
            event_store_config: config,
        }
    }

    /// Create a new session and return its ID and receiver.
    #[must_use]
    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(100);
        self.sessions.insert(id.clone(), tx);

        // Create an event store for this session
        let event_store = Arc::new(EventStore::new(self.event_store_config.clone()));
        self.event_stores.insert(id.clone(), event_store);

        (id, rx)
    }

    /// Get a receiver for an existing session.
    #[must_use]
    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.sessions.get(id).map(|tx| tx.subscribe())
    }

    /// Get the event store for a session.
    #[must_use]
    pub fn get_event_store(&self, id: &str) -> Option<Arc<EventStore>> {
        self.event_stores.get(id).map(|store| Arc::clone(&store))
    }

    /// Send a message to a specific session.
    ///
    /// Returns `true` if the message was sent, `false` if the session doesn't exist.
    #[must_use]
    pub fn send_to_session(&self, id: &str, message: String) -> bool {
        if let Some(tx) = self.sessions.get(id) {
            // Ignore send errors (no receivers)
            let _ = tx.send(message);
            true
        } else {
            false
        }
    }

    /// Send a message to a specific session and store it for replay.
    ///
    /// This method stores the event in the event store before sending,
    /// enabling message resumability for clients that reconnect.
    ///
    /// Returns the event ID if the message was sent and stored, `None` if the session doesn't exist.
    #[must_use]
    pub fn send_to_session_with_storage(
        &self,
        session_id: &str,
        event_type: impl Into<String>,
        message: String,
    ) -> Option<String> {
        if let Some(tx) = self.sessions.get(session_id) {
            // Store the event first
            let event_id = if let Some(store) = self.event_stores.get(session_id) {
                store.store_auto_id(event_type, message.clone())
            } else {
                // Create a store if it doesn't exist (shouldn't happen normally)
                let store = Arc::new(EventStore::new(self.event_store_config.clone()));
                let event_id = store.store_auto_id(event_type, message.clone());
                self.event_stores.insert(session_id.to_string(), store);
                event_id
            };

            // Send the message
            let _ = tx.send(message);
            Some(event_id)
        } else {
            None
        }
    }

    /// Broadcast a message to all sessions.
    pub fn broadcast(&self, message: String) {
        for entry in &self.sessions {
            let _ = entry.value().send(message.clone());
        }
    }

    /// Broadcast a message to all sessions with storage.
    ///
    /// Stores the event in each session's event store for resumability.
    pub fn broadcast_with_storage(&self, event_type: impl Into<String> + Clone, message: String) {
        for entry in &self.sessions {
            let session_id = entry.key();

            // Store in event store
            if let Some(store) = self.event_stores.get(session_id) {
                store.store_auto_id(event_type.clone(), message.clone());
            }

            // Send
            let _ = entry.value().send(message.clone());
        }
    }

    /// Remove a session.
    pub fn remove_session(&self, id: &str) {
        self.sessions.remove(id);
        self.event_stores.remove(id);
    }

    /// Get the number of active sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Clean up expired events across all sessions.
    pub async fn cleanup_expired_events(&self) {
        for entry in &self.event_stores {
            entry.value().cleanup_expired().await;
        }
    }

    /// Get events after the specified event ID for replay.
    ///
    /// Used when a client reconnects with `Last-Event-ID`.
    pub async fn get_events_for_replay(
        &self,
        session_id: &str,
        last_event_id: &str,
    ) -> Option<Vec<StoredEvent>> {
        if let Some(store) = self.event_stores.get(session_id) {
            Some(store.get_events_after(last_event_id).await)
        } else {
            None
        }
    }
}

/// Default timeout after which a session created but never initialized is
/// reaped.
pub const DEFAULT_INIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Thread-safe session store with automatic cleanup.
///
/// Stores session metadata for HTTP request handling.
#[derive(Debug)]
pub struct SessionStore {
    sessions: DashMap<String, Session>,
    timeout: Duration,
    init_timeout: Duration,
}

impl SessionStore {
    /// Create a new session store with the given idle timeout.
    ///
    /// The initialization timeout defaults to [`DEFAULT_INIT_TIMEOUT`]; use
    /// [`Self::with_init_timeout`] to change it.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            timeout,
            init_timeout: DEFAULT_INIT_TIMEOUT,
        }
    }

    /// Create a new session store with a default 1-hour idle timeout.
    #[must_use]
    pub fn with_default_timeout() -> Self {
        Self::new(Duration::from_secs(3600))
    }

    /// Set the timeout after which a session that never completed
    /// initialization is reaped.
    #[must_use]
    pub const fn with_init_timeout(mut self, init_timeout: Duration) -> Self {
        self.init_timeout = init_timeout;
        self
    }

    /// Create a new session and return its ID.
    ///
    /// Expired sessions are reaped first, so the store stays bounded without a
    /// background cleanup task.
    #[must_use]
    pub fn create(&self) -> String {
        self.create_for_user(None)
    }

    /// Create a new session bound to an optional verified user.
    ///
    /// A session created with `Some(user)` may then only be used by that same
    /// user (see [`SessionStore::get_verified`]).
    #[must_use]
    pub fn create_for_user(&self, user: Option<VerifiedUser>) -> String {
        self.cleanup_expired();
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .insert(id.clone(), Session::with_user(id.clone(), user));
        id
    }

    /// Get a session by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.get(id).map(|r| r.clone())
    }

    /// Get a session by ID, enforcing its user binding against the identity
    /// presenting this request.
    ///
    /// Returns `Ok(None)` if no such session exists, `Ok(Some(session))` if the
    /// binding holds, or `Err` if the presenting identity does not match the
    /// session's bound user.
    pub fn get_verified(
        &self,
        id: &str,
        presenting: Option<&VerifiedUser>,
    ) -> Result<Option<Session>, SessionBindingError> {
        let Some(session) = self.get(id) else {
            return Ok(None);
        };
        check_session_binding(session.user.as_ref(), presenting)?;
        Ok(Some(session))
    }

    /// Touch a session to update its last active time.
    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.touch();
        }
    }

    /// Touch a session, enforcing its user binding first.
    ///
    /// Returns `Ok(true)` if the session existed and was touched, `Ok(false)` if
    /// it did not exist, or `Err` on a binding violation.
    pub fn touch_verified(
        &self,
        id: &str,
        presenting: Option<&VerifiedUser>,
    ) -> Result<bool, SessionBindingError> {
        let Some(mut session) = self.sessions.get_mut(id) else {
            return Ok(false);
        };
        check_session_binding(session.user.as_ref(), presenting)?;
        session.touch();
        Ok(true)
    }

    /// Update a session.
    pub fn update<F>(&self, id: &str, f: F)
    where
        F: FnOnce(&mut Session),
    {
        if let Some(mut session) = self.sessions.get_mut(id) {
            f(&mut session);
        }
    }

    /// Remove expired sessions (idle past the timeout, or never initialized
    /// past the initialization timeout).
    pub fn cleanup_expired(&self) {
        let timeout = self.timeout;
        let init_timeout = self.init_timeout;
        self.sessions
            .retain(|_, s| !s.is_reapable(timeout, init_timeout));
    }

    /// Remove a session.
    #[must_use]
    pub fn remove(&self, id: &str) -> Option<Session> {
        self.sessions.remove(id).map(|(_, s)| s)
    }

    /// Get the number of active sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Start a background task to periodically clean up expired sessions.
    pub fn start_cleanup_task(self: &Arc<Self>, interval: Duration) {
        let store = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                store.cleanup_expired();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new("test-123".to_string());
        assert_eq!(session.id, "test-123");
        assert!(!session.initialized);
        assert!(session.client_capabilities.is_none());
        assert!(session.user.is_none());
    }

    #[test]
    fn user_bound_session_enforces_identity() {
        let store = SessionStore::new(Duration::from_secs(60));
        let alice = VerifiedUser::new("alice").issuer("https://idp");
        let bob = VerifiedUser::new("bob").issuer("https://idp");

        let id = store.create_for_user(Some(alice.clone()));

        // Same user: ok.
        assert_eq!(store.touch_verified(&id, Some(&alice)), Ok(true));
        assert!(store.get_verified(&id, Some(&alice)).unwrap().is_some());
        // Different user: mismatch.
        assert_eq!(
            store.touch_verified(&id, Some(&bob)),
            Err(SessionBindingError::IdentityMismatch)
        );
        // Missing identity on a bound session: rejected.
        assert_eq!(
            store.get_verified(&id, None).unwrap_err(),
            SessionBindingError::IdentityRequired
        );

        // Anonymous session: anonymous ok, but a verified identity is rejected
        // (no silent upgrade).
        let anon = store.create();
        assert_eq!(store.touch_verified(&anon, None), Ok(true));
        assert_eq!(
            store.touch_verified(&anon, Some(&alice)),
            Err(SessionBindingError::UnexpectedIdentity)
        );

        // Unknown session id: not found, not an error.
        assert_eq!(store.touch_verified("nope", Some(&alice)), Ok(false));
        assert!(store.get_verified("nope", Some(&alice)).unwrap().is_none());
    }

    #[test]
    fn test_session_expiry() -> Result<(), Box<dyn std::error::Error>> {
        let mut session = Session::new("test".to_string());
        assert!(!session.is_expired(Duration::from_secs(60)));

        // Simulate old session by setting last_active in the past
        session.last_active = Instant::now()
            .checked_sub(Duration::from_secs(120))
            .ok_or("Failed to subtract duration")?;
        assert!(session.is_expired(Duration::from_secs(60)));
        Ok(())
    }

    #[test]
    fn test_session_store() {
        let store = SessionStore::new(Duration::from_secs(60));
        let id = store.create();

        assert!(store.get(&id).is_some());
        store.touch(&id);

        let _ = store.remove(&id);
        assert!(store.get(&id).is_none());
    }

    #[test]
    fn uninitialized_session_is_reapable_after_init_timeout()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut session = Session::new("s".to_string());
        let idle = Duration::from_secs(3600);
        let init = Duration::from_secs(30);

        // A fresh, uninitialized session is not yet reapable.
        assert!(!session.is_reapable(idle, init));

        // Once it has existed longer than the init timeout without
        // initializing, it becomes reapable.
        session.created_at = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .ok_or("Failed to subtract duration")?;
        assert!(session.is_reapable(idle, init));

        // After initialization, the init timeout no longer applies.
        session.mark_initialized(ProtocolVersion::LATEST, None);
        assert!(!session.is_reapable(idle, init));
        Ok(())
    }

    #[test]
    fn create_reaps_uninitialized_sessions_past_init_timeout() {
        let store = SessionStore::new(Duration::from_secs(3600)).with_init_timeout(Duration::ZERO);
        let id = store.create();

        // A zero init timeout makes the uninitialized session reapable, so the
        // next create() sweeps it away.
        let _other = store.create();
        assert!(store.get(&id).is_none());
    }

    #[test]
    fn create_keeps_initialized_sessions() {
        let store = SessionStore::new(Duration::from_secs(3600)).with_init_timeout(Duration::ZERO);
        let id = store.create();
        store.update(&id, |s| s.mark_initialized(ProtocolVersion::LATEST, None));

        // An initialized session is not subject to the init timeout and is well
        // within the idle timeout, so it survives create-time reaping.
        let _other = store.create();
        assert!(store.get(&id).is_some());
    }

    #[tokio::test]
    async fn test_session_manager() -> Result<(), Box<dyn std::error::Error>> {
        let manager = SessionManager::new();
        let (id, mut rx) = manager.create_session();

        // Send a message
        assert!(manager.send_to_session(&id, "test message".to_string()));

        // Receive the message
        let msg = rx.recv().await?;
        assert_eq!(msg, "test message");

        // Remove session
        manager.remove_session(&id);
        assert!(!manager.send_to_session(&id, "another".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_event_store_creation() {
        let store = EventStore::with_defaults();
        assert!(store.is_empty().await);
        assert_eq!(store.len().await, 0);
    }

    #[tokio::test]
    async fn test_event_store_store_and_retrieve() {
        let store = EventStore::with_defaults();

        store.store_async("evt-1", "message", "data1").await;
        store.store_async("evt-2", "message", "data2").await;
        store.store_async("evt-3", "message", "data3").await;

        assert_eq!(store.len().await, 3);

        let all_events = store.get_all_events().await;
        assert_eq!(all_events.len(), 3);
        assert_eq!(all_events[0].id, "evt-1");
        assert_eq!(all_events[1].id, "evt-2");
        assert_eq!(all_events[2].id, "evt-3");
    }

    #[tokio::test]
    async fn test_event_store_get_events_after() {
        let store = EventStore::with_defaults();

        store.store_async("evt-1", "message", "data1").await;
        store.store_async("evt-2", "message", "data2").await;
        store.store_async("evt-3", "message", "data3").await;

        // Get events after evt-1
        let events = store.get_events_after("evt-1").await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "evt-2");
        assert_eq!(events[1].id, "evt-3");

        // Get events after evt-2
        let events = store.get_events_after("evt-2").await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "evt-3");

        // Get events after evt-3 (should be empty)
        let events = store.get_events_after("evt-3").await;
        assert_eq!(events.len(), 0);

        // Get events after unknown ID (should return all)
        let events = store.get_events_after("unknown").await;
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_event_store_auto_id() {
        let store = EventStore::with_defaults();

        let id1 = store.store_auto_id("message", "data1");
        let id2 = store.store_auto_id("message", "data2");

        assert!(id1.starts_with("evt-"));
        assert!(id2.starts_with("evt-"));
        assert_ne!(id1, id2);

        assert_eq!(store.len().await, 2);
    }

    #[tokio::test]
    async fn test_event_store_max_events_limit() {
        let config = EventStoreConfig::new(3, Duration::from_secs(300));
        let store = EventStore::new(config);

        store.store_async("evt-1", "message", "data1").await;
        store.store_async("evt-2", "message", "data2").await;
        store.store_async("evt-3", "message", "data3").await;
        store.store_async("evt-4", "message", "data4").await;

        // Should only have 3 events (oldest removed)
        assert_eq!(store.len().await, 3);

        let events = store.get_all_events().await;
        assert_eq!(events[0].id, "evt-2"); // evt-1 was evicted
        assert_eq!(events[1].id, "evt-3");
        assert_eq!(events[2].id, "evt-4");
    }

    #[tokio::test]
    async fn test_event_store_clear() {
        let store = EventStore::with_defaults();

        store.store_async("evt-1", "message", "data1").await;
        store.store_async("evt-2", "message", "data2").await;

        assert_eq!(store.len().await, 2);

        store.clear().await;

        assert!(store.is_empty().await);
        assert_eq!(store.len().await, 0);
    }

    #[tokio::test]
    async fn test_session_manager_with_event_store() -> Result<(), Box<dyn std::error::Error>> {
        let manager = SessionManager::new();
        let (id, _rx) = manager.create_session();

        // Event store should be created automatically
        let store = manager.get_event_store(&id);
        assert!(store.is_some());

        let store = store.ok_or("Event store not found")?;
        assert!(store.is_empty().await);
        Ok(())
    }

    #[tokio::test]
    async fn test_session_manager_send_with_storage() -> Result<(), Box<dyn std::error::Error>> {
        let manager = SessionManager::new();
        let (id, mut rx) = manager.create_session();

        // Send with storage
        let event_id =
            manager.send_to_session_with_storage(&id, "message", "test data".to_string());
        assert!(event_id.is_some());

        // Verify message was received
        let msg = rx.recv().await?;
        assert_eq!(msg, "test data");

        // Verify event was stored
        let store = manager
            .get_event_store(&id)
            .ok_or("Event store not found")?;
        assert_eq!(store.len().await, 1);

        let events = store.get_all_events().await;
        assert_eq!(events[0].data, "test data");
        assert_eq!(events[0].event_type, "message");
        Ok(())
    }

    #[tokio::test]
    async fn test_session_manager_replay() -> Result<(), Box<dyn std::error::Error>> {
        let manager = SessionManager::new();
        let (id, _rx) = manager.create_session();

        // Send multiple messages with storage
        let _ = manager.send_to_session_with_storage(&id, "message", "msg1".to_string());
        let evt2 = manager.send_to_session_with_storage(&id, "message", "msg2".to_string());
        let _ = manager.send_to_session_with_storage(&id, "message", "msg3".to_string());

        // Simulate reconnection - get events after evt2
        let events = manager
            .get_events_for_replay(&id, &evt2.ok_or("Failed to get event ID")?)
            .await
            .ok_or("Failed to get events for replay")?;

        // Should only get msg3
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "msg3");
        Ok(())
    }

    #[test]
    fn test_event_store_config() {
        let config = EventStoreConfig::default();
        assert_eq!(config.max_events, 1000);
        assert_eq!(config.max_age, Duration::from_secs(300));

        let config = EventStoreConfig::new(500, Duration::from_secs(120))
            .with_max_events(600)
            .with_max_age(Duration::from_secs(180));

        assert_eq!(config.max_events, 600);
        assert_eq!(config.max_age, Duration::from_secs(180));
    }

    #[test]
    fn test_stored_event() {
        let event = StoredEvent::new("evt-123".to_string(), "message", "test data");
        assert_eq!(event.id, "evt-123");
        assert_eq!(event.event_type, "message");
        assert_eq!(event.data, "test data");
    }
}
