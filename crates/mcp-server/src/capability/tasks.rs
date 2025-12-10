//! Task capability implementation.
//!
//! This module provides full support for long-running tasks
//! in MCP servers - a feature missing from rmcp.
//!
//! Tasks allow servers to execute long-running operations while
//! providing progress updates and supporting cancellation.

use crate::context::CancellationToken;
use crate::context::Context;
use crate::handler::TaskHandler;
use mcp_core::error::McpError;
use mcp_core::types::task::{Task, TaskId, TaskStatus};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Internal state for a running task.
#[derive(Debug)]
pub struct TaskState {
    /// Task metadata.
    pub task: Task,
    /// Cancellation token.
    pub cancel_token: CancellationToken,
    /// When the task was last accessed (for cleanup).
    pub last_access: Instant,
}

impl TaskState {
    /// Create a new task state.
    fn new(task: Task) -> Self {
        Self {
            task,
            cancel_token: CancellationToken::new(),
            last_access: Instant::now(),
        }
    }

    /// Check if the task is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

/// Handle for interacting with a running task.
///
/// This handle is given to task executors to report progress
/// and completion.
pub struct TaskHandle {
    task_id: TaskId,
    manager: Arc<TaskManager>,
}

impl TaskHandle {
    /// Get the task ID.
    pub fn id(&self) -> &TaskId {
        &self.task_id
    }

    /// Report that the task is now running.
    pub async fn running(&self) -> Result<(), McpError> {
        self.manager.update_status(&self.task_id, TaskStatus::Running).await
    }

    /// Report progress on the task.
    pub async fn progress(
        &self,
        current: u64,
        total: Option<u64>,
        message: Option<&str>,
    ) -> Result<(), McpError> {
        self.manager
            .update_progress(&self.task_id, current, total, message)
            .await
    }

    /// Mark the task as completed with a result.
    pub async fn complete(&self, result: Value) -> Result<(), McpError> {
        self.manager.complete_success(&self.task_id, result).await
    }

    /// Mark the task as failed with an error.
    pub async fn error(&self, message: impl Into<String>) -> Result<(), McpError> {
        self.manager.complete_error(&self.task_id, message.into()).await
    }

    /// Check if the task has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.manager
            .get(&self.task_id)
            .map(|s| s.is_cancelled())
            .unwrap_or(true)
    }

    /// Get a future that completes when the task is cancelled.
    pub async fn cancelled(&self) {
        if let Some(state) = self.manager.get(&self.task_id) {
            state.cancel_token.cancelled().await;
        }
    }
}

/// Manager for coordinating tasks.
///
/// This manages the lifecycle of tasks, including creation,
/// progress tracking, cancellation, and cleanup.
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
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new task.
    pub fn create(self: &Arc<Self>, tool_name: Option<&str>) -> TaskHandle {
        let mut task = Task::create();
        task.tool = tool_name.map(String::from);

        let task_id = task.id.clone();
        let state = TaskState::new(task);

        if let Ok(mut tasks) = self.tasks.write() {
            tasks.insert(task_id.clone(), state);
        }

        TaskHandle {
            task_id,
            manager: Arc::clone(self),
        }
    }

    /// Get a task state by ID.
    pub fn get(&self, id: &TaskId) -> Option<TaskState> {
        self.tasks.read().ok()?.get(id).map(|s| TaskState {
            task: s.task.clone(),
            cancel_token: s.cancel_token.clone(),
            last_access: s.last_access,
        })
    }

    /// List all tasks.
    pub fn list(&self) -> Vec<Task> {
        self.tasks
            .read()
            .map(|tasks| tasks.values().map(|s| s.task.clone()).collect())
            .unwrap_or_default()
    }

    /// Cancel a task.
    pub fn cancel(&self, id: &TaskId) -> Result<(), McpError> {
        let mut tasks = self.tasks.write().map_err(|_| {
            McpError::internal("Failed to acquire task lock")
        })?;

        if let Some(state) = tasks.get_mut(id) {
            state.cancel_token.cancel();
            state.task.status = TaskStatus::Cancelled;
            state.task.updated_at = chrono::Utc::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/cancel",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Update task status.
    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<(), McpError> {
        let mut tasks = self.tasks.write().map_err(|_| {
            McpError::internal("Failed to acquire task lock")
        })?;

        if let Some(state) = tasks.get_mut(id) {
            state.task.status = status;
            state.task.updated_at = chrono::Utc::now();
            state.last_access = Instant::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Update task progress.
    async fn update_progress(
        &self,
        id: &TaskId,
        current: u64,
        total: Option<u64>,
        message: Option<&str>,
    ) -> Result<(), McpError> {
        let mut tasks = self.tasks.write().map_err(|_| {
            McpError::internal("Failed to acquire task lock")
        })?;

        if let Some(state) = tasks.get_mut(id) {
            state.task.progress = Some(mcp_core::types::task::TaskProgress {
                current,
                total,
                message: message.map(String::from),
            });
            state.task.updated_at = chrono::Utc::now();
            state.last_access = Instant::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Complete a task with success.
    async fn complete_success(&self, id: &TaskId, result: Value) -> Result<(), McpError> {
        let mut tasks = self.tasks.write().map_err(|_| {
            McpError::internal("Failed to acquire task lock")
        })?;

        if let Some(state) = tasks.get_mut(id) {
            state.task.status = TaskStatus::Completed;
            state.task.result = Some(result);
            state.task.updated_at = chrono::Utc::now();
            state.last_access = Instant::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Complete a task with an error.
    async fn complete_error(&self, id: &TaskId, message: String) -> Result<(), McpError> {
        let mut tasks = self.tasks.write().map_err(|_| {
            McpError::internal("Failed to acquire task lock")
        })?;

        if let Some(state) = tasks.get_mut(id) {
            state.task.status = TaskStatus::Failed;
            state.task.error = Some(mcp_core::types::task::TaskError {
                code: -1,
                message,
                data: None,
            });
            state.task.updated_at = chrono::Utc::now();
            state.last_access = Instant::now();
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "tasks/get",
                format!("Unknown task: {}", id.as_str()),
            ))
        }
    }

    /// Remove completed tasks older than the given duration.
    pub fn cleanup(&self, max_age: std::time::Duration) {
        if let Ok(mut tasks) = self.tasks.write() {
            tasks.retain(|_, state| {
                let is_terminal = state.task.status.is_terminal();
                !is_terminal || state.last_access.elapsed() < max_age
            });
        }
    }
}

/// Task service implementing the TaskHandler trait.
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
    pub fn new() -> Self {
        Self {
            manager: Arc::new(TaskManager::new()),
        }
    }

    /// Get the underlying task manager.
    pub fn manager(&self) -> &Arc<TaskManager> {
        &self.manager
    }

    /// Create a new task and get a handle for it.
    pub fn create(&self, tool_name: Option<&str>) -> TaskHandle {
        self.manager.create(tool_name)
    }
}

impl TaskHandler for TaskService {
    async fn list_tasks(&self, _ctx: &Context<'_>) -> Result<Vec<Task>, McpError> {
        Ok(self.manager.list())
    }

    async fn get_task(&self, task_id: &TaskId, _ctx: &Context<'_>) -> Result<Option<Task>, McpError> {
        Ok(self.manager.get(task_id).map(|s| s.task))
    }

    async fn cancel_task(&self, task_id: &TaskId, _ctx: &Context<'_>) -> Result<bool, McpError> {
        match self.manager.cancel(task_id) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_manager() {
        let manager = Arc::new(TaskManager::new());

        let handle = manager.create(Some("test-tool"));
        assert!(!handle.is_cancelled());

        let tasks = manager.list();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].tool.as_deref(), Some("test-tool"));
        assert_eq!(tasks[0].status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_task_lifecycle() {
        let manager = Arc::new(TaskManager::new());

        let handle = manager.create(Some("processor"));
        let task_id = handle.id().clone();

        // Start running
        handle.running().await.unwrap();
        let state = manager.get(&task_id).unwrap();
        assert_eq!(state.task.status, TaskStatus::Running);

        // Report progress
        handle.progress(50, Some(100), Some("Halfway done")).await.unwrap();
        let state = manager.get(&task_id).unwrap();
        assert_eq!(state.task.progress.as_ref().map(|p| p.current), Some(50));

        // Complete
        handle.complete(serde_json::json!({"result": "success"})).await.unwrap();
        let state = manager.get(&task_id).unwrap();
        assert_eq!(state.task.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_cancellation() {
        let manager = Arc::new(TaskManager::new());

        let handle = manager.create(None);
        let task_id = handle.id().clone();

        assert!(!handle.is_cancelled());

        manager.cancel(&task_id).unwrap();

        assert!(handle.is_cancelled());
        let state = manager.get(&task_id).unwrap();
        assert_eq!(state.task.status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_task_service() {
        let service = TaskService::new();

        let handle = service.create(Some("service-task"));
        handle.running().await.unwrap();

        let tasks = service.manager.list();
        assert_eq!(tasks.len(), 1);
    }
}
