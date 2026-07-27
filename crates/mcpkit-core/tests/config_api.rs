//! The crate's `#[non_exhaustive]` config types must stay buildable from
//! outside the crate.
//!
//! These live in `tests/` deliberately: `#[non_exhaustive]` does not apply
//! within the defining crate, so an in-crate test would compile a struct
//! literal and prove nothing about what downstream code can do.

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
