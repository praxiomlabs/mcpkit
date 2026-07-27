//! Spec-defined method and notification names.
//!
//! These live in core because they are protocol facts, not server facts. They
//! were previously declared only in `mcpkit-server`, which left every other
//! crate — core, client, testing, the examples — writing the names as string
//! literals. That is how three separate places shipped a non-spec
//! `"initialized"`, and how the debug validator registered it as a known method.
//!
//! Use these at any site that **sends** or **matches** a method name. Do not
//! use them in tests that assert what the wire name is: an assertion against
//! the constant passes even when the constant is wrong, which is precisely how
//! those defects stayed green. Tests should spell the literal out.
//!
//! Completeness is enforced against the vendored schema — see
//! `mcpkit/tests/protocol_behaviour_conformance.rs`.

// ---------------------------------------------------------------------------
// Request methods (20 in 2025-11-25)
// ---------------------------------------------------------------------------

/// Initialize the connection and negotiate capabilities.
pub const INITIALIZE: &str = "initialize";
/// Ping to check if the connection is alive.
pub const PING: &str = "ping";

/// List available tools.
pub const TOOLS_LIST: &str = "tools/list";
/// Call a specific tool with arguments.
pub const TOOLS_CALL: &str = "tools/call";

/// List available resources.
pub const RESOURCES_LIST: &str = "resources/list";
/// Read the contents of a resource.
pub const RESOURCES_READ: &str = "resources/read";
/// List available resource templates.
pub const RESOURCES_TEMPLATES_LIST: &str = "resources/templates/list";
/// Subscribe to resource updates.
pub const RESOURCES_SUBSCRIBE: &str = "resources/subscribe";
/// Unsubscribe from resource updates.
pub const RESOURCES_UNSUBSCRIBE: &str = "resources/unsubscribe";

/// List available prompts.
pub const PROMPTS_LIST: &str = "prompts/list";
/// Get a specific prompt with arguments.
pub const PROMPTS_GET: &str = "prompts/get";

/// List running tasks.
pub const TASKS_LIST: &str = "tasks/list";
/// Get status of a specific task.
pub const TASKS_GET: &str = "tasks/get";
/// Cancel a running task.
pub const TASKS_CANCEL: &str = "tasks/cancel";
/// Retrieve a terminal task's payload (blocks until terminal, per spec).
pub const TASKS_RESULT: &str = "tasks/result";

/// List the roots the client exposes.
pub const ROOTS_LIST: &str = "roots/list";

/// Request the client to sample from a language model.
pub const SAMPLING_CREATE_MESSAGE: &str = "sampling/createMessage";

/// Request completion suggestions.
pub const COMPLETION_COMPLETE: &str = "completion/complete";

/// Set the logging level.
pub const LOGGING_SET_LEVEL: &str = "logging/setLevel";

/// Create an elicitation request.
pub const ELICITATION_CREATE: &str = "elicitation/create";

// ---------------------------------------------------------------------------
// Notification methods (11 in 2025-11-25)
// ---------------------------------------------------------------------------

/// Spec-defined notification method names.
pub mod notifications {
    /// Sent by client after successful initialization.
    pub const INITIALIZED: &str = "notifications/initialized";
    /// Sent when a request is cancelled.
    pub const CANCELLED: &str = "notifications/cancelled";
    /// Sent to report progress on a long-running operation.
    pub const PROGRESS: &str = "notifications/progress";
    /// Sent to deliver a log message.
    pub const MESSAGE: &str = "notifications/message";
    /// Sent when a resource's content has changed.
    pub const RESOURCES_UPDATED: &str = "notifications/resources/updated";
    /// Sent when the list of available resources has changed.
    pub const RESOURCES_LIST_CHANGED: &str = "notifications/resources/list_changed";
    /// Sent when the list of available tools has changed.
    pub const TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";
    /// Sent when the list of available prompts has changed.
    pub const PROMPTS_LIST_CHANGED: &str = "notifications/prompts/list_changed";
    /// Sent by the client when its list of roots has changed.
    pub const ROOTS_LIST_CHANGED: &str = "notifications/roots/list_changed";
    /// Sent when a URL-mode elicitation's out-of-band interaction has completed.
    pub const ELICITATION_COMPLETE: &str = "notifications/elicitation/complete";
    /// Sent by a task receiver when a task changes status. Optional per spec:
    /// the requesting peer must not rely on receiving it.
    pub const TASK_STATUS: &str = "notifications/tasks/status";
}
