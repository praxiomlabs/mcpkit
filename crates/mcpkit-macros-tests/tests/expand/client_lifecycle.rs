//! Test: Client with lifecycle hooks expands correctly

use mcpkit::mcp_client;
use std::sync::atomic::{AtomicBool, Ordering};

struct LifecycleHandler {
    connected: AtomicBool,
}

#[mcp_client]
impl LifecycleHandler {
    #[on_connected]
    async fn handle_connected(&self) {
        self.connected.store(true, Ordering::SeqCst);
    }

    #[on_disconnected]
    async fn handle_disconnected(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }
}

fn main() {
    let handler = LifecycleHandler {
        connected: AtomicBool::new(false),
    };
    let _caps = handler.capabilities();
}
