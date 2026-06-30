//! Task capability implementation.
//!
//! Tasks let a server run a long-running operation while the caller polls for
//! status (`tasks/get`) and, once terminal, the payload (`tasks/result`).

use crate::context::CancellationToken;
use crate::context::Context;
use crate::handler::TaskHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::task::{Task, TaskId, TaskStatus};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Internal state for a tracked task.
#[derive(Debug, Clone)]
pub struct TaskState {
    /// Task metadata (status, timestamps, ttl).
    pub task: Task,
    /// The eventual payload, available once the task is `completed`
    /// (returned by `tasks/result`).
    pub payload: Option<Value>,
    /// Cancellation token.
    pub cancel_token: CancellationToken,
    /// When the task was last accessed (for cleanup).
    pub last_access: Instant,
}

impl TaskState {
    fn new(task: Task) -> Self {
        Self {
            task,
            payload: None,
            cancel_token: CancellationToken::new(),
            last_access: Instant::now(),
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
        self.manager.complete(&self.task_id, payload)
    }

    /// Mark the task `failed` with a status message.
    pub fn fail(&self, message: impl Into<String>) -> Result<(), McpError> {
        self.manager
            .set_status(&self.task_id, TaskStatus::Failed, Some(message.into()))
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

/// Manager coordinating the lifecycle of tracked tasks.
pub struct TaskManager {
    tasks: RwLock<HashMap<TaskId, TaskState>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    /// Create a new task manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new `working` task and return a handle to it.
    pub fn create(self: &Arc<Self>, ttl: Option<u64>) -> TaskHandle {
        let mut task = Task::create();
        task.ttl = ttl;
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

    /// Get the payload of a completed task, if available.
    #[must_use]
    pub fn payload(&self, id: &TaskId) -> Option<Value> {
        self.tasks.read().ok()?.get(id)?.payload.clone()
    }

    /// Cancel a task.
    pub fn cancel(&self, id: &TaskId) -> Result<(), McpError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| McpError::internal("Failed to acquire task lock"))?;

        if let Some(state) = tasks.get_mut(id) {
            state.cancel_token.cancel();
            state.task.set_status(TaskStatus::Cancelled);
            state.last_access = Instant::now();
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
            state.task.set_status(status);
            if message.is_some() {
                state.task.status_message = message;
            }
            state.last_access = Instant::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Mark a task completed and store its payload.
    fn complete(&self, id: &TaskId, payload: Value) -> Result<(), McpError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| McpError::internal("Failed to acquire task lock"))?;

        if let Some(state) = tasks.get_mut(id) {
            state.task.set_status(TaskStatus::Completed);
            state.payload = Some(payload);
            state.last_access = Instant::now();
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
}

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
    async fn list_tasks(&self, _ctx: &Context<'_>) -> Result<Vec<Task>, McpError> {
        Ok(self.manager.list())
    }

    async fn get_task(
        &self,
        task_id: &TaskId,
        _ctx: &Context<'_>,
    ) -> Result<Option<Task>, McpError> {
        Ok(self.manager.get(task_id).map(|s| s.task))
    }

    async fn cancel_task(&self, task_id: &TaskId, _ctx: &Context<'_>) -> Result<bool, McpError> {
        Ok(self.manager.cancel(task_id).is_ok())
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
        assert_eq!(
            manager.payload(&task_id),
            Some(serde_json::json!({"result": "ok"}))
        );
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
