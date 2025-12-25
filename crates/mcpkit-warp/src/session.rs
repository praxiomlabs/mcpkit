//! Session management for MCP Warp integration.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Session manager for tracking MCP client sessions.
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<DashMap<String, SessionState>>,
    sse_channels: Arc<DashMap<String, broadcast::Sender<String>>>,
}

struct SessionState {
    last_seen: Instant,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Create a new session store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            sse_channels: Arc::new(DashMap::new()),
        }
    }

    /// Create a new session and return its ID.
    #[must_use]
    pub fn create(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Instant::now();
        self.sessions
            .insert(id.clone(), SessionState { last_seen: now });
        id
    }

    /// Update the last seen time for a session.
    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.last_seen = Instant::now();
        }
    }

    /// Check if a session exists.
    #[must_use]
    pub fn exists(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    /// Get or create an SSE channel for a session.
    #[must_use]
    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = self.create();
        let (tx, rx) = broadcast::channel(100);
        self.sse_channels.insert(id.clone(), tx);
        (id, rx)
    }

    /// Get a receiver for an existing SSE session.
    #[must_use]
    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.sse_channels.get(id).map(|tx| tx.subscribe())
    }

    /// Remove sessions older than the given duration.
    pub fn cleanup(&self, max_age: Duration) {
        let now = Instant::now();
        self.sessions
            .retain(|_, session| now.duration_since(session.last_seen) < max_age);
    }
}

/// Session manager trait for managing MCP sessions.
pub trait SessionManager {
    /// Create a new session.
    fn create_session(&self) -> String;

    /// Touch a session to update its last seen time.
    fn touch_session(&self, id: &str);

    /// Check if a session exists.
    fn session_exists(&self, id: &str) -> bool;
}

impl SessionManager for SessionStore {
    fn create_session(&self) -> String {
        self.create()
    }

    fn touch_session(&self, id: &str) {
        self.touch(id);
    }

    fn session_exists(&self, id: &str) -> bool {
        self.exists(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_store_creation() {
        let store = SessionStore::new();
        let id = store.create();

        assert!(!id.is_empty());
        assert!(store.exists(&id));
    }

    #[test]
    fn test_session_store_default() {
        let store = SessionStore::default();
        let id = store.create();

        assert!(store.exists(&id));
    }

    #[test]
    fn test_session_store_touch() {
        let store = SessionStore::new();
        let id = store.create();

        // Touch should not panic
        store.touch(&id);
        assert!(store.exists(&id));

        // Touching non-existent session should be no-op
        store.touch("non-existent");
    }

    #[test]
    fn test_session_store_exists() {
        let store = SessionStore::new();
        let id = store.create();

        assert!(store.exists(&id));
        assert!(!store.exists("non-existent"));
    }

    #[test]
    fn test_session_store_cleanup() {
        let store = SessionStore::new();
        let id = store.create();

        // Session should exist before cleanup with long max_age
        assert!(store.exists(&id));

        // Cleanup with 0 duration should remove all sessions
        store.cleanup(Duration::from_secs(0));
        assert!(!store.exists(&id));
    }

    #[tokio::test]
    async fn test_session_store_sse_channel() {
        let store = SessionStore::new();
        let (id, mut rx) = store.create_session();

        // Get the sender and send
        let tx = store.sse_channels.get(&id).unwrap();
        tx.send("test message".to_string()).unwrap();
        drop(tx);

        // Receive the message
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg, "test message");
    }

    #[test]
    fn test_session_store_get_receiver() {
        let store = SessionStore::new();
        let (id, _rx) = store.create_session();

        // Should be able to get another receiver
        let rx2 = store.get_receiver(&id);
        assert!(rx2.is_some());

        // Non-existent session should return None
        let rx3 = store.get_receiver("non-existent");
        assert!(rx3.is_none());
    }

    #[test]
    fn test_session_manager_trait() {
        let store = SessionStore::new();

        // Test via trait
        let id = SessionManager::create_session(&store);
        assert!(SessionManager::session_exists(&store, &id));

        SessionManager::touch_session(&store, &id);
        assert!(SessionManager::session_exists(&store, &id));

        assert!(!SessionManager::session_exists(&store, "non-existent"));
    }

    #[test]
    fn test_multiple_sessions() {
        let store = SessionStore::new();

        let id1 = store.create();
        let id2 = store.create();
        let id3 = store.create();

        assert!(store.exists(&id1));
        assert!(store.exists(&id2));
        assert!(store.exists(&id3));

        // All IDs should be unique
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }
}
