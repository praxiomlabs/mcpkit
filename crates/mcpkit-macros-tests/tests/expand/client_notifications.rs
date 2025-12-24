//! Test: Client with notification handlers expands correctly

use mcpkit::mcp_client;
use mcpkit::types::{TaskId, TaskProgress};

struct NotificationHandler;

#[mcp_client]
impl NotificationHandler {
    #[on_task_progress]
    async fn handle_task_progress(&self, task_id: TaskId, progress: TaskProgress) {
        println!("Task {} progress: {:?}", task_id, progress);
    }

    #[on_resource_updated]
    async fn handle_resource_updated(&self, uri: String) {
        println!("Resource updated: {}", uri);
    }

    #[on_tools_list_changed]
    async fn handle_tools_changed(&self) {
        println!("Tools list changed");
    }

    #[on_resources_list_changed]
    async fn handle_resources_changed(&self) {
        println!("Resources list changed");
    }

    #[on_prompts_list_changed]
    async fn handle_prompts_changed(&self) {
        println!("Prompts list changed");
    }
}

fn main() {
    let handler = NotificationHandler;
    let _caps = handler.capabilities();
}
