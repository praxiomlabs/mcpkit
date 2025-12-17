//! Session management for MCP HTTP connections.

use dashmap::DashMap;
use mcpkit_core::capability::ClientCapabilities;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::sync::RwLock;

/// Default session timeout duration (1 hour).
pub const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(3600);

/// Default SSE broadcast channel capacity.
pub const DEFAULT_SSE_CAPACITY: usize = 100;

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
}

impl Session {
    /// Create a new session.
    #[must_use]
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_active: now,
            initialized: false,
            client_capabilities: None,
        }
    }

    /// Check if the session has expired.
    #[must_use]
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_active.elapsed() >= timeout
    }

    /// Mark the session as active.
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    /// Mark the session as initialized.
    pub fn mark_initialized(&mut self, capabilities: Option<ClientCapabilities>) {
        self.initialized = true;
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
        Self { max_events, max_age }
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
    pub fn store(&self, id: impl Into<String>, event_type: impl Into<String>, data: impl Into<String>) {
        let event = StoredEvent::new(id.into(), event_type, data);

        // Use blocking write since we can't use async in this sync method
        let mut events = futures::executor::block_on(self.events.write());

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
    pub async fn store_async(&self, id: impl Into<String>, event_type: impl Into<String>, data: impl Into<String>) {
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
    pub async fn get_events_after(&self, last_event_id: &str) -> Vec<StoredEvent> {
        let events = self.events.read().await;

        let start_idx = events
            .iter()
            .position(|e| e.id == last_event_id)
            .map(|i| i + 1)
            .unwrap_or(0);

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
    capacity: usize,
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
        Self::with_capacity(DEFAULT_SSE_CAPACITY)
    }

    /// Create a new session manager with custom channel capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            sessions: DashMap::new(),
            event_stores: DashMap::new(),
            event_store_config: EventStoreConfig::default(),
            capacity,
        }
    }

    /// Create a new session manager with custom event store configuration.
    #[must_use]
    pub fn with_event_store_config(config: EventStoreConfig) -> Self {
        Self {
            sessions: DashMap::new(),
            event_stores: DashMap::new(),
            event_store_config: config,
            capacity: DEFAULT_SSE_CAPACITY,
        }
    }

    /// Create a new session and return its ID and receiver.
    #[must_use]
    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(self.capacity);
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
            let _ = tx.send(message);
            true
        } else {
            false
        }
    }

    /// Send a message to a specific session and store it for replay.
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
            let event_id = if let Some(store) = self.event_stores.get(session_id) {
                store.store_auto_id(event_type, message.clone())
            } else {
                let store = Arc::new(EventStore::new(self.event_store_config.clone()));
                let event_id = store.store_auto_id(event_type, message.clone());
                self.event_stores.insert(session_id.to_string(), store);
                event_id
            };

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
    pub fn broadcast_with_storage(&self, event_type: impl Into<String> + Clone, message: String) {
        for entry in &self.sessions {
            let session_id = entry.key();

            if let Some(store) = self.event_stores.get(session_id) {
                store.store_auto_id(event_type.clone(), message.clone());
            }

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

/// Thread-safe session store with automatic cleanup.
///
/// Stores session metadata for HTTP request handling.
#[derive(Debug)]
pub struct SessionStore {
    sessions: DashMap<String, Session>,
    timeout: Duration,
}

impl SessionStore {
    /// Create a new session store with the given timeout.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            sessions: DashMap::new(),
            timeout,
        }
    }

    /// Create a new session store with a default 1-hour timeout.
    #[must_use]
    pub fn with_default_timeout() -> Self {
        Self::new(DEFAULT_SESSION_TIMEOUT)
    }

    /// Create a new session and return its ID.
    #[must_use]
    pub fn create(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions.insert(id.clone(), Session::new(id.clone()));
        id
    }

    /// Get a session by ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.get(id).map(|r| r.clone())
    }

    /// Touch a session to update its last active time.
    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.touch();
        }
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

    /// Remove expired sessions.
    pub fn cleanup_expired(&self) {
        let timeout = self.timeout;
        self.sessions.retain(|_, s| !s.is_expired(timeout));
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
    }

    #[test]
    fn test_session_expiry() {
        let mut session = Session::new("test".to_string());
        assert!(!session.is_expired(Duration::from_secs(60)));

        session.last_active = Instant::now()
            .checked_sub(Duration::from_secs(120))
            .unwrap();
        assert!(session.is_expired(Duration::from_secs(60)));
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

    #[tokio::test]
    async fn test_session_manager() {
        let manager = SessionManager::new();
        let (id, mut rx) = manager.create_session();

        assert!(manager.send_to_session(&id, "test message".to_string()));

        let msg = rx.recv().await.expect("Should receive message");
        assert_eq!(msg, "test message");

        manager.remove_session(&id);
        assert!(!manager.send_to_session(&id, "another".to_string()));
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

        let events = store.get_events_after("evt-1").await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "evt-2");
        assert_eq!(events[1].id, "evt-3");

        let events = store.get_events_after("evt-3").await;
        assert_eq!(events.len(), 0);

        let events = store.get_events_after("unknown").await;
        assert_eq!(events.len(), 3);
    }

    #[tokio::test]
    async fn test_session_manager_with_event_store() {
        let manager = SessionManager::new();
        let (id, _rx) = manager.create_session();

        let store = manager.get_event_store(&id);
        assert!(store.is_some());

        let store = store.unwrap();
        assert!(store.is_empty().await);
    }

    #[tokio::test]
    async fn test_session_manager_send_with_storage() {
        let manager = SessionManager::new();
        let (id, mut rx) = manager.create_session();

        let event_id = manager.send_to_session_with_storage(&id, "message", "test data".to_string());
        assert!(event_id.is_some());

        let msg = rx.recv().await.expect("Should receive message");
        assert_eq!(msg, "test data");

        let store = manager.get_event_store(&id).unwrap();
        assert_eq!(store.len().await, 1);

        let events = store.get_all_events().await;
        assert_eq!(events[0].data, "test data");
        assert_eq!(events[0].event_type, "message");
    }

    #[tokio::test]
    async fn test_session_manager_replay() {
        let manager = SessionManager::new();
        let (id, _rx) = manager.create_session();

        let _ = manager.send_to_session_with_storage(&id, "message", "msg1".to_string());
        let evt2 = manager.send_to_session_with_storage(&id, "message", "msg2".to_string());
        let _ = manager.send_to_session_with_storage(&id, "message", "msg3".to_string());

        let events = manager
            .get_events_for_replay(&id, &evt2.unwrap())
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "msg3");
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
