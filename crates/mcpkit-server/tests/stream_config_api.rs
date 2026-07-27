//! `StreamConfig` must stay buildable from outside the crate.
//!
//! It is `#[non_exhaustive]`, so downstream code cannot use a struct literal —
//! including `..Default::default()`. This test lives in `tests/` for that
//! reason: `#[non_exhaustive]` does not apply within the defining crate, so an
//! in-crate test would compile a struct literal and prove nothing.

use mcpkit_server::streams::StreamConfig;
use std::time::Duration;

#[test]
fn every_field_is_reachable_through_a_setter() {
    let config = StreamConfig::new()
        .max_events_per_stream(7)
        .max_age(Duration::from_secs(11))
        .channel_capacity(3);

    assert_eq!(config.max_events_per_stream, 7);
    assert_eq!(config.max_age, Duration::from_secs(11));
    assert_eq!(config.channel_capacity, 3);
}

#[test]
fn new_agrees_with_default() {
    let a = StreamConfig::new();
    let b = StreamConfig::default();
    assert_eq!(a.max_events_per_stream, b.max_events_per_stream);
    assert_eq!(a.max_age, b.max_age);
    assert_eq!(a.channel_capacity, b.channel_capacity);
}
