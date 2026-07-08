//! Receiver-side task machinery (2025-11-25 experimental tasks).
//!
//! Tasks let a *receiver* run a long-running request in the background while
//! the *requestor* polls for status (`tasks/get`) and, once terminal, the
//! payload (`tasks/result`). Either side can be the receiver: servers receive
//! task-augmented `tools/call`, clients receive task-augmented
//! `sampling/createMessage` / `elicitation/create`. This module is the shared
//! store and dispatch used by both.

use crate::error::{JsonRpcError, McpError};
use crate::types::task::{
    CancelTaskResult, GetTaskResult, ListTasksResult, Task, TaskId, TaskStatus,
};
use event_listener::Event;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context as TaskContext, Poll};
use std::time::Instant;

/// The `_meta` key associating a message with a task
/// (`io.modelcontextprotocol/related-task`).
pub const RELATED_TASK_META_KEY: &str = "io.modelcontextprotocol/related-task";

// ============================================================================
// Cancellation
// ============================================================================

/// A cancellation token for tracking request cancellation.
///
/// Wraps an atomic flag plus an [`event_listener::Event`] so waiters can park
/// until cancellation instead of busy-polling the flag.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    event: Arc<Event>,
}

impl CancellationToken {
    /// Create a new cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            event: Arc::new(Event::new()),
        }
    }

    /// Check if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Wake every task currently waiting in `cancelled()`.
        self.event.notify(usize::MAX);
    }

    /// Wait for cancellation.
    ///
    /// Returns a future that completes when cancellation is requested. The
    /// future parks on an [`event_listener::Event`] and is woken by
    /// [`cancel`](Self::cancel); it does not busy-poll.
    #[must_use]
    pub fn cancelled(&self) -> CancelledFuture {
        CancelledFuture::new(self.cancelled.clone(), self.event.clone())
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// A future that completes when cancellation is requested.
///
/// Parks on the token's [`event_listener::Event`] until cancellation, rather
/// than waking itself on every poll.
pub struct CancelledFuture {
    inner: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl CancelledFuture {
    fn new(cancelled: Arc<AtomicBool>, event: Arc<Event>) -> Self {
        Self {
            inner: Box::pin(async move {
                loop {
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    // Register a listener *before* the final flag check so a
                    // `cancel()` that races with us cannot be missed: if it set
                    // the flag after our first check, the re-check below catches
                    // it; if it fires after, the listener is woken.
                    let listener = event.listen();
                    if cancelled.load(Ordering::SeqCst) {
                        return;
                    }
                    listener.await;
                }
            }),
        }
    }
}

impl Future for CancelledFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        self.inner.as_mut().poll(cx)
    }
}

// ============================================================================
// Task store
// ============================================================================

/// The terminal outcome of the request a task wraps.
///
/// Per spec, `tasks/result` must return exactly what the underlying request
/// would have returned: a successful result, or the JSON-RPC error.
#[derive(Debug, Clone)]
pub enum TaskPayload {
    /// The successful result of the underlying request. Note a *failed* task
    /// can still carry a `Success` payload — e.g. a `tools/call` whose result
    /// has `isError: true` is reported as status `failed`, while its
    /// `tasks/result` payload is that (successful, JSON-RPC-wise) result.
    Success(Value),
    /// The JSON-RPC error the underlying request would have returned.
    Error(JsonRpcError),
}

/// Internal state for a tracked task.
#[derive(Debug, Clone)]
pub struct TaskState {
    /// Task metadata (status, timestamps, ttl).
    pub task: Task,
    /// The eventual outcome, available once the task is terminal (returned by
    /// `tasks/result`).
    pub payload: Option<TaskPayload>,
    /// Cancellation token.
    pub cancel_token: CancellationToken,
    /// When the task was last accessed (for cleanup).
    pub last_access: Instant,
    /// When the task was created. TTL retention is measured from here (per the
    /// `Task.ttl` "retention duration from creation" semantics).
    pub created: Instant,
    /// Notified when the task transitions to a terminal status, waking
    /// blocked `tasks/result` waiters.
    terminal: Arc<Event>,
}

impl TaskState {
    fn new(task: Task) -> Self {
        let now = Instant::now();
        Self {
            task,
            payload: None,
            cancel_token: CancellationToken::new(),
            last_access: now,
            created: now,
            terminal: Arc::new(Event::new()),
        }
    }

    /// Check if the task is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

/// Handle for driving a tracked task to a terminal state.
pub struct TaskHandle {
    task_id: TaskId,
    manager: Arc<TaskManager>,
}

impl TaskHandle {
    /// Get the task ID.
    #[must_use]
    pub const fn id(&self) -> &TaskId {
        &self.task_id
    }

    /// A snapshot of this task's current state.
    #[must_use]
    pub fn task(&self) -> Option<Task> {
        self.manager.get(&self.task_id).map(|s| s.task)
    }

    /// The cancellation token for this task, for wiring into an execution
    /// context so `tasks/cancel` aborts the running operation.
    #[must_use]
    pub fn cancel_token(&self) -> Option<CancellationToken> {
        self.manager.get(&self.task_id).map(|s| s.cancel_token)
    }

    /// Mark the task as waiting for input (e.g. during elicitation/sampling).
    pub fn mark_input_required(&self) -> Result<(), McpError> {
        self.manager
            .set_status(&self.task_id, TaskStatus::InputRequired, None)
    }

    /// Mark the task `completed` and store its payload.
    pub fn complete(&self, payload: Value) -> Result<(), McpError> {
        self.manager.finish(
            &self.task_id,
            TaskStatus::Completed,
            Some(TaskPayload::Success(payload)),
            None,
        )
    }

    /// Mark the task `failed` with a status message.
    ///
    /// The stored `tasks/result` payload is an internal JSON-RPC error carrying
    /// `message`. When the underlying request failed with a specific JSON-RPC
    /// error, prefer [`fail_with_error`](Self::fail_with_error) so
    /// `tasks/result` reproduces it exactly.
    pub fn fail(&self, message: impl Into<String>) -> Result<(), McpError> {
        let message = message.into();
        self.manager.finish(
            &self.task_id,
            TaskStatus::Failed,
            Some(TaskPayload::Error(JsonRpcError::internal_error(
                message.clone(),
            ))),
            Some(message),
        )
    }

    /// Mark the task `failed`, storing the JSON-RPC error the underlying
    /// request would have returned (reproduced verbatim by `tasks/result`).
    pub fn fail_with_error(&self, error: JsonRpcError) -> Result<(), McpError> {
        let message = error.message.clone();
        self.manager.finish(
            &self.task_id,
            TaskStatus::Failed,
            Some(TaskPayload::Error(error)),
            Some(message),
        )
    }

    /// Mark the task `failed` while storing a *successful* payload.
    ///
    /// Per spec, a `tools/call` whose result has `isError: true` reaches the
    /// `failed` status, but `tasks/result` still returns that result.
    pub fn fail_with_result(
        &self,
        payload: Value,
        message: Option<String>,
    ) -> Result<(), McpError> {
        self.manager.finish(
            &self.task_id,
            TaskStatus::Failed,
            Some(TaskPayload::Success(payload)),
            message,
        )
    }

    /// Check if the task has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.manager
            .get(&self.task_id)
            .is_none_or(|s| s.is_cancelled())
    }

    /// A future that completes when the task is cancelled.
    pub async fn cancelled(&self) {
        if let Some(state) = self.manager.get(&self.task_id) {
            state.cancel_token.cancelled().await;
        }
    }
}

/// Default retention for a terminal task whose request omitted a `ttl`
/// (one hour, in milliseconds). Override via [`TaskManager::with_default_ttl`].
pub const DEFAULT_TASK_TTL_MS: u64 = 60 * 60 * 1000;

/// Manager coordinating the lifecycle of tracked tasks.
#[derive(Debug)]
pub struct TaskManager {
    tasks: RwLock<HashMap<TaskId, TaskState>>,
    /// Retention applied to a task when the request omits `ttl`. `None` means
    /// unlimited (such tasks are never TTL-evicted).
    default_ttl_ms: Option<u64>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    /// Create a new task manager retaining terminal tasks for
    /// [`DEFAULT_TASK_TTL_MS`] when the request omits a `ttl`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_default_ttl(Some(DEFAULT_TASK_TTL_MS))
    }

    /// Create a task manager with a custom default retention (milliseconds) for
    /// tasks whose request omits `ttl`. Pass `None` for unlimited retention (such
    /// tasks are never TTL-evicted).
    #[must_use]
    pub fn with_default_ttl(default_ttl_ms: Option<u64>) -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            default_ttl_ms,
        }
    }

    /// Create a new `working` task and return a handle to it.
    ///
    /// A `None` `ttl` is materialized to the manager's default so the returned
    /// task reports its actual retention. Expired terminal tasks are swept first.
    pub fn create(self: &Arc<Self>, ttl: Option<u64>) -> TaskHandle {
        self.cleanup_expired();

        let mut task = Task::create();
        task.ttl = ttl.or(self.default_ttl_ms);
        let task_id = task.task_id.clone();

        if let Ok(mut tasks) = self.tasks.write() {
            tasks.insert(task_id.clone(), TaskState::new(task));
        }

        TaskHandle {
            task_id,
            manager: Arc::clone(self),
        }
    }

    /// Get a snapshot of a task's state by ID.
    #[must_use]
    pub fn get(&self, id: &TaskId) -> Option<TaskState> {
        self.tasks.read().ok()?.get(id).cloned()
    }

    /// List all tracked tasks.
    #[must_use]
    pub fn list(&self) -> Vec<Task> {
        self.tasks
            .read()
            .map(|tasks| tasks.values().map(|s| s.task.clone()).collect())
            .unwrap_or_default()
    }

    /// Get the stored outcome of a terminal task, if available.
    #[must_use]
    pub fn payload(&self, id: &TaskId) -> Option<TaskPayload> {
        self.tasks.read().ok()?.get(id)?.payload.clone()
    }

    /// Wait until the task reaches a terminal status, returning its final
    /// state. Returns `None` if the task is unknown (or evicted while
    /// waiting).
    ///
    /// The wait parks on a per-task event notified by terminal transitions;
    /// no lock is held across an await.
    pub async fn wait_terminal(&self, id: &TaskId) -> Option<TaskState> {
        loop {
            let listener = {
                let tasks = self.tasks.read().ok()?;
                let state = tasks.get(id)?;
                if state.task.status.is_terminal() {
                    return Some(state.clone());
                }
                // Register before releasing the lock: a terminal transition
                // takes the write lock, so it can only notify after we are
                // listening.
                state.terminal.listen()
            };
            listener.await;
        }
    }

    /// Cancel a task.
    ///
    /// Cancelling a task already in a terminal status is rejected with
    /// *invalid params* (spec).
    pub fn cancel(&self, id: &TaskId) -> Result<(), McpError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| McpError::internal("Failed to acquire task lock"))?;

        if let Some(state) = tasks.get_mut(id) {
            if state.task.status.is_terminal() {
                return Err(McpError::invalid_params(
                    "tasks/cancel",
                    format!(
                        "Cannot cancel task: already in terminal status '{}'",
                        state.task.status
                    ),
                ));
            }
            state.cancel_token.cancel();
            state.task.set_status(TaskStatus::Cancelled);
            state.last_access = Instant::now();
            state.terminal.notify(usize::MAX);
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/cancel",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Set a task's status (and optional status message).
    fn set_status(
        &self,
        id: &TaskId,
        status: TaskStatus,
        message: Option<String>,
    ) -> Result<(), McpError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| McpError::internal("Failed to acquire task lock"))?;

        if let Some(state) = tasks.get_mut(id) {
            // Terminal statuses are final (spec): in particular, a cancelled
            // task stays cancelled even if its execution later finishes.
            if state.task.status.is_terminal() {
                return Err(McpError::invalid_params(
                    "tasks/get",
                    format!(
                        "task {} is already terminal ('{}')",
                        id.as_str(),
                        state.task.status
                    ),
                ));
            }
            state.task.set_status(status);
            if message.is_some() {
                state.task.status_message = message;
            }
            state.last_access = Instant::now();
            if status.is_terminal() {
                state.terminal.notify(usize::MAX);
            }
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Move a task to a terminal status, storing its outcome.
    fn finish(
        &self,
        id: &TaskId,
        status: TaskStatus,
        payload: Option<TaskPayload>,
        message: Option<String>,
    ) -> Result<(), McpError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| McpError::internal("Failed to acquire task lock"))?;

        if let Some(state) = tasks.get_mut(id) {
            // Terminal statuses are final (spec): a cancelled task stays
            // cancelled even if its execution later completes or fails, and
            // its outcome is discarded.
            if state.task.status.is_terminal() {
                return Err(McpError::invalid_params(
                    "tasks/result",
                    format!(
                        "task {} is already terminal ('{}')",
                        id.as_str(),
                        state.task.status
                    ),
                ));
            }
            state.task.set_status(status);
            if message.is_some() {
                state.task.status_message = message;
            }
            state.payload = payload;
            state.last_access = Instant::now();
            state.terminal.notify(usize::MAX);
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/result",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Remove terminal tasks older than `max_age`.
    pub fn cleanup(&self, max_age: std::time::Duration) {
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.retain(|_, state| {
                let is_terminal = state.task.status.is_terminal();
                !is_terminal || state.last_access.elapsed() < max_age
            });
        }
    }

    /// Evict terminal tasks whose age since creation exceeds their own `ttl`
    /// (milliseconds). Non-terminal tasks are always kept; a task with `ttl`
    /// `None` (unlimited) is never evicted. Called on `create` and on task-store
    /// access, so no timer is required.
    pub fn cleanup_expired(&self) {
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.retain(|_, state| {
                if !state.task.status.is_terminal() {
                    return true;
                }
                match state.task.ttl {
                    Some(ttl_ms) => {
                        state.created.elapsed() < std::time::Duration::from_millis(ttl_ms)
                    }
                    None => true,
                }
            });
        }
    }
}

// ============================================================================
// Dispatch
// ============================================================================

/// Attach `_meta["io.modelcontextprotocol/related-task"]` to a `tasks/result`
/// payload (spec MUST for `tasks/result` responses). Merges with any existing
/// `_meta` keys.
fn inject_related_task(mut payload: Value, id: &TaskId) -> Value {
    if let Value::Object(map) = &mut payload {
        if let Value::Object(meta) = map
            .entry("_meta")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
        {
            meta.insert(
                RELATED_TASK_META_KEY.to_string(),
                serde_json::json!({ "taskId": id.as_str() }),
            );
        }
    }
    payload
}

/// Serve task queries from a [`TaskManager`] store.
///
/// Returns `None` for non-task methods, and for `tasks/get`/`tasks/result`/
/// `tasks/cancel` whose id the store does not own (so a caller can fall through
/// to a custom task handler). Shared by the server runtime, the HTTP adapters,
/// and the client, so every receiver serves `tasks/*` identically against its
/// own store.
///
/// Per spec, `tasks/result` **blocks** until the task reaches a terminal
/// status, then returns exactly what the underlying request would have
/// returned: the successful result (with the `related-task` `_meta` attached),
/// or the stored JSON-RPC error verbatim. Error responses carry no
/// `related-task` metadata — the schema's `Error` object has no `_meta` slot,
/// so the spec's requirement is unsatisfiable there; the requestor already
/// knows the task id it asked for.
pub async fn route_task_store(
    store: &TaskManager,
    method: &str,
    params: Option<&Value>,
) -> Option<Result<Value, McpError>> {
    // Sweep expired terminal tasks on access, so a session that stops creating
    // but keeps polling/listing still bounds its store.
    store.cleanup_expired();
    let task_id = || {
        params
            .and_then(|p| p.get("taskId"))
            .and_then(|v| v.as_str())
            .map(TaskId::new)
    };
    match method {
        "tasks/list" => {
            // Serialize through `ListTasksResult` (the built-in store has no
            // cursor/`_meta` to add). `nextCursor`/`_meta` omit when `None`.
            let result = ListTasksResult::from(store.list());
            Some(Ok(serde_json::to_value(result).unwrap_or_default()))
        }
        "tasks/get" => {
            let Some(id) = task_id() else {
                return Some(Err(McpError::invalid_params("tasks/get", "missing taskId")));
            };
            store.get(&id).map(|s| {
                let result = GetTaskResult::from(s.task);
                Ok(serde_json::to_value(result).unwrap_or_default())
            })
        }
        "tasks/result" => {
            let Some(id) = task_id() else {
                return Some(Err(McpError::invalid_params(
                    "tasks/result",
                    "missing taskId",
                )));
            };
            // Unknown id: fall through to a custom handler.
            store.get(&id)?;
            // Spec: MUST block until the task reaches a terminal status.
            let Some(state) = store.wait_terminal(&id).await else {
                // Evicted (TTL) while waiting.
                return Some(Err(McpError::invalid_params(
                    "tasks/result",
                    format!("Task has expired: {}", id.as_str()),
                )));
            };
            match state.payload {
                Some(TaskPayload::Success(payload)) => Some(Ok(inject_related_task(payload, &id))),
                Some(TaskPayload::Error(error)) => Some(Err(McpError::JsonRpc(error))),
                // Terminal with no stored outcome — e.g. cancelled before the
                // underlying request finished.
                None => Some(Err(McpError::invalid_params(
                    "tasks/result",
                    format!(
                        "task {} ended {} with no result",
                        id.as_str(),
                        state.task.status
                    ),
                ))),
            }
        }
        "tasks/cancel" => {
            let Some(id) = task_id() else {
                return Some(Err(McpError::invalid_params(
                    "tasks/cancel",
                    "missing taskId",
                )));
            };
            if store.get(&id).is_some() {
                // Cancelling an already-terminal task is -32602 (spec).
                if let Err(e) = store.cancel(&id) {
                    return Some(Err(e));
                }
                Some(Ok(store
                    .get(&id)
                    .map(|s| {
                        let result = CancelTaskResult::from(s.task);
                        serde_json::to_value(result).unwrap_or_default()
                    })
                    .unwrap_or_default()))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_manager_create_and_list() {
        let manager = Arc::new(TaskManager::new());

        let handle = manager.create(None);
        assert!(!handle.is_cancelled());

        let tasks = manager.list();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, TaskStatus::Working);
    }

    #[test]
    fn test_task_complete_stores_payload() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let task_id = handle.id().clone();

        handle.complete(serde_json::json!({"result": "ok"}))?;

        let state = manager.get(&task_id).ok_or("Task not found")?;
        assert_eq!(state.task.status, TaskStatus::Completed);
        match manager.payload(&task_id) {
            Some(TaskPayload::Success(v)) => {
                assert_eq!(v, serde_json::json!({"result": "ok"}));
            }
            other => panic!("expected success payload, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn test_task_input_required_and_fail() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let task_id = handle.id().clone();

        handle.mark_input_required()?;
        assert_eq!(
            manager.get(&task_id).ok_or("not found")?.task.status,
            TaskStatus::InputRequired
        );

        handle.fail("boom")?;
        let state = manager.get(&task_id).ok_or("not found")?;
        assert_eq!(state.task.status, TaskStatus::Failed);
        assert_eq!(state.task.status_message.as_deref(), Some("boom"));
        Ok(())
    }

    #[test]
    fn test_task_cancellation() -> Result<(), Box<dyn std::error::Error>> {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let task_id = handle.id().clone();

        assert!(!handle.is_cancelled());
        manager.cancel(&task_id)?;
        assert!(handle.is_cancelled());
        assert_eq!(
            manager.get(&task_id).ok_or("not found")?.task.status,
            TaskStatus::Cancelled
        );
        Ok(())
    }

    // --- #121: TTL cleanup ------------------------------------------------

    #[test]
    fn omitted_ttl_is_materialized_to_default() {
        let manager = Arc::new(TaskManager::with_default_ttl(Some(5000)));
        // Omitted ttl -> materialized to the default so the task reports it.
        assert_eq!(manager.create(None).task().unwrap().ttl, Some(5000));
        // An explicit ttl is preserved.
        assert_eq!(manager.create(Some(1234)).task().unwrap().ttl, Some(1234));
    }

    #[test]
    fn cleanup_expired_evicts_old_terminal_task() {
        let manager = Arc::new(TaskManager::with_default_ttl(Some(1)));
        let handle = manager.create(None); // ttl materialized to 1ms
        let id = handle.id().clone();
        handle.complete(serde_json::json!({})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        manager.cleanup_expired();
        assert!(
            manager.get(&id).is_none(),
            "expired terminal task not evicted"
        );
    }

    #[test]
    fn cleanup_expired_keeps_fresh_terminal_task() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(Some(60_000));
        let id = handle.id().clone();
        handle.complete(serde_json::json!({})).unwrap();
        manager.cleanup_expired();
        assert!(
            manager.get(&id).is_some(),
            "fresh terminal task wrongly evicted"
        );
    }

    #[test]
    fn cleanup_expired_keeps_non_terminal_task() {
        let manager = Arc::new(TaskManager::with_default_ttl(Some(1)));
        let handle = manager.create(None); // stays Working
        let id = handle.id().clone();
        std::thread::sleep(std::time::Duration::from_millis(20));
        manager.cleanup_expired();
        assert!(
            manager.get(&id).is_some(),
            "non-terminal task must never be evicted"
        );
    }

    #[test]
    fn unlimited_ttl_is_never_evicted() {
        let manager = Arc::new(TaskManager::with_default_ttl(None));
        let handle = manager.create(None); // ttl stays None (unlimited)
        let id = handle.id().clone();
        assert_eq!(handle.task().unwrap().ttl, None);
        handle.complete(serde_json::json!({})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        manager.cleanup_expired();
        assert!(
            manager.get(&id).is_some(),
            "unlimited-ttl task wrongly evicted"
        );
    }

    #[tokio::test]
    async fn route_task_store_access_triggers_cleanup() {
        let manager = Arc::new(TaskManager::with_default_ttl(Some(1)));
        let handle = manager.create(None);
        let id = handle.id().clone();
        handle.complete(serde_json::json!({})).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        // A tasks/* access (not just create) sweeps the store.
        let _ = route_task_store(&manager, "tasks/list", None).await;
        assert!(manager.get(&id).is_none(), "access did not trigger cleanup");
    }

    // --- #143 phase 2: blocking tasks/result, error passthrough, _meta -----

    fn result_params(id: &TaskId) -> Value {
        serde_json::json!({ "taskId": id.as_str() })
    }

    #[tokio::test]
    async fn tasks_result_returns_immediately_for_terminal_task() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        handle
            .complete(serde_json::json!({ "answer": 42 }))
            .unwrap();

        let params = result_params(&id);
        let result = route_task_store(&manager, "tasks/result", Some(&params))
            .await
            .expect("owned task")
            .expect("success");
        assert_eq!(result["answer"], 42);
    }

    #[tokio::test]
    async fn tasks_result_blocks_until_terminal() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();

        let completer = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let handle = TaskHandle {
                    task_id: id,
                    manager,
                };
                handle
                    .complete(serde_json::json!({ "late": true }))
                    .unwrap();
            })
        };

        let params = result_params(handle.id());
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            route_task_store(&manager, "tasks/result", Some(&params)),
        )
        .await
        .expect("must not hang")
        .expect("owned task")
        .expect("success");
        assert_eq!(result["late"], true);
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(40),
            "must have blocked until completion"
        );
        completer.await.unwrap();
    }

    #[tokio::test]
    async fn tasks_result_reproduces_stored_jsonrpc_error() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        let stored = JsonRpcError {
            code: -32001,
            message: "downstream exploded".to_string(),
            data: Some(serde_json::json!({ "detail": "xyz" })),
        };
        handle.fail_with_error(stored.clone()).unwrap();

        let params = result_params(&id);
        let err = route_task_store(&manager, "tasks/result", Some(&params))
            .await
            .expect("owned task")
            .expect_err("stored error");
        let wire: JsonRpcError = (&err).into();
        assert_eq!(wire.code, stored.code);
        assert_eq!(wire.message, stored.message);
        assert_eq!(wire.data, stored.data);
        // The task itself reports failed + the diagnostic message.
        let state = manager.get(&id).unwrap();
        assert_eq!(state.task.status, TaskStatus::Failed);
        assert_eq!(
            state.task.status_message.as_deref(),
            Some("downstream exploded")
        );
    }

    #[tokio::test]
    async fn tasks_result_success_carries_related_task_meta() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        // Existing _meta keys must be preserved, not clobbered.
        handle
            .complete(serde_json::json!({ "ok": true, "_meta": { "keep": 1 } }))
            .unwrap();

        let params = result_params(&id);
        let result = route_task_store(&manager, "tasks/result", Some(&params))
            .await
            .expect("owned task")
            .expect("success");
        assert_eq!(result["_meta"]["keep"], 1);
        assert_eq!(
            result["_meta"][RELATED_TASK_META_KEY]["taskId"],
            id.as_str()
        );
    }

    #[tokio::test]
    async fn failed_task_with_success_payload_returns_it() {
        // A tools/call whose result has isError:true is status `failed`, but
        // tasks/result returns that (JSON-RPC-successful) result.
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        handle
            .fail_with_result(
                serde_json::json!({ "isError": true, "content": [] }),
                Some("tool reported an error".to_string()),
            )
            .unwrap();

        assert_eq!(manager.get(&id).unwrap().task.status, TaskStatus::Failed);
        let params = result_params(&id);
        let result = route_task_store(&manager, "tasks/result", Some(&params))
            .await
            .expect("owned task")
            .expect("isError result is still a successful JSON-RPC response");
        assert_eq!(result["isError"], true);
        assert_eq!(
            result["_meta"][RELATED_TASK_META_KEY]["taskId"],
            id.as_str()
        );
    }

    #[tokio::test]
    async fn tasks_result_for_cancelled_task_is_an_error() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        manager.cancel(&id).unwrap();

        let params = result_params(&id);
        let err = route_task_store(&manager, "tasks/result", Some(&params))
            .await
            .expect("owned task")
            .expect_err("cancelled task has no result");
        assert_eq!(err.code(), -32602);
    }

    #[tokio::test]
    async fn cancel_unblocks_tasks_result_waiter() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();

        let canceller = {
            let manager = Arc::clone(&manager);
            let id = id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                manager.cancel(&id).unwrap();
            })
        };

        let params = result_params(&id);
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            route_task_store(&manager, "tasks/result", Some(&params)),
        )
        .await
        .expect("cancel must unblock the waiter")
        .expect("owned task");
        assert!(outcome.is_err(), "cancelled task has no result");
        canceller.await.unwrap();
    }

    // --- terminal-state immutability (spec: terminal statuses are final) ---

    #[test]
    fn cancelled_task_stays_cancelled_when_execution_completes() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        manager.cancel(&id).unwrap();

        // The background execution finishes anyway; the outcome is discarded.
        assert!(handle.complete(serde_json::json!({ "late": 1 })).is_err());
        assert!(handle.fail("late failure").is_err());

        let state = manager.get(&id).unwrap();
        assert_eq!(state.task.status, TaskStatus::Cancelled);
        assert!(state.payload.is_none(), "late outcome must be discarded");
    }

    #[tokio::test]
    async fn cancel_on_terminal_task_is_invalid_params() {
        let manager = Arc::new(TaskManager::new());
        let handle = manager.create(None);
        let id = handle.id().clone();
        handle.complete(serde_json::json!({})).unwrap();

        // Direct store call.
        let err = manager.cancel(&id).expect_err("terminal cancel rejected");
        assert_eq!(err.code(), -32602);

        // And through the route.
        let params = result_params(&id);
        let err = route_task_store(&manager, "tasks/cancel", Some(&params))
            .await
            .expect("owned task")
            .expect_err("terminal cancel rejected");
        assert_eq!(err.code(), -32602);
    }

    // --- cancellation token (moved with the token from mcpkit-server) ------

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    /// Regression test for #8: `cancelled()` must park on a waker instead of
    /// busy-spinning (the old impl called `wake_by_ref()` on every poll). We
    /// poll with a waker that counts wake-ups and assert the future does not
    /// wake itself, then that `cancel()` wakes it and it resolves.
    #[test]
    fn cancelled_future_parks_and_wakes_on_cancel() {
        use std::sync::atomic::AtomicUsize;
        use std::task::{Wake, Waker};

        struct CountingWaker(AtomicUsize);
        impl Wake for CountingWaker {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            fn wake_by_ref(self: &Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut cx = TaskContext::from_waker(&waker);

        let token = CancellationToken::new();
        let mut fut = Box::pin(token.cancelled());

        // First poll: not cancelled -> must be Pending and must NOT have woken
        // itself (a busy-spin would wake immediately).
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Pending);
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            0,
            "cancelled future must park, not busy-spin (no self-wake)"
        );

        // Cancelling wakes the registered waker and the future resolves.
        token.cancel();
        assert!(
            counter.0.load(Ordering::SeqCst) >= 1,
            "cancel() must wake the parked waiter"
        );
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }

    /// A token already cancelled before `cancelled()` is awaited resolves
    /// immediately.
    #[test]
    fn cancelled_future_ready_when_already_cancelled() {
        let waker = std::task::Waker::noop();
        let mut cx = TaskContext::from_waker(waker);

        let token = CancellationToken::new();
        token.cancel();
        let mut fut = Box::pin(token.cancelled());
        assert_eq!(fut.as_mut().poll(&mut cx), Poll::Ready(()));
    }
}
