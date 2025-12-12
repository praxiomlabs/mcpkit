//! Session management for MCP connections.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Default session timeout duration (30 minutes).
pub const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Default SSE broadcast channel capacity.
pub const DEFAULT_SSE_CAPACITY: usize = 100;

/// Represents an active MCP session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// When the session was created.
    pub created_at: Instant,
    /// When the session was last accessed.
    pub last_accessed: Instant,
}

impl Session {
    /// Create a new session.
    #[must_use]
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            last_accessed: now,
        }
    }

    /// Check if the session has expired.
    #[must_use]
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_accessed.elapsed() > timeout
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages MCP sessions with automatic cleanup.
#[derive(Debug, Clone)]
pub struct SessionManager {
    sessions: Arc<DashMap<String, Session>>,
    timeout: Duration,
}

impl SessionManager {
    /// Create a new session manager with default timeout.
    #[must_use]
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_SESSION_TIMEOUT)
    }

    /// Create a new session manager with a custom timeout.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            timeout,
        }
    }

    /// Create a new session and return its ID.
    #[must_use]
    pub fn create(&self) -> String {
        let session = Session::new();
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        id
    }

    /// Get a session by ID, updating its last accessed time.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.get_mut(id).map(|mut session| {
            session.last_accessed = Instant::now();
            session.clone()
        })
    }

    /// Check if a session exists.
    #[must_use]
    pub fn exists(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    /// Update the last accessed time for a session.
    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.last_accessed = Instant::now();
        }
    }

    /// Remove expired sessions.
    pub fn cleanup(&self) {
        self.sessions
            .retain(|_, session| !session.is_expired(self.timeout));
    }

    /// Get the number of active sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Check if there are no active sessions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages Server-Sent Events sessions with broadcast channels.
#[derive(Debug, Clone)]
pub struct SessionStore {
    senders: Arc<DashMap<String, broadcast::Sender<String>>>,
    capacity: usize,
}

impl SessionStore {
    /// Create a new SSE session store.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SSE_CAPACITY)
    }

    /// Create a new SSE session store with custom channel capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            senders: Arc::new(DashMap::new()),
            capacity,
        }
    }

    /// Create a new SSE session.
    #[must_use]
    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(self.capacity);
        self.senders.insert(id.clone(), tx);
        (id, rx)
    }

    /// Get a receiver for an existing session.
    #[must_use]
    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.senders.get(id).map(|tx| tx.subscribe())
    }

    /// Send a message to a specific session.
    pub fn send(&self, id: &str, message: String) -> Result<(), String> {
        if let Some(tx) = self.senders.get(id) {
            tx.send(message)
                .map(|_| ())
                .map_err(|e| format!("Failed to send: {e}"))
        } else {
            Err(format!("Session not found: {id}"))
        }
    }

    /// Broadcast a message to all sessions.
    pub fn broadcast(&self, message: String) {
        for tx in self.senders.iter() {
            let _ = tx.send(message.clone());
        }
    }

    /// Remove a session.
    pub fn remove(&self, id: &str) {
        self.senders.remove(id);
    }

    /// Get the number of active SSE sessions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.senders.len()
    }

    /// Check if there are no active SSE sessions.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.senders.is_empty()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = Session::new();
        assert!(!session.id.is_empty());
        assert!(!session.is_expired(Duration::from_secs(60)));
    }

    #[test]
    fn test_session_expiry() {
        let mut session = Session::new();
        // Simulate old access time
        session.last_accessed = Instant::now()
            .checked_sub(Duration::from_secs(120))
            .unwrap();
        assert!(session.is_expired(Duration::from_secs(60)));
    }

    #[test]
    fn test_session_manager() {
        let manager = SessionManager::new();

        let id = manager.create();
        assert!(manager.exists(&id));
        assert_eq!(manager.len(), 1);

        let session = manager.get(&id);
        assert!(session.is_some());

        manager.cleanup();
        assert!(manager.exists(&id)); // Should still exist
    }

    #[test]
    fn test_session_store() {
        let store = SessionStore::new();

        let (id, _rx) = store.create_session();
        assert_eq!(store.len(), 1);

        let rx2 = store.get_receiver(&id);
        assert!(rx2.is_some());

        store.remove(&id);
        assert!(store.is_empty());
    }
}
