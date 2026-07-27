//! Common types and utilities for multi-service example.

#![allow(dead_code)]
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
#![allow(clippy::needless_continue)]
#![allow(clippy::match_wildcard_for_single_variants)]
#![allow(clippy::significant_drop_in_scrutinee)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::path_buf_push_overwrite)]
#![allow(clippy::unnecessary_debug_formatting)]

use serde::{Deserialize, Serialize};

/// Service registration information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name.
    pub name: String,
    /// Service endpoint URL.
    pub endpoint: String,
    /// Service capabilities.
    pub capabilities: Vec<String>,
}

/// Default ports for services.
pub mod ports {
    /// Gateway service port.
    pub const GATEWAY: u16 = 3000;
    /// Tools service port.
    pub const TOOLS: u16 = 3001;
    /// Resources service port.
    pub const RESOURCES: u16 = 3002;
}

/// Initialize tracing for a service.
pub fn init_tracing(service_name: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("{service_name}=info").parse().unwrap())
                .add_directive("mcpkit=debug".parse().unwrap()),
        )
        .init();
}
