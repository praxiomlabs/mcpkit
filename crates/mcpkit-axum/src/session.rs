//! Session management for MCP HTTP connections.

use dashmap::DashMap;
use mcpkit_core::capability::ClientCapabilities;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

/// Session manager for SSE connections.
///
/// Manages broadcast channels for pushing messages to SSE clients.
pub struct SessionManager {
    sessions: DashMap<String, broadcast::Sender<String>>,
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
        }
    }

    /// Create a new session and return its ID and receiver.
    #[must_use]
    pub fn create_session(&self) -> (String, broadcast::Receiver<String>) {
        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(100);
        self.sessions.insert(id.clone(), tx);
        (id, rx)
    }

    /// Get a receiver for an existing session.
    #[must_use]
    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<String>> {
        self.sessions.get(id).map(|tx| tx.subscribe())
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

    /// Broadcast a message to all sessions.
    pub fn broadcast(&self, message: String) {
        for entry in &self.sessions {
            let _ = entry.value().send(message.clone());
        }
    }

    /// Remove a session.
    pub fn remove_session(&self, id: &str) {
        self.sessions.remove(id);
    }

    /// Get the number of active sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

/// Thread-safe session store with automatic cleanup.
///
/// Stores session metadata for HTTP request handling.
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
        Self::new(Duration::from_secs(3600))
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

        // Simulate old session by setting last_active in the past
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

        // Send a message
        assert!(manager.send_to_session(&id, "test message".to_string()));

        // Receive the message
        let msg = rx.recv().await.expect("Should receive message");
        assert_eq!(msg, "test message");

        // Remove session
        manager.remove_session(&id);
        assert!(!manager.send_to_session(&id, "another".to_string()));
    }
}
