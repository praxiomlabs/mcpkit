//! Common types and utilities for multi-service example.

#![allow(dead_code)]

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
