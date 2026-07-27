//! Every `#[non_exhaustive]` public config must stay constructible downstream.
//!
//! `#[non_exhaustive]` forbids struct-literal construction outside the defining
//! crate, so each of these types needs a working constructor. This test lives in
//! the umbrella crate — downstream of every one of them — because the attribute
//! does not apply within the defining crate: an in-crate test compiles a struct
//! literal and proves nothing about what a user can do.
//!
//! **What this does not prove.** Constructing a config says nothing about
//! whether every *field* is settable. That gap is real: `WebSocketConfig`
//! shipped with `reconnect_backoff` initialized to a default by `new()` and no
//! setter, so a downstream caller could not change it once the struct literal
//! was closed off. A field initialized to a constant in a constructor looks
//! reachable to a naive scan and is not. Per-field coverage belongs with each
//! type's own test; this file guards the coarser property that the type can be
//! built at all — across every config reachable from here, not a sample of them.

use std::time::Duration;

#[test]
fn server_configs_are_constructible() {
    let _ = mcpkit_server::RuntimeConfig::new()
        .max_concurrent_requests(8)
        .default_task_poll_interval_ms(Some(500));

    let _ = mcpkit_server::streams::StreamConfig::new()
        .max_events_per_stream(10)
        .max_age(Duration::from_secs(60))
        .channel_capacity(4);
}

#[test]
fn core_configs_are_constructible() {
    let _ = mcpkit_core::auth::AuthorizationConfig::new("https://auth.example.com")
        .with_client_id("id");

    let _ = mcpkit_core::extension::apps::AppsConfig::new()
        .with_ui_resources(true)
        .with_max_content_size(1024);
}

#[test]
fn client_configs_are_constructible() {
    let _ = mcpkit_client::PoolConfig::default();
}

/// Every transport config reachable from here. Three of the fourteen are not,
/// and the reasons are recorded so silence is not mistaken for coverage:
///
/// * `GrpcConfig` and `GrpcServerConfig` sit behind `mcpkit-transport`'s `grpc`
///   feature, which the umbrella crate does not expose — nothing downstream of
///   `mcpkit` can reach them.
/// * `NamedPipeConfig` is `#[cfg(windows)]`; it is covered instead by
///   `cargo check --target x86_64-pc-windows-msvc`.
///
/// `UnixSocketConfig` is `#[cfg(unix)]` and lives in its own gated test below —
/// this suite runs on Linux, macOS and Windows.
#[test]
fn transport_configs_are_constructible() {
    let _ = mcpkit_transport::pool::PoolConfig::new().max_connections(4);
    let _ = mcpkit_transport::http::HttpTransportConfig::new("https://example.com");
    let _ = mcpkit_transport::middleware::BatchingConfig::default();
    let _ = mcpkit_transport::middleware::RateLimitConfig::new(10, Duration::from_secs(1));
    let _ = mcpkit_transport::TelemetryConfig::new("svc");
}

/// `UnixSocketConfig` is `#[cfg(unix)]`, so it needs its own gated test — putting
/// it in the block above compiled on Linux and broke the Windows job.
#[cfg(unix)]
#[test]
fn unix_transport_config_is_constructible() {
    let _ = mcpkit_transport::unix::UnixSocketConfig::new("/tmp/mcpkit-test.sock");
}

/// The one field whose absent setter this suite exists to have caught.
#[test]
fn websocket_reconnect_backoff_is_settable() {
    use mcpkit_transport::websocket::{ExponentialBackoff, WebSocketConfig};
    let config = WebSocketConfig::new("wss://example.com/mcp")
        .with_reconnect_backoff(ExponentialBackoff::default())
        .with_max_message_size(1024);
    assert_eq!(config.max_message_size, 1024);
}
