//! Task capability implementation.
//!
//! Tasks let a server run a long-running operation while the caller polls for
//! status (`tasks/get`) and, once terminal, the payload (`tasks/result`).
//!
//! The store itself ([`TaskManager`], [`TaskHandle`], [`route_task_store`]) is
//! shared with the client side and lives in [`mcpkit_core::tasks`]; this
//! module re-exports it and adds the server-only [`TaskService`].

pub use mcpkit_core::tasks::{
    DEFAULT_TASK_TTL_MS, RELATED_TASK_META_KEY, TaskHandle, TaskManager, TaskPayload, TaskState,
    route_task_store,
};

use crate::context::Context;
use crate::handler::TaskHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::task::{CancelTaskResult, GetTaskResult, ListTasksResult, TaskId};
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
