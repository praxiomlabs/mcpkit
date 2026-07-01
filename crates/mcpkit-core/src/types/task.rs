//! Task types for the MCP 2025-11-25 tasks utility.
//!
//! A task represents a long-running operation. A receiver that supports
//! task-augmented execution returns a [`CreateTaskResult`] immediately, and the
//! caller polls [`tasks/get`](GetTaskRequest) for status and
//! [`tasks/result`](GetTaskPayloadRequest) for the eventual payload.

use super::meta::Meta;
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
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// The request is currently being processed.
    Working,
    /// The task is waiting for input (e.g. an elicitation or sampling round-trip).
    InputRequired,
    /// The request completed successfully and the result is available.
    Completed,
    /// The request did not complete successfully.
    Failed,
    /// The request was cancelled before completion.
    Cancelled,
}

impl TaskStatus {
    /// Check if the task is in a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::InputRequired => write!(f, "input_required"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Full state information for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// The task identifier.
    #[serde(rename = "taskId")]
    pub task_id: TaskId,
    /// Current task state.
    pub status: TaskStatus,
    /// Optional human-readable message describing the current state (e.g. a
    /// failure reason or completion summary).
    #[serde(rename = "statusMessage", skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// ISO 8601 timestamp when the task was created.
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// ISO 8601 timestamp when the task was last updated.
    #[serde(rename = "lastUpdatedAt")]
    pub last_updated_at: String,
    /// Actual retention duration from creation, in milliseconds; `None`
    /// (serialized as `null`) for unlimited.
    pub ttl: Option<u64>,
    /// Suggested polling interval, in milliseconds.
    #[serde(rename = "pollInterval", skip_serializing_if = "Option::is_none")]
    pub poll_interval: Option<u64>,
}

impl Task {
    /// Create a new `working` task with the given ID, timestamped now.
    #[must_use]
    pub fn new(task_id: TaskId) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            task_id,
            status: TaskStatus::Working,
            status_message: None,
            created_at: now.clone(),
            last_updated_at: now,
            ttl: None,
            poll_interval: None,
        }
    }

    /// Create a new `working` task with a generated ID.
    #[must_use]
    pub fn create() -> Self {
        Self::new(TaskId::generate())
    }

    /// Set the status message.
    #[must_use]
    pub fn status_message(mut self, message: impl Into<String>) -> Self {
        self.status_message = Some(message.into());
        self
    }

    /// Set the retention duration in milliseconds (`None` for unlimited).
    #[must_use]
    pub const fn ttl(mut self, ttl: Option<u64>) -> Self {
        self.ttl = ttl;
        self
    }

    /// Set the suggested polling interval in milliseconds.
    #[must_use]
    pub const fn poll_interval(mut self, poll_interval: u64) -> Self {
        self.poll_interval = Some(poll_interval);
        self
    }

    /// Transition the task to a new status and bump `lastUpdatedAt`.
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.last_updated_at = chrono::Utc::now().to_rfc3339();
    }
}

/// Metadata for requesting task-augmented execution.
///
/// Include this in the `task` field of a request's params to ask the receiver
/// to run the request as a task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Requested retention duration from creation, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
}

/// Metadata associating a message with a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedTaskMetadata {
    /// The task identifier this message is associated with.
    #[serde(rename = "taskId")]
    pub task_id: TaskId,
}

/// Result of a task-augmented request: the created task, returned immediately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskResult {
    /// The created task.
    pub task: Task,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

// NOTE: Per the spec, `tasks/get`/`tasks/cancel` results and the task-status
// notification are `Result & Task` / `NotificationParams & Task`, i.e. they may
// carry a result/notification-level `_meta`. That is intentionally *not* modeled
// here: the base `Task` has no `_meta` (adding one would leak `_meta` into nested
// `CreateTaskResult.task` and `ListTasksResult.tasks[]`), and the task handler API
// returns a bare `Task` with nowhere to supply result-level metadata. Modeling it
// would need dedicated result wrapper types + a handler-contract change — see
// issue #136. `CreateTaskResult`/`ListTasksResult` do carry their own
// result-level `_meta`.

/// Result of `tasks/get` — the task's current state (spec `Result & Task`).
pub type GetTaskResult = Task;

/// Result of `tasks/cancel` — the task's state after cancellation (`Result & Task`).
pub type CancelTaskResult = Task;

/// Result of `tasks/result` — the eventual payload of the augmented request
/// (e.g. the `CallToolResult`). An arbitrary result object.
pub type GetTaskPayloadResult = Value;

/// Request parameters for `tasks/list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksRequest {
    /// Cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for `tasks/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTasksResult {
    /// The list of tasks.
    pub tasks: Vec<Task>,
    /// Cursor for the next page, if more tasks exist.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// Request parameters for `tasks/get`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskRequest {
    /// The task identifier to query.
    #[serde(rename = "taskId")]
    pub task_id: TaskId,
}

/// Request parameters for `tasks/result`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskPayloadRequest {
    /// The task identifier to retrieve results for.
    #[serde(rename = "taskId")]
    pub task_id: TaskId,
}

/// Request parameters for `tasks/cancel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelTaskRequest {
    /// The task identifier to cancel.
    #[serde(rename = "taskId")]
    pub task_id: TaskId,
}

/// Parameters for a task status notification (spec `NotificationParams & Task`):
/// the task's current state.
pub type TaskStatusNotification = Task;

/// Task-domain progress information.
///
/// Note: the base-protocol `notifications/progress` payload is
/// [`ProgressNotificationParams`](crate::types::ProgressNotificationParams), not
/// this type.
///
/// Note: this is the progress utility's payload, not part of the spec `Task`
/// object (the 2025-11-25 `Task` has no embedded progress). It is retained for
/// the client's progress-notification handler and will be reconciled with the
/// typed `ProgressNotification` params in #112.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_serializes_with_spec_field_names() {
        let task = Task::new(TaskId::new("t-1"));
        let j = serde_json::to_value(&task).unwrap();
        assert_eq!(j["taskId"], "t-1");
        assert_eq!(j["status"], "working");
        // ttl is required + nullable -> present as null when unset.
        assert!(j.get("ttl").is_some() && j["ttl"].is_null());
        // Optional fields absent when unset.
        assert!(j.get("statusMessage").is_none());
        assert!(j.get("pollInterval").is_none());
        assert!(j["createdAt"].is_string());
        assert!(j["lastUpdatedAt"].is_string());
    }

    #[test]
    fn task_status_serde_values() {
        assert_eq!(
            serde_json::to_value(TaskStatus::Working).unwrap(),
            serde_json::json!("working")
        );
        assert_eq!(
            serde_json::to_value(TaskStatus::InputRequired).unwrap(),
            serde_json::json!("input_required")
        );
        assert!(TaskStatus::Completed.is_terminal());
        assert!(!TaskStatus::Working.is_terminal());
    }

    #[test]
    fn set_status_updates_timestamp() {
        let mut task = Task::new(TaskId::new("t"));
        let created = task.last_updated_at.clone();
        task.set_status(TaskStatus::Completed);
        assert_eq!(task.status, TaskStatus::Completed);
        // created_at is stable; last_updated_at is refreshed.
        assert_eq!(task.created_at, created);
    }

    #[test]
    fn create_task_result_wraps_task() {
        let res = CreateTaskResult {
            task: Task::new(TaskId::new("abc")),
            meta: None,
        };
        let j = serde_json::to_value(&res).unwrap();
        assert_eq!(j["task"]["taskId"], "abc");
    }

    #[test]
    fn requests_use_task_id_field() {
        let j = serde_json::to_value(GetTaskRequest {
            task_id: TaskId::new("x"),
        })
        .unwrap();
        assert_eq!(j["taskId"], "x");
    }
}
