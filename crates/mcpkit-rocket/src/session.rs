//! Session management for MCP Rocket integration.

use dashmap::DashMap;
use mcpkit_core::auth::{SessionBindingError, VerifiedUser, check_session_binding};
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::adapter_peer::{OutboundOwner, SessionOutbound};
use mcpkit_server::streams::{StreamConfig, StreamRegistry};
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Default idle timeout after which an inactive session is reaped.
pub const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(3600);

/// Session manager for tracking MCP client sessions.
#[derive(Clone)]
pub struct SessionStore {
    sessions: Arc<DashMap<String, SessionState>>,
    idle_timeout: Duration,
    /// Stream configuration applied to each session's SSE stream registry.
    stream_config: StreamConfig,
    /// Default task retention (ms) applied to each session's task store; `None`
    /// means unlimited. Configure via `McpRouter::with_task_ttl`.
    pub(crate) default_task_ttl: Option<u64>,
}

struct SessionState {
    last_seen: Instant,
    /// Protocol version negotiated during initialization.
    protocol_version: Option<ProtocolVersion>,
    /// Client capabilities from initialization.
    client_capabilities: Option<ClientCapabilities>,
    /// The verified user this session is bound to, if any.
    user: Option<VerifiedUser>,
    /// This session's task store for task-augmented `tools/call` (per-session
    /// isolation for `tasks/*`).
    tasks: Arc<mcpkit_server::capability::tasks::TaskManager>,
    /// This session's SSE stream registry (#153): per-stream channels,
    /// single-stream delivery, `{stream_id}-{seq}` ids, same-stream replay.
    streams: Arc<StreamRegistry>,
    /// Owner of the outbound-request registry; dropped with the session so
    /// pending server-initiated requests fail immediately on reap/DELETE.
    outbound_owner: Arc<OutboundOwner>,
    /// In-flight notification-hook tasks (aborted on session teardown).
    hooks: Arc<std::sync::Mutex<tokio::task::JoinSet<()>>>,
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
            idle_timeout: DEFAULT_SESSION_TIMEOUT,
            stream_config: StreamConfig::default(),
            default_task_ttl: Some(mcpkit_server::capability::tasks::DEFAULT_TASK_TTL_MS),
        }
    }

    /// Set the idle timeout after which an inactive session is reaped on the
    /// next [`create`](Self::create).
    #[must_use]
    pub const fn with_idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.idle_timeout = idle_timeout;
        self
    }

    /// Create a new session and return its ID.
    ///
    /// Sessions idle past the idle timeout are reaped first, so the store stays
    /// bounded without a background cleanup task.
    #[must_use]
    pub fn create(&self) -> String {
        self.create_for_user(None)
    }

    /// Create a new session bound to an optional verified user, returning its ID.
    ///
    /// A session created with `Some(user)` may then only be used by that same
    /// user (see [`touch_verified`](Self::touch_verified)).
    #[must_use]
    pub fn create_for_user(&self, user: Option<VerifiedUser>) -> String {
        self.cleanup(self.idle_timeout);
        let id = Uuid::new_v4().to_string();
        let now = Instant::now();
        // The store is built from the stream registry so task transitions
        // publish `notifications/tasks/status` onto this session's SSE stream.
        let streams = Arc::new(StreamRegistry::new(self.stream_config.clone()));
        self.sessions.insert(
            id.clone(),
            SessionState {
                last_seen: now,
                protocol_version: None,
                client_capabilities: None,
                user,
                tasks: mcpkit_server::capability::tasks::session_task_store(
                    &streams,
                    self.default_task_ttl,
                ),
                streams,
                outbound_owner: Arc::new(OutboundOwner::new()),
                hooks: Arc::new(std::sync::Mutex::new(tokio::task::JoinSet::new())),
            },
        );
        id
    }

    /// Update the last seen time for a session.
    pub fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.last_seen = Instant::now();
        }
    }

    /// Touch a session, enforcing its user binding against the identity
    /// presenting this request first.
    ///
    /// Returns `Ok(true)` if the session existed and was touched, `Ok(false)` if
    /// it did not exist, or `Err` on a binding violation.
    ///
    /// # Errors
    ///
    /// Returns the error produced by the underlying operation.
    pub fn touch_verified(
        &self,
        id: &str,
        presenting: Option<&VerifiedUser>,
    ) -> Result<bool, SessionBindingError> {
        let Some(mut session) = self.sessions.get_mut(id) else {
            return Ok(false);
        };
        check_session_binding(session.user.as_ref(), presenting)?;
        session.last_seen = Instant::now();
        Ok(true)
    }

    /// Record the protocol version and client capabilities negotiated during
    /// initialization for a session.
    pub fn set_negotiated(
        &self,
        id: &str,
        protocol_version: ProtocolVersion,
        capabilities: Option<ClientCapabilities>,
    ) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.protocol_version = Some(protocol_version);
            session.client_capabilities = capabilities;
        }
    }

    /// Get the negotiated protocol version and client capabilities for a
    /// session, defaulting the version to the latest before initialization.
    #[must_use]
    pub fn negotiated(&self, id: &str) -> Option<(ProtocolVersion, Option<ClientCapabilities>)> {
        self.sessions.get(id).map(|s| {
            (
                s.protocol_version.unwrap_or(ProtocolVersion::LATEST),
                s.client_capabilities.clone(),
            )
        })
    }

    /// This session's task store, if the session exists.
    #[must_use]
    pub fn tasks(&self, id: &str) -> Option<Arc<mcpkit_server::capability::tasks::TaskManager>> {
        self.sessions.get(id).map(|s| s.tasks.clone())
    }

    /// Check if a session exists.
    #[must_use]
    pub fn exists(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    /// Set the stream configuration applied to each new session.
    #[must_use]
    pub const fn with_stream_config(mut self, config: StreamConfig) -> Self {
        self.stream_config = config;
        self
    }

    /// The SSE stream registry for a session. `None` if unknown.
    #[must_use]
    pub fn streams(&self, id: &str) -> Option<Arc<StreamRegistry>> {
        self.sessions.get(id).map(|s| Arc::clone(&s.streams))
    }

    /// The outbound-request registry for a session. `None` if unknown.
    #[must_use]
    pub fn outbound(&self, id: &str) -> Option<Arc<SessionOutbound>> {
        self.sessions
            .get(id)
            .map(|s| Arc::clone(s.outbound_owner.outbound()))
    }

    /// The notification-hook task set for a session. `None` if unknown.
    #[must_use]
    pub fn hooks(&self, id: &str) -> Option<Arc<std::sync::Mutex<tokio::task::JoinSet<()>>>> {
        self.sessions.get(id).map(|s| Arc::clone(&s.hooks))
    }

    /// Terminate and remove a session (DELETE). Dropping it drops the
    /// `OutboundOwner`, failing all pending server-initiated requests.
    #[must_use]
    pub fn remove(&self, id: &str) -> bool {
        self.sessions.remove(id).is_some()
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

    #[test]
    fn create_reaps_idle_sessions() {
        let store = SessionStore::new().with_idle_timeout(Duration::ZERO);
        let id = store.create();

        // A zero idle timeout makes the previous session reapable, so the next
        // create() sweeps it away.
        let _other = store.create();
        assert!(!store.exists(&id));
    }

    #[test]
    fn create_keeps_recent_sessions() {
        // The default idle timeout is an hour, so a freshly created session
        // survives create-time reaping.
        let store = SessionStore::new();
        let id = store.create();

        let _other = store.create();
        assert!(store.exists(&id));
    }

    #[test]
    fn test_session_store_peer_accessors() {
        let store = SessionStore::new();
        let id = store.create();

        assert!(store.streams(&id).is_some());
        assert!(store.outbound(&id).is_some());
        assert!(store.hooks(&id).is_some());

        assert!(store.streams("non-existent").is_none());
        assert!(store.outbound("non-existent").is_none());
        assert!(store.hooks("non-existent").is_none());
    }

    #[test]
    fn test_session_store_remove() {
        let store = SessionStore::new();
        let id = store.create();

        assert!(store.remove(&id));
        assert!(!store.exists(&id));
        assert!(!store.remove(&id));
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
