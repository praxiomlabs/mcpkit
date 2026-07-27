//! The crate's `#[non_exhaustive]` config types must stay buildable from
//! outside the crate.
//!
//! These live in `tests/` deliberately: `#[non_exhaustive]` does not apply
//! within the defining crate, so an in-crate test would compile a struct
//! literal and prove nothing about what downstream code can do.

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

use mcpkit_core::auth::AuthorizationConfig;
use mcpkit_core::extension::apps::AppsConfig;

#[test]
fn authorization_config_is_buildable_externally() {
    let config = AuthorizationConfig::new("https://auth.example.com")
        .with_client_id("client-abc")
        .with_scope("mcp:read");

    assert_eq!(config.authorization_server, "https://auth.example.com");
    assert_eq!(config.client_id, "client-abc");
    assert!(config.scopes.iter().any(|s| s == "mcp:read"));
}

#[test]
fn apps_config_is_buildable_externally() {
    // `ui_resources` had no setter before this type became non-exhaustive,
    // which would have left it unreachable from downstream code.
    let config = AppsConfig::new()
        .with_ui_resources(false)
        .with_max_content_size(4096)
        .with_sandbox_permissions(vec!["allow-scripts".to_string()]);

    assert!(!config.ui_resources);
    assert_eq!(config.max_content_size, Some(4096));
    assert_eq!(
        config.sandbox_permissions,
        vec!["allow-scripts".to_string()]
    );
}
