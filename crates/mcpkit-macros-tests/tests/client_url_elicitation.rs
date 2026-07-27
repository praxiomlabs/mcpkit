//! `#[mcp_client]` wires `#[elicit_url]` and `#[on_elicitation_complete]` to the
//! `ClientHandler` methods (rather than silently using the defaults).

// Test / example code: assertion shapes, fixture naming and framework-shaped
// signatures are written for readability at the call site, not to satisfy
// pedantic/nursery lints. None of this ships in the library.
#![allow(clippy::similar_names)]
#![allow(clippy::redundant_else)]
#![allow(clippy::wildcard_enum_match_arm)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::unused_async)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::future_not_send)]
#![allow(clippy::type_complexity)]

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
