//! Task capability implementation.
//!
//! Tasks let a server run a long-running operation while the caller polls for
//! status (`tasks/get`) and, once terminal, the payload (`tasks/result`).
//!
//! The store itself ([`TaskManager`], [`TaskHandle`], [`route_task_store`]) is
//! shared with the client side and lives in [`mcpkit_core::tasks`]; this
//! module re-exports it and adds the server-only [`TaskService`].

pub use mcpkit_core::tasks::{
    DEFAULT_TASK_TTL_MS, RELATED_TASK_META_KEY, TaskEvent, TaskHandle, TaskManager, TaskObserver,
    TaskPayload, TaskRoute, TaskState, route_task_store,
};

/// Where an ambient notification goes on a given transport.
///
/// A task transition has no request-scoped [`Peer`](crate::context::Peer) to
/// send on, and each transport reaches its client differently: the stdio/socket
/// runtime queues onto [`ServerState`](crate::server::ServerState)'s ambient
/// pump, while the HTTP adapters store-and-forward onto the session's SSE
/// [`StreamRegistry`](crate::streams::StreamRegistry).
///
/// The trait exists so the *mapping* from a task transition to
/// `notifications/tasks/status` is written once. A new transport implements one
/// method; it does not re-derive the notification, and so cannot get it subtly
/// wrong the way five hand-rolled copies of a routing rule did.
///
/// Publishing is best-effort by contract: a notification with nowhere to go is
/// dropped, never an error.
pub trait NotificationSink: Send + Sync {
    /// Hand the notification to this transport's outbound path.
    fn publish(&self, notification: Notification);
}

impl NotificationSink for crate::server::ServerState {
    fn publish(&self, notification: Notification) {
        self.publish_notification(notification);
    }
}

impl NotificationSink for crate::streams::StreamRegistry {
    fn publish(&self, notification: Notification) {
        match serde_json::to_string(&mcpkit_core::protocol::Message::Notification(notification)) {
            // `None` simply means no live stream; the event is buffered for a
            // resuming GET, and a client that never returns misses it.
            Ok(json) => {
                let _ = self.send("message", json);
            }
            Err(e) => tracing::warn!(error = ?e, "failed to serialize ambient notification"),
        }
    }
}

/// Publishes `notifications/tasks/status` when a task changes status.
///
/// The store emits domain events ([`TaskEvent`]); this is the only place that
/// decides a transition is worth telling the client about, and the only place
/// that builds the notification. Where it goes is the [`NotificationSink`]'s
/// business.
///
/// Per spec the notification carries the task state in `params` and must **not**
/// be tagged with `io.modelcontextprotocol/related-task` — the `taskId` is
/// already there. [`TaskStatusNotificationParams`] carries no `_meta` when built
/// from a [`Task`](mcpkit_core::types::task::Task), which is what keeps that true.
pub struct TaskStatusNotifier {
    sink: Arc<dyn NotificationSink>,
}

impl std::fmt::Debug for TaskStatusNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The sink is not `Debug` (implementors hold locks and capability
        // sets); the observer's identity is all a reader needs here.
        f.debug_struct("TaskStatusNotifier").finish_non_exhaustive()
    }
}

impl TaskStatusNotifier {
    /// Publish status transitions onto `sink`.
    #[must_use]
    pub const fn new(sink: Arc<dyn NotificationSink>) -> Self {
        Self { sink }
    }
}

impl TaskObserver for TaskStatusNotifier {
    fn on_task_event(&self, event: &TaskEvent) {
        let params = TaskStatusNotificationParams::from(event.task.clone());
        match serde_json::to_value(params) {
            Ok(params) => self.sink.publish(Notification::with_params(
                crate::router::notifications::TASK_STATUS,
                params,
            )),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to serialize task status notification");
            }
        }
    }
}

/// Build a per-session task store that publishes `notifications/tasks/status`
/// onto the session's SSE stream registry.
///
/// Every HTTP adapter creates its `TaskManager` and `StreamRegistry` together
/// per session; this is the one place that wires them, so an adapter cannot
/// forget to and silently stop emitting the notification.
///
/// `default_ttl_ms` mirrors [`TaskManager::with_default_ttl`].
#[must_use]
pub fn session_task_store(
    streams: &Arc<crate::streams::StreamRegistry>,
    default_ttl_ms: Option<u64>,
) -> Arc<TaskManager> {
    let store = Arc::new(TaskManager::with_default_ttl(default_ttl_ms));
    // Only fails if an observer were already registered, which cannot happen on
    // a store constructed a line ago.
    let sink: Arc<dyn NotificationSink> = Arc::<crate::streams::StreamRegistry>::clone(streams);
    let _ = store.set_observer(Arc::new(TaskStatusNotifier::new(sink)));
    store
}

use crate::context::Context;
use crate::handler::TaskHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::protocol::Notification;
use mcpkit_core::types::task::{
    CancelTaskResult, GetTaskResult, ListTasksResult, TaskId, TaskStatusNotificationParams,
};
use std::sync::Arc;

/// Task service implementing the [`TaskHandler`] trait over a [`TaskManager`].
pub struct TaskService {
    manager: Arc<TaskManager>,
}

impl Default for TaskService {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskService {
    /// Create a new task service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            manager: Arc::new(TaskManager::new()),
        }
    }

    /// Get the underlying task manager.
    #[must_use]
    pub const fn manager(&self) -> &Arc<TaskManager> {
        &self.manager
    }

    /// Create a new task and return a handle for driving it.
    #[must_use]
    pub fn create(&self) -> TaskHandle {
        self.manager.create(None)
    }
}

impl TaskHandler for TaskService {
    async fn list_tasks(&self, _ctx: &Context<'_>) -> Result<ListTasksResult, McpError> {
        Ok(self.manager.list().into())
    }

    async fn get_task(
        &self,
        task_id: &TaskId,
        _ctx: &Context<'_>,
    ) -> Result<Option<GetTaskResult>, McpError> {
        Ok(self
            .manager
            .get(task_id)
            .map(|s| GetTaskResult::from(s.task)))
    }

    async fn cancel_task(
        &self,
        task_id: &TaskId,
        _ctx: &Context<'_>,
    ) -> Result<Option<CancelTaskResult>, McpError> {
        // Unknown task -> Ok(None); a real internal failure (e.g. poisoned lock)
        // must surface as Err, not be collapsed into "unknown".
        if self.manager.get(task_id).is_none() {
            return Ok(None);
        }
        self.manager.cancel(task_id)?;
        Ok(self
            .manager
            .get(task_id)
            .map(|s| CancelTaskResult::from(s.task)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_service_handler() -> Result<(), Box<dyn std::error::Error>> {
        let service = TaskService::new();
        let handle = service.create();
        let task_id = handle.id().clone();

        assert_eq!(service.manager().list().len(), 1);
        assert!(service.manager().get(&task_id).is_some());
        Ok(())
    }
}

#[cfg(test)]
mod notifier_tests {
    use super::*;
    use crate::streams::{StreamConfig, StreamRegistry};

    /// `session_task_store` is the one place adapters wire a store to a stream;
    /// if it stops publishing, every HTTP transport silently stops emitting
    /// `notifications/tasks/status`.
    #[tokio::test]
    async fn session_task_store_publishes_transitions_onto_the_registry() {
        let streams = Arc::new(StreamRegistry::new(StreamConfig::default()));
        let (mut handle, _prime) = streams.open("message", "{}".to_string());
        let store = session_task_store(&streams, None);

        let task = store.create(None);
        task.complete(serde_json::json!({"ok": true}))
            .expect("complete");

        let event = handle.recv().await.expect("an event");
        let json: serde_json::Value = serde_json::from_str(&event.data).expect("json");
        assert_eq!(json["method"], "notifications/tasks/status");
        assert_eq!(json["params"]["status"], "completed");
        assert_eq!(json["params"]["taskId"], task.id().as_str());
        assert!(
            json["params"]["_meta"].is_null(),
            "must not carry related-task _meta: {json}"
        );
    }

    /// A store with no live stream must not error or panic — notifications are
    /// best-effort by contract.
    #[tokio::test]
    async fn publishing_with_no_live_stream_is_a_no_op() {
        let streams = Arc::new(StreamRegistry::new(StreamConfig::default()));
        let store = session_task_store(&streams, None);
        let task = store.create(None);
        task.complete(serde_json::json!({})).expect("complete");
        assert!(!streams.has_live_stream());
    }
}
