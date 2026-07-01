//! `#[mcp_client]` wires `#[elicit_url]` and `#[on_elicitation_complete]` to the
//! `ClientHandler` methods (rather than silently using the defaults).

use mcpkit::client::ClientHandler;
use mcpkit::error::McpError;
use mcpkit::mcp_client;
use mcpkit::types::{ElicitResult, UrlElicitRequest};
use std::sync::atomic::{AtomicBool, Ordering};

struct H {
    completed: AtomicBool,
}

#[mcp_client]
impl H {
    // A real client would show the URL's domain, get consent, and open it.
    #[elicit_url]
    async fn on_url(&self, _request: UrlElicitRequest) -> Result<ElicitResult, McpError> {
        Ok(ElicitResult::accepted(serde_json::Map::new()))
    }

    #[on_elicitation_complete]
    async fn on_done(&self, _elicitation_id: String) {
        self.completed.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn client_wires_url_elicitation_handlers() {
    let handler = H {
        completed: AtomicBool::new(false),
    };

    // `elicit_url` dispatches to the user method (which accepts) rather than the
    // trait default (which declines).
    let result = handler
        .elicit_url(UrlElicitRequest::new("authorize", "e1", "https://auth/x"))
        .await
        .expect("elicit_url");
    assert!(
        result.is_accepted(),
        "the #[elicit_url] method must be wired, not the default decline"
    );

    // `on_elicitation_complete` dispatches to the user method.
    handler.on_elicitation_complete("e1".to_string()).await;
    assert!(handler.completed.load(Ordering::SeqCst));
}
