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

/// Publishes `notifications/tasks/status` when a task changes status.
///
/// The store emits domain events ([`TaskEvent`]); this adapter is the only place
/// that decides a transition is worth telling the client about. It queues onto
/// the server's ambient-notification path rather than sending directly, because
/// a transition has no request-scoped peer to send on.
///
/// Per spec the notification carries the task state in `params` and must **not**
/// be tagged with `io.modelcontextprotocol/related-task` — the `taskId` is
/// already there. [`TaskStatusNotificationParams`] carries no `_meta` when built
/// from a [`Task`](mcpkit_core::types::task::Task), which is what keeps that true.
pub struct TaskStatusNotifier {
    state: Arc<crate::server::ServerState>,
}

impl std::fmt::Debug for TaskStatusNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `ServerState` is not `Debug` (it holds locks and a capability set);
        // the observer's identity is all a reader needs here.
        f.debug_struct("TaskStatusNotifier").finish_non_exhaustive()
    }
}

impl TaskStatusNotifier {
    /// Publish status transitions onto `state`'s ambient notification queue.
    #[must_use]
    pub const fn new(state: Arc<crate::server::ServerState>) -> Self {
        Self { state }
    }
}

impl TaskObserver for TaskStatusNotifier {
    fn on_task_event(&self, event: &TaskEvent) {
        let params = TaskStatusNotificationParams::from(event.task.clone());
        match serde_json::to_value(params) {
            Ok(params) => self.state.publish_notification(Notification::with_params(
                crate::router::notifications::TASK_STATUS,
                params,
            )),
            Err(e) => {
                tracing::warn!(error = ?e, "failed to serialize task status notification");
            }
        }
    }
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
