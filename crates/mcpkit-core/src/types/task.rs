//! Task types for MCP servers.
//!
//! Tasks represent long-running operations that can be tracked, monitored,
//! and cancelled.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unique identifier for a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub String);

impl TaskId {
    /// Create a new task ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random task ID.
    #[must_use]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Get the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TaskId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TaskId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// The current status of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// Task is pending execution.
    Pending,
    /// Task is currently running.
    Running,
    /// Task completed successfully.
    Completed,
    /// Task failed with an error.
    Failed,
    /// Task was cancelled.
    Cancelled,
}

impl TaskStatus {
    /// Check if the task is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Check if the task is actively running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Check if the task is pending.
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Progress information for a running task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    /// Current progress value.
    pub current: u64,
    /// Total progress value (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Human-readable progress message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TaskProgress {
    /// Create new progress information.
    #[must_use]
    pub const fn new(current: u64) -> Self {
        Self {
            current,
            total: None,
            message: None,
        }
    }

    /// Set the total progress value.
    #[must_use]
    pub const fn total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    /// Set the progress message.
    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Get the progress percentage (0.0 to 1.0) if total is known.
    #[must_use]
    pub fn percentage(&self) -> Option<f64> {
        self.total.map(|t| {
            if t == 0 {
                1.0
            } else {
                (self.current as f64 / t as f64).min(1.0)
            }
        })
    }
}

/// Error information for a failed task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskError {
    /// Error code.
    pub code: i32,
    /// Error message.
    pub message: String,
    /// Additional error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl TaskError {
    /// Create a new task error.
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Set additional error data.
    #[must_use]
    pub fn data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Full state information for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: TaskId,
    /// Current task status.
    pub status: TaskStatus,
    /// Name of the tool that created this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Progress information (for running tasks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    /// Result data (for completed tasks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error information (for failed tasks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TaskError>,
    /// Timestamp when the task was created.
    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp when the task was last updated.
    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Task {
    /// Create a new pending task.
    #[must_use]
    pub fn new(id: TaskId) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            status: TaskStatus::Pending,
            tool: None,
            description: None,
            progress: None,
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new task with a generated ID.
    #[must_use]
    pub fn create() -> Self {
        Self::new(TaskId::generate())
    }

    /// Set the tool name.
    #[must_use]
    pub fn tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Mark the task as running.
    pub fn start(&mut self) {
        self.status = TaskStatus::Running;
        self.updated_at = chrono::Utc::now();
    }

    /// Update task progress.
    pub fn update_progress(&mut self, progress: TaskProgress) {
        self.progress = Some(progress);
        self.updated_at = chrono::Utc::now();
    }

    /// Complete the task with a result.
    pub fn complete(&mut self, result: Value) {
        self.status = TaskStatus::Completed;
        self.result = Some(result);
        self.progress = None;
        self.updated_at = chrono::Utc::now();
    }

    /// Fail the task with an error.
    pub fn fail(&mut self, error: TaskError) {
        self.status = TaskStatus::Failed;
        self.error = Some(error);
        self.progress = None;
        self.updated_at = chrono::Utc::now();
    }

    /// Cancel the task.
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.progress = None;
        self.updated_at = chrono::Utc::now();
    }
}

/// A summary of a task (for listing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Unique task identifier.
    pub id: TaskId,
    /// Current task status.
    pub status: TaskStatus,
    /// Name of the tool that created this task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Progress percentage (0.0 to 1.0) if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
}

impl From<&Task> for TaskSummary {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            status: task.status,
            tool: task.tool.clone(),
            description: task.description.clone(),
            progress: task.progress.as_ref().and_then(TaskProgress::percentage),
        }
    }
}

/// Request parameters for listing tasks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksRequest {
    /// Filter by status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskStatus>,
    /// Cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for listing tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTasksResult {
    /// The list of tasks.
    pub tasks: Vec<TaskSummary>,
    /// Cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Request parameters for getting a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskRequest {
    /// ID of the task to get.
    pub id: TaskId,
}

/// Request parameters for cancelling a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    /// ID of the task to cancel.
    pub id: TaskId,
}

/// Notification that a task's status has changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusNotification {
    /// ID of the task.
    pub id: TaskId,
    /// New status.
    pub status: TaskStatus,
    /// Progress information (if running).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    /// Result (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TaskError>,
}

impl TaskStatusNotification {
    /// Create a running notification.
    #[must_use]
    pub const fn running(id: TaskId) -> Self {
        Self {
            id,
            status: TaskStatus::Running,
            progress: None,
            result: None,
            error: None,
        }
    }

    /// Create a progress notification.
    #[must_use]
    pub const fn progress(id: TaskId, progress: TaskProgress) -> Self {
        Self {
            id,
            status: TaskStatus::Running,
            progress: Some(progress),
            result: None,
            error: None,
        }
    }

    /// Create a completed notification.
    #[must_use]
    pub const fn completed(id: TaskId, result: Value) -> Self {
        Self {
            id,
            status: TaskStatus::Completed,
            progress: None,
            result: Some(result),
            error: None,
        }
    }

    /// Create a failed notification.
    #[must_use]
    pub const fn failed(id: TaskId, error: TaskError) -> Self {
        Self {
            id,
            status: TaskStatus::Failed,
            progress: None,
            result: None,
            error: Some(error),
        }
    }

    /// Create a cancelled notification.
    #[must_use]
    pub const fn cancelled(id: TaskId) -> Self {
        Self {
            id,
            status: TaskStatus::Cancelled,
            progress: None,
            result: None,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_lifecycle() {
        let mut task = Task::create().tool("analyze").description("Analyzing data");

        assert_eq!(task.status, TaskStatus::Pending);
        assert!(!task.status.is_terminal());

        task.start();
        assert_eq!(task.status, TaskStatus::Running);
        assert!(task.status.is_running());

        task.update_progress(TaskProgress::new(50).total(100).message("Halfway done"));
        assert_eq!(task.progress.as_ref().unwrap().percentage(), Some(0.5));

        task.complete(serde_json::json!({"result": "success"}));
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.status.is_terminal());
        assert!(task.result.is_some());
    }

    #[test]
    fn test_task_failure() {
        let mut task = Task::create();
        task.start();
        task.fail(
            TaskError::new(-1, "Something went wrong")
                .data(serde_json::json!({"details": "error"})),
        );

        assert_eq!(task.status, TaskStatus::Failed);
        assert!(task.status.is_terminal());
        assert!(task.error.is_some());
    }

    #[test]
    fn test_task_cancellation() {
        let mut task = Task::create();
        task.start();
        task.cancel();

        assert_eq!(task.status, TaskStatus::Cancelled);
        assert!(task.status.is_terminal());
    }

    #[test]
    fn test_task_summary() {
        let mut task = Task::create()
            .tool("process")
            .description("Processing files");
        task.start();
        task.update_progress(TaskProgress::new(25).total(100));

        let summary: TaskSummary = (&task).into();
        assert_eq!(summary.id, task.id);
        assert_eq!(summary.status, TaskStatus::Running);
        assert_eq!(summary.progress, Some(0.25));
    }

    #[test]
    fn test_progress_percentage() {
        let progress = TaskProgress::new(0).total(100);
        assert_eq!(progress.percentage(), Some(0.0));

        let progress = TaskProgress::new(100).total(100);
        assert_eq!(progress.percentage(), Some(1.0));

        let progress = TaskProgress::new(150).total(100);
        assert_eq!(progress.percentage(), Some(1.0)); // Clamped to 1.0

        let progress = TaskProgress::new(50);
        assert_eq!(progress.percentage(), None); // No total

        let progress = TaskProgress::new(0).total(0);
        assert_eq!(progress.percentage(), Some(1.0)); // Division by zero handled
    }

    #[test]
    fn test_task_notifications() {
        let id = TaskId::generate();

        let running = TaskStatusNotification::running(id.clone());
        assert_eq!(running.status, TaskStatus::Running);

        let progress =
            TaskStatusNotification::progress(id.clone(), TaskProgress::new(50).total(100));
        assert!(progress.progress.is_some());

        let completed =
            TaskStatusNotification::completed(id, serde_json::json!({"data": "result"}));
        assert_eq!(completed.status, TaskStatus::Completed);
        assert!(completed.result.is_some());
    }
}
