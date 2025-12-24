//! Test: Full client with all handlers expands correctly

use mcpkit::mcp_client;
use mcpkit::types::{
    CreateMessageRequest, CreateMessageResult, ElicitRequest, ElicitResult,
    Role, TaskId, TaskProgress, Content, StopReason,
};
use mcpkit::client::handler::Root;
use mcpkit::error::McpError;
use std::sync::atomic::{AtomicBool, Ordering};

struct FullHandler {
    connected: AtomicBool,
    roots: Vec<Root>,
}

#[mcp_client]
impl FullHandler {
    // Sampling handler
    #[sampling]
    async fn handle_sampling(&self, _request: CreateMessageRequest) -> Result<CreateMessageResult, McpError> {
        Ok(CreateMessageResult {
            model: "gpt-4".to_string(),
            role: Role::Assistant,
            content: Content::text("Response"),
            stop_reason: Some(StopReason::EndTurn),
        })
    }

    // Elicitation handler
    #[elicitation]
    async fn handle_elicitation(&self, _request: ElicitRequest) -> Result<ElicitResult, McpError> {
        Ok(ElicitResult::declined())
    }

    // Roots handler
    #[roots]
    async fn list_roots(&self) -> Result<Vec<Root>, McpError> {
        Ok(self.roots.clone())
    }

    // Lifecycle hooks
    #[on_connected]
    async fn on_connected(&self) {
        self.connected.store(true, Ordering::SeqCst);
    }

    #[on_disconnected]
    async fn on_disconnected(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    // Notification handlers
    #[on_task_progress]
    async fn on_task_progress(&self, _task_id: TaskId, _progress: TaskProgress) {}

    #[on_resource_updated]
    async fn on_resource_updated(&self, _uri: String) {}

    #[on_tools_list_changed]
    async fn on_tools_changed(&self) {}

    #[on_resources_list_changed]
    async fn on_resources_changed(&self) {}

    #[on_prompts_list_changed]
    async fn on_prompts_changed(&self) {}
}

fn main() {
    let handler = FullHandler {
        connected: AtomicBool::new(false),
        roots: vec![],
    };

    // Should have all capabilities enabled
    let caps = handler.capabilities();
    assert!(caps.sampling.is_some());
    assert!(caps.elicitation.is_some());
    assert!(caps.roots.is_some());
}
