//! `StreamConfig` must stay buildable from outside the crate.
//!
//! It is `#[non_exhaustive]`, so downstream code cannot use a struct literal —
//! including `..Default::default()`. This test lives in `tests/` for that
//! reason: `#[non_exhaustive]` does not apply within the defining crate, so an
//! in-crate test would compile a struct literal and prove nothing.

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
