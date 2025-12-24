//! Health check utilities for MCP servers.
//!
//! This module provides a standardized health check mechanism for MCP servers,
//! supporting both simple and detailed health status reporting.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::health::{HealthChecker, HealthStatus, ComponentHealth};
//!
//! // Create a health checker
//! let mut checker = HealthChecker::new("my-mcp-server");
//!
//! // Add component checks
//! checker.add_check("database", || {
//!     // Your database health check logic
//!     ComponentHealth::healthy()
//! });
//!
//! checker.add_check("cache", || {
//!     ComponentHealth::healthy().with_detail("hit_rate", "95%")
//! });
//!
//! // Get overall health status
//! let status = checker.check();
//! assert!(status.is_healthy());
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Overall health status of the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// All components are healthy.
    Healthy,
    /// Some components are degraded but the service is functional.
    Degraded,
    /// The service is unhealthy and may not function correctly.
    Unhealthy,
}

impl HealthStatus {
    /// Check if the status is healthy.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Check if the status is degraded.
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        matches!(self, Self::Degraded)
    }

    /// Check if the status is unhealthy.
    #[must_use]
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, Self::Unhealthy)
    }

    /// Get the status as an HTTP status code.
    #[must_use]
    pub fn http_status_code(&self) -> u16 {
        match self {
            Self::Healthy => 200,
            Self::Degraded => 200, // Still operational
            Self::Unhealthy => 503,
        }
    }

    /// Get the status as a string.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Health status of a single component.
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    /// Component health status.
    pub status: HealthStatus,
    /// Optional message describing the status.
    pub message: Option<String>,
    /// Additional details about the component.
    pub details: HashMap<String, String>,
    /// Time taken to check this component.
    pub check_duration: Duration,
}

impl ComponentHealth {
    /// Create a healthy component status.
    #[must_use]
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            details: HashMap::new(),
            check_duration: Duration::ZERO,
        }
    }

    /// Create a degraded component status.
    #[must_use]
    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            details: HashMap::new(),
            check_duration: Duration::ZERO,
        }
    }

    /// Create an unhealthy component status.
    #[must_use]
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            details: HashMap::new(),
            check_duration: Duration::ZERO,
        }
    }

    /// Add a detail to the health status.
    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    /// Add multiple details to the health status.
    #[must_use]
    pub fn with_details(mut self, details: impl IntoIterator<Item = (String, String)>) -> Self {
        self.details.extend(details);
        self
    }

    /// Set the check duration.
    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.check_duration = duration;
        self
    }
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self::healthy()
    }
}

/// Type alias for health check functions.
pub type HealthCheckFn = Arc<dyn Fn() -> ComponentHealth + Send + Sync>;

/// Detailed health check result.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Service name.
    pub service: String,
    /// Overall health status.
    pub status: HealthStatus,
    /// Service version (if available).
    pub version: Option<String>,
    /// How long the health check took.
    pub check_duration: Duration,
    /// Individual component health statuses.
    pub components: HashMap<String, ComponentHealth>,
    /// Timestamp of the health check.
    pub timestamp: std::time::SystemTime,
}

impl HealthReport {
    /// Check if the service is healthy.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }

    /// Get the number of healthy components.
    #[must_use]
    pub fn healthy_count(&self) -> usize {
        self.components
            .values()
            .filter(|c| c.status.is_healthy())
            .count()
    }

    /// Get the number of degraded components.
    #[must_use]
    pub fn degraded_count(&self) -> usize {
        self.components
            .values()
            .filter(|c| c.status.is_degraded())
            .count()
    }

    /// Get the number of unhealthy components.
    #[must_use]
    pub fn unhealthy_count(&self) -> usize {
        self.components
            .values()
            .filter(|c| c.status.is_unhealthy())
            .count()
    }

    /// Convert to a JSON-serializable structure.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let components: HashMap<String, serde_json::Value> = self
            .components
            .iter()
            .map(|(name, health)| {
                let mut obj = serde_json::json!({
                    "status": health.status.as_str(),
                    "check_duration_ms": health.check_duration.as_millis(),
                });

                if let Some(msg) = &health.message {
                    obj["message"] = serde_json::json!(msg);
                }

                if !health.details.is_empty() {
                    obj["details"] = serde_json::json!(health.details);
                }

                (name.clone(), obj)
            })
            .collect();

        let mut result = serde_json::json!({
            "status": self.status.as_str(),
            "service": self.service,
            "check_duration_ms": self.check_duration.as_millis(),
            "components": components,
        });

        if let Some(version) = &self.version {
            result["version"] = serde_json::json!(version);
        }

        result
    }
}

/// Health checker for MCP servers.
///
/// Provides a centralized way to register and execute health checks.
#[derive(Default)]
pub struct HealthChecker {
    service_name: String,
    version: Option<String>,
    checks: HashMap<String, HealthCheckFn>,
}

impl HealthChecker {
    /// Create a new health checker.
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            version: None,
            checks: HashMap::new(),
        }
    }

    /// Set the service version.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add a health check for a component.
    pub fn add_check<F>(&mut self, name: impl Into<String>, check: F)
    where
        F: Fn() -> ComponentHealth + Send + Sync + 'static,
    {
        self.checks.insert(name.into(), Arc::new(check));
    }

    /// Add a simple health check that just returns healthy.
    pub fn add_simple_check(&mut self, name: impl Into<String>) {
        self.add_check(name, ComponentHealth::healthy);
    }

    /// Run all health checks and return a report.
    #[must_use]
    pub fn check(&self) -> HealthReport {
        let start = Instant::now();
        let mut components = HashMap::new();
        let mut overall_status = HealthStatus::Healthy;

        for (name, check_fn) in &self.checks {
            let check_start = Instant::now();
            let mut result = check_fn();
            result.check_duration = check_start.elapsed();

            // Update overall status based on component status
            match (&overall_status, &result.status) {
                (HealthStatus::Healthy, HealthStatus::Degraded) => {
                    overall_status = HealthStatus::Degraded;
                }
                (_, HealthStatus::Unhealthy) => {
                    overall_status = HealthStatus::Unhealthy;
                }
                _ => {}
            }

            components.insert(name.clone(), result);
        }

        HealthReport {
            service: self.service_name.clone(),
            status: overall_status,
            version: self.version.clone(),
            check_duration: start.elapsed(),
            components,
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Run a quick liveness check (just verifies the service is running).
    #[must_use]
    pub fn liveness(&self) -> bool {
        true
    }

    /// Run a readiness check (verifies all components are ready).
    #[must_use]
    pub fn readiness(&self) -> bool {
        self.check().is_healthy()
    }
}

impl std::fmt::Debug for HealthChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthChecker")
            .field("service_name", &self.service_name)
            .field("version", &self.version)
            .field("check_count", &self.checks.len())
            .finish()
    }
}

/// Liveness probe response.
#[derive(Debug, Clone)]
pub struct LivenessResponse {
    /// Whether the service is alive.
    pub alive: bool,
    /// Service name.
    pub service: String,
}

impl LivenessResponse {
    /// Create a new liveness response.
    #[must_use]
    pub fn new(service: impl Into<String>, alive: bool) -> Self {
        Self {
            alive,
            service: service.into(),
        }
    }

    /// Create an alive response.
    #[must_use]
    pub fn alive(service: impl Into<String>) -> Self {
        Self::new(service, true)
    }

    /// Convert to JSON.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "alive": self.alive,
            "service": self.service,
        })
    }
}

/// Readiness probe response.
#[derive(Debug, Clone)]
pub struct ReadinessResponse {
    /// Whether the service is ready.
    pub ready: bool,
    /// Service name.
    pub service: String,
    /// Optional reason if not ready.
    pub reason: Option<String>,
}

impl ReadinessResponse {
    /// Create a new readiness response.
    #[must_use]
    pub fn new(service: impl Into<String>, ready: bool) -> Self {
        Self {
            ready,
            service: service.into(),
            reason: None,
        }
    }

    /// Create a ready response.
    #[must_use]
    pub fn ready(service: impl Into<String>) -> Self {
        Self::new(service, true)
    }

    /// Create a not-ready response with a reason.
    #[must_use]
    pub fn not_ready(service: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            ready: false,
            service: service.into(),
            reason: Some(reason.into()),
        }
    }

    /// Convert to JSON.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut result = serde_json::json!({
            "ready": self.ready,
            "service": self.service,
        });

        if let Some(reason) = &self.reason {
            result["reason"] = serde_json::json!(reason);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Degraded.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_healthy());

        assert_eq!(HealthStatus::Healthy.http_status_code(), 200);
        assert_eq!(HealthStatus::Degraded.http_status_code(), 200);
        assert_eq!(HealthStatus::Unhealthy.http_status_code(), 503);
    }

    #[test]
    fn test_component_health() {
        let health = ComponentHealth::healthy()
            .with_detail("connections", "10")
            .with_detail("memory_mb", "256");

        assert!(health.status.is_healthy());
        assert_eq!(health.details.get("connections"), Some(&"10".to_string()));
    }

    #[test]
    fn test_health_checker() {
        let mut checker = HealthChecker::new("test-service").with_version("1.0.0");

        checker.add_check("component_a", ComponentHealth::healthy);
        checker.add_check("component_b", || {
            ComponentHealth::healthy().with_detail("status", "ok")
        });

        let report = checker.check();

        assert!(report.is_healthy());
        assert_eq!(report.healthy_count(), 2);
        assert_eq!(report.unhealthy_count(), 0);
    }

    #[test]
    fn test_degraded_status() {
        let mut checker = HealthChecker::new("test-service");

        checker.add_check("healthy_component", ComponentHealth::healthy);
        checker.add_check("degraded_component", || {
            ComponentHealth::degraded("High latency detected")
        });

        let report = checker.check();

        assert!(!report.is_healthy());
        assert_eq!(report.status, HealthStatus::Degraded);
    }

    #[test]
    fn test_unhealthy_status() {
        let mut checker = HealthChecker::new("test-service");

        checker.add_check("healthy_component", ComponentHealth::healthy);
        checker.add_check("unhealthy_component", || {
            ComponentHealth::unhealthy("Connection refused")
        });

        let report = checker.check();

        assert!(report.status.is_unhealthy());
    }

    #[test]
    fn test_liveness_and_readiness() {
        let mut checker = HealthChecker::new("test-service");
        checker.add_check("component", ComponentHealth::healthy);

        assert!(checker.liveness());
        assert!(checker.readiness());
    }

    #[test]
    fn test_health_report_json() {
        let mut checker = HealthChecker::new("test-service").with_version("1.0.0");
        checker.add_check("database", ComponentHealth::healthy);

        let report = checker.check();
        let json = report.to_json();

        assert_eq!(json["status"], "healthy");
        assert_eq!(json["service"], "test-service");
        assert_eq!(json["version"], "1.0.0");
    }
}
