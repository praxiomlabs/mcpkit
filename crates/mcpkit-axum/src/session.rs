//! Session management for MCP HTTP connections.

use dashmap::DashMap;
use mcpkit_core::auth::{SessionBindingError, VerifiedUser, check_session_binding};
use mcpkit_core::capability::ClientCapabilities;
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::adapter_peer::{OutboundOwner, SessionOutbound};
use mcpkit_server::streams::{StreamConfig, StreamRegistry};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// This session's task store for task-augmented `tools/call`. Scoped per
    /// session so one session cannot read or cancel another's tasks (matching
    /// the stdio runtime's per-connection store).
    pub tasks: Arc<mcpkit_server::capability::tasks::TaskManager>,
    /// This session's SSE stream registry (#153 PR 2): per-stream bounded
    /// channels, single-stream delivery, `{stream_id}-{seq}` event ids, and
    /// same-stream `Last-Event-ID` replay — shared adapter logic from
    /// `mcpkit_server::streams`.
    pub streams: Arc<StreamRegistry>,
    /// Owner of this session's outbound-request registry (#153 PR 4). When
    /// the session is removed (reap or DELETE) the owner drops and every
    /// pending server-initiated request fails immediately, so waiting hooks
    /// and tools resolve instead of running out their timeout.
    pub outbound_owner: Arc<OutboundOwner>,
    /// In-flight notification-hook tasks for this session. Dropped (and
    /// thereby ABORTED — deliberate) on session teardown: the session and
    /// its peer are gone, so a hook mid-request has nothing valid to await.
    pub hooks: Arc<std::sync::Mutex<tokio::task::JoinSet<()>>>,
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
            tasks: Arc::new(mcpkit_server::capability::tasks::TaskManager::new()),
            streams: Arc::new(StreamRegistry::new(StreamConfig::default())),
            outbound_owner: Arc::new(OutboundOwner::new()),
            hooks: Arc::new(std::sync::Mutex::new(tokio::task::JoinSet::new())),
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
    /// Default task retention (ms) applied to each session's task store; `None`
    /// means unlimited. Configure via `McpRouter::with_task_ttl`.
    pub(crate) default_task_ttl: Option<u64>,
    /// Stream configuration applied to each session's SSE stream registry.
    stream_config: StreamConfig,
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
            default_task_ttl: Some(mcpkit_server::capability::tasks::DEFAULT_TASK_TTL_MS),
            stream_config: StreamConfig::default(),
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

    /// Set the stream configuration applied to each new session's SSE
    /// stream registry.
    #[must_use]
    pub fn with_stream_config(mut self, config: StreamConfig) -> Self {
        self.stream_config = config;
        self
    }

    /// The SSE stream registry for a session. `None` if the session is
    /// unknown.
    #[must_use]
    pub fn streams(&self, id: &str) -> Option<Arc<StreamRegistry>> {
        self.sessions.get(id).map(|s| Arc::clone(&s.streams))
    }

    /// Store `message` on the session's designated stream and queue it for
    /// delivery, returning the allocated event id. `None` if the session is
    /// unknown or has no live stream.
    #[must_use]
    pub fn send_event(&self, id: &str, event_type: &str, message: String) -> Option<String> {
        self.sessions.get(id)?.streams.send(event_type, message)
    }

    /// The outbound-request registry for a session (server-initiated request
    /// correlation). `None` if the session is unknown.
    #[must_use]
    pub fn outbound(&self, id: &str) -> Option<Arc<SessionOutbound>> {
        self.sessions
            .get(id)
            .map(|s| Arc::clone(s.outbound_owner.outbound()))
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
        let mut session = Session::with_user(id.clone(), user);
        session.tasks = Arc::new(
            mcpkit_server::capability::tasks::TaskManager::with_default_ttl(self.default_task_ttl),
        );
        session.streams = Arc::new(StreamRegistry::new(self.stream_config.clone()));
        self.sessions.insert(id.clone(), session);
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
    async fn session_stream_send_and_receive() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::with_default_timeout();
        let id = store.create();
        let registry = store.streams(&id).ok_or("session missing")?;
        let (mut handle, prime) = registry.open("connected", id.clone());
        assert_eq!(prime.event_type, "connected");

        // Send an event (stored + queued on the designated stream).
        let event_id = store.send_event(&id, "message", "test message".to_string());
        assert!(event_id.is_some());

        let got = handle.recv().await.ok_or("stream closed")?;
        assert_eq!(got.data, "test message");
        assert_eq!(Some(got.id), event_id);

        // Unknown session: no registry, no send.
        assert!(store.streams("nope").is_none());
        assert!(
            store
                .send_event("nope", "message", "x".to_string())
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn session_without_live_stream_drops_sends() {
        // The event is not deliverable (send_event -> None) when no GET
        // stream was ever opened; nothing panics and nothing leaks.
        let store = SessionStore::with_default_timeout();
        let id = store.create();
        assert!(store.send_event(&id, "message", "x".to_string()).is_none());
    }

    #[tokio::test]
    async fn session_replay_after_stream_death() -> Result<(), Box<dyn std::error::Error>> {
        let store = SessionStore::with_default_timeout();
        let id = store.create();
        let registry = store.streams(&id).ok_or("session missing")?;
        let (handle, _prime) = registry.open("connected", id.clone());

        let evt1 = store
            .send_event(&id, "message", "msg1".to_string())
            .ok_or("send failed")?;
        let _ = store.send_event(&id, "message", "msg2".to_string());
        drop(handle); // stream dies (client disconnect)

        // Resume with Last-Event-ID = evt1: replay only what followed, on the
        // same stream identity.
        let (_h2, replay) = registry.resume(&evt1).ok_or("not resumable")?;
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].data, "msg2");
        Ok(())
    }
}
