//! `RuntimeConfig` must stay buildable from *outside* the crate.
//!
//! It is `#[non_exhaustive]`, so downstream code cannot use a struct literal —
//! including functional-update syntax. That is deliberate (it makes future field
//! additions non-breaking), but it only works if the setters cover every field.
//! An in-crate test cannot check this: `#[non_exhaustive]` does not apply within
//! the defining crate, so a struct literal there would compile and prove nothing.

use mcpkit_server::RuntimeConfig;
use std::time::Duration;

#[test]
fn every_field_is_reachable_through_a_setter() {
    let config = RuntimeConfig::new()
        .auto_initialized(false)
        .max_concurrent_requests(7)
        .outbound_request_timeout(Duration::from_secs(3))
        .default_task_ttl_ms(None)
        .default_task_poll_interval_ms(Some(250))
        .task_status_notifications(false);

    assert!(!config.auto_initialized);
    assert_eq!(config.max_concurrent_requests, 7);
    assert_eq!(config.outbound_request_timeout, Duration::from_secs(3));
    assert_eq!(config.default_task_ttl_ms, None);
    assert_eq!(config.default_task_poll_interval_ms, Some(250));
    assert!(!config.task_status_notifications);
}

#[test]
fn defaults_are_unchanged_by_the_builder_rewrite() {
    let config = RuntimeConfig::default();
    assert!(config.auto_initialized);
    assert_eq!(config.max_concurrent_requests, 100);
    assert_eq!(config.outbound_request_timeout, Duration::from_secs(60));
    assert!(config.default_task_ttl_ms.is_some());
    assert!(config.task_status_notifications);
    // `new()` is the documented entry point and must agree with `default()`.
    let via_new = RuntimeConfig::new();
    assert_eq!(
        via_new.max_concurrent_requests,
        config.max_concurrent_requests
    );
    assert_eq!(via_new.auto_initialized, config.auto_initialized);
}
