//! OpenTelemetry integration for MCP transports.
//!
//! This module provides observability infrastructure compatible with
//! OpenTelemetry for distributed tracing, metrics, and logging.
//!
//! # Features
//!
//! - **Distributed Tracing**: Automatic span creation for transport operations
//! - **Metrics**: Request counts, latencies, and error rates
//! - **Context Propagation**: Pass trace context across service boundaries
//!
//! # Example
//!
//! ```rust
//! use mcpkit_transport::telemetry::TelemetryConfig;
//!
//! // Configure telemetry
//! let config = TelemetryConfig::new("my-mcp-service")
//!     .with_message_content()  // Record message contents
//!     .with_max_recorded_size(1024);  // Limit size
//!
//! assert_eq!(config.service_name, "my-mcp-service");
//! assert!(config.record_message_content);
//! ```
//!
//! # Span Attributes
//!
//! MCP operations emit spans with the following attributes:
//!
//! | Attribute | Description |
//! |-----------|-------------|
//! | `mcp.method` | The MCP method name (e.g., "tools/call") |
//! | `mcp.request_id` | The JSON-RPC request ID |
//! | `mcp.transport` | Transport type (stdio, http, websocket, unix) |
//! | `mcp.message_size` | Size of the message in bytes |
//! | `mcp.error` | Error message if the operation failed |
//!
//! # Metrics
//!
//! The following metrics are exposed:
//!
//! | Metric | Type | Description |
//! |--------|------|-------------|
//! | `mcp_messages_sent_total` | Counter | Total messages sent |
//! | `mcp_messages_received_total` | Counter | Total messages received |
//! | `mcp_message_latency_seconds` | Histogram | Message processing latency |
//! | `mcp_errors_total` | Counter | Total errors by type |
//! | `mcp_active_connections` | Gauge | Currently active connections |

use crate::error::TransportError;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Configuration for MCP telemetry.
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for span attributes.
    pub service_name: String,
    /// Whether to record message contents (may contain sensitive data).
    pub record_message_content: bool,
    /// Whether to record detailed timing breakdowns.
    pub record_timing: bool,
    /// Maximum message size to record (bytes, 0 = don't record).
    pub max_recorded_message_size: usize,
    /// Span name prefix.
    pub span_prefix: String,
}

impl TelemetryConfig {
    /// Create a new telemetry configuration.
    #[must_use]
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            record_message_content: false,
            record_timing: true,
            max_recorded_message_size: 0,
            span_prefix: "mcp".to_string(),
        }
    }

    /// Enable recording of message contents (use with caution).
    #[must_use]
    pub const fn with_message_content(mut self) -> Self {
        self.record_message_content = true;
        self
    }

    /// Set maximum message size to record.
    #[must_use]
    pub const fn with_max_recorded_size(mut self, size: usize) -> Self {
        self.max_recorded_message_size = size;
        self
    }

    /// Disable timing recording.
    #[must_use]
    pub const fn without_timing(mut self) -> Self {
        self.record_timing = false;
        self
    }

    /// Set custom span prefix.
    #[must_use]
    pub fn with_span_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.span_prefix = prefix.into();
        self
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self::new("mcp-service")
    }
}

/// Telemetry metrics collected during transport operations.
#[derive(Debug, Default)]
pub struct TelemetryMetrics {
    /// Total messages sent.
    pub messages_sent: AtomicU64,
    /// Total messages received.
    pub messages_received: AtomicU64,
    /// Total bytes sent.
    pub bytes_sent: AtomicU64,
    /// Total bytes received.
    pub bytes_received: AtomicU64,
    /// Total errors.
    pub errors: AtomicU64,
    /// Total connection errors.
    pub connection_errors: AtomicU64,
    /// Total serialization errors.
    pub serialization_errors: AtomicU64,
    /// Total timeout errors.
    pub timeout_errors: AtomicU64,
}

impl TelemetryMetrics {
    /// Create new metrics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a sent message.
    pub fn record_send(&self, size: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }

    /// Record a received message.
    pub fn record_receive(&self, size: usize) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(size as u64, Ordering::Relaxed);
    }

    /// Record an error.
    pub fn record_error(&self, err: &TransportError) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        match err {
            TransportError::Connection { .. }
            | TransportError::ConnectionClosed
            | TransportError::NotConnected => {
                self.connection_errors.fetch_add(1, Ordering::Relaxed);
            }
            TransportError::Serialization { .. }
            | TransportError::Deserialization { .. }
            | TransportError::Json(_) => {
                self.serialization_errors.fetch_add(1, Ordering::Relaxed);
            }
            TransportError::Timeout { .. } => {
                self.timeout_errors.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Get snapshot of all metrics.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            connection_errors: self.connection_errors.load(Ordering::Relaxed),
            serialization_errors: self.serialization_errors.load(Ordering::Relaxed),
            timeout_errors: self.timeout_errors.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time snapshot of telemetry metrics.
#[derive(Debug, Clone, Copy)]
pub struct MetricsSnapshot {
    /// Total messages sent.
    pub messages_sent: u64,
    /// Total messages received.
    pub messages_received: u64,
    /// Total bytes sent.
    pub bytes_sent: u64,
    /// Total bytes received.
    pub bytes_received: u64,
    /// Total errors.
    pub errors: u64,
    /// Connection errors.
    pub connection_errors: u64,
    /// Serialization errors.
    pub serialization_errors: u64,
    /// Timeout errors.
    pub timeout_errors: u64,
}

impl MetricsSnapshot {
    /// Calculate error rate.
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        let total = self.messages_sent + self.messages_received;
        if total == 0 {
            0.0
        } else {
            self.errors as f64 / total as f64
        }
    }

    /// Calculate average message size (bytes).
    #[must_use]
    pub fn avg_message_size(&self) -> f64 {
        let total = self.messages_sent + self.messages_received;
        if total == 0 {
            0.0
        } else {
            (self.bytes_sent + self.bytes_received) as f64 / total as f64
        }
    }
}

/// Latency histogram for tracking request durations.
#[derive(Debug)]
pub struct LatencyHistogram {
    /// Bucket boundaries in milliseconds.
    buckets: Vec<u64>,
    /// Counts per bucket.
    counts: Vec<AtomicU64>,
    /// Total count.
    total_count: AtomicU64,
    /// Sum of all values (for average).
    sum_ms: AtomicU64,
}

impl LatencyHistogram {
    /// Create a histogram with default buckets (in ms).
    #[must_use]
    pub fn new() -> Self {
        // Default buckets: 1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000 ms
        Self::with_buckets(vec![
            1, 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000,
        ])
    }

    /// Create a histogram with custom bucket boundaries.
    #[must_use]
    pub fn with_buckets(buckets: Vec<u64>) -> Self {
        let counts = buckets.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            buckets,
            counts,
            total_count: AtomicU64::new(0),
            sum_ms: AtomicU64::new(0),
        }
    }

    /// Record a latency value.
    pub fn record(&self, duration: Duration) {
        let ms = duration.as_millis() as u64;
        self.sum_ms.fetch_add(ms, Ordering::Relaxed);
        self.total_count.fetch_add(1, Ordering::Relaxed);

        // Find the appropriate bucket
        for (i, &boundary) in self.buckets.iter().enumerate() {
            if ms <= boundary {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        // Value exceeds all buckets, add to last bucket
        if let Some(last) = self.counts.last() {
            last.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get the average latency in milliseconds.
    #[must_use]
    pub fn average_ms(&self) -> f64 {
        let total = self.total_count.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.sum_ms.load(Ordering::Relaxed) as f64 / total as f64
        }
    }

    /// Get the count of observations.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.total_count.load(Ordering::Relaxed)
    }

    /// Get percentile estimate (approximate).
    #[must_use]
    pub fn percentile(&self, p: f64) -> u64 {
        let total = self.total_count.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }

        let target = (total as f64 * p / 100.0) as u64;
        let mut cumulative = 0u64;

        for (i, count) in self.counts.iter().enumerate() {
            cumulative += count.load(Ordering::Relaxed);
            if cumulative >= target {
                return self.buckets[i];
            }
        }

        *self.buckets.last().unwrap_or(&0)
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Telemetry layer for transports.
pub struct TelemetryLayer {
    config: TelemetryConfig,
    metrics: Arc<TelemetryMetrics>,
    send_latency: Arc<LatencyHistogram>,
    recv_latency: Arc<LatencyHistogram>,
}

impl TelemetryLayer {
    /// Create a new telemetry layer.
    #[must_use]
    pub fn new(config: TelemetryConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(TelemetryMetrics::new()),
            send_latency: Arc::new(LatencyHistogram::new()),
            recv_latency: Arc::new(LatencyHistogram::new()),
        }
    }

    /// Get the metrics.
    #[must_use]
    pub fn metrics(&self) -> &TelemetryMetrics {
        &self.metrics
    }

    /// Get send latency histogram.
    #[must_use]
    pub fn send_latency(&self) -> &LatencyHistogram {
        &self.send_latency
    }

    /// Get receive latency histogram.
    #[must_use]
    pub fn recv_latency(&self) -> &LatencyHistogram {
        &self.recv_latency
    }
}

impl<T: Transport> super::middleware::TransportLayer<T> for TelemetryLayer {
    type Transport = TelemetryTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        TelemetryTransport {
            inner,
            config: self.config.clone(),
            metrics: Arc::clone(&self.metrics),
            send_latency: Arc::clone(&self.send_latency),
            recv_latency: Arc::clone(&self.recv_latency),
        }
    }
}

/// A transport wrapped with telemetry instrumentation.
pub struct TelemetryTransport<T> {
    inner: T,
    config: TelemetryConfig,
    metrics: Arc<TelemetryMetrics>,
    send_latency: Arc<LatencyHistogram>,
    recv_latency: Arc<LatencyHistogram>,
}

impl<T: Transport> TelemetryTransport<T> {
    /// Create a new telemetry transport.
    #[must_use]
    pub fn new(inner: T, config: TelemetryConfig) -> Self {
        Self {
            inner,
            config,
            metrics: Arc::new(TelemetryMetrics::new()),
            send_latency: Arc::new(LatencyHistogram::new()),
            recv_latency: Arc::new(LatencyHistogram::new()),
        }
    }

    /// Get the metrics.
    #[must_use]
    pub fn metrics(&self) -> &TelemetryMetrics {
        &self.metrics
    }

    /// Get send latency histogram.
    #[must_use]
    pub fn send_latency(&self) -> &LatencyHistogram {
        &self.send_latency
    }

    /// Get receive latency histogram.
    #[must_use]
    pub fn recv_latency(&self) -> &LatencyHistogram {
        &self.recv_latency
    }

    /// Get the inner transport.
    #[must_use]
    pub const fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T: Transport> Transport for TelemetryTransport<T> {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let start = Instant::now();
        let msg_json = serde_json::to_string(&msg).unwrap_or_default();
        let size = msg_json.len();

        // Extract method name for tracing
        let method = match &msg {
            Message::Request(req) => &req.method,
            Message::Notification(notif) => &notif.method,
            Message::Response(_) => "response",
        };

        // Create span with tracing
        let span = tracing::info_span!(
            "mcp.send",
            otel.name = %format!("{}.send.{}", self.config.span_prefix, method),
            otel.kind = "client",
            mcp.method = %method,
            mcp.transport = %self.inner.metadata().transport_type,
            mcp.message_size = size,
            service.name = %self.config.service_name,
        );

        let _guard = span.enter();

        let result = self.inner.send(msg).await.map_err(|e| {
            let err = TransportError::Connection {
                message: e.to_string(),
            };
            self.metrics.record_error(&err);
            tracing::error!(
                mcp.error = %e,
                "MCP send failed"
            );
            err
        });

        let duration = start.elapsed();
        self.send_latency.record(duration);
        self.metrics.record_send(size);

        if self.config.record_timing {
            tracing::debug!(
                latency_ms = duration.as_millis() as u64,
                "MCP send complete"
            );
        }

        result
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        let start = Instant::now();

        let span = tracing::info_span!(
            "mcp.recv",
            otel.name = %format!("{}.recv", self.config.span_prefix),
            otel.kind = "server",
            mcp.transport = %self.inner.metadata().transport_type,
            service.name = %self.config.service_name,
        );

        let _guard = span.enter();

        let result = self.inner.recv().await.map_err(|e| {
            let err = TransportError::Connection {
                message: e.to_string(),
            };
            self.metrics.record_error(&err);
            tracing::error!(
                mcp.error = %e,
                "MCP recv failed"
            );
            err
        });

        if let Ok(Some(ref msg)) = result {
            let msg_json = serde_json::to_string(msg).unwrap_or_default();
            let size = msg_json.len();
            self.metrics.record_receive(size);

            let method = match msg {
                Message::Request(req) => &req.method,
                Message::Notification(notif) => &notif.method,
                Message::Response(_) => "response",
            };

            let duration = start.elapsed();
            self.recv_latency.record(duration);

            tracing::debug!(
                mcp.method = %method,
                mcp.message_size = size,
                latency_ms = duration.as_millis() as u64,
                "MCP recv complete"
            );
        }

        result
    }

    async fn close(&self) -> Result<(), Self::Error> {
        let span = tracing::info_span!(
            "mcp.close",
            otel.name = %format!("{}.close", self.config.span_prefix),
            mcp.transport = %self.inner.metadata().transport_type,
            service.name = %self.config.service_name,
        );

        let _guard = span.enter();

        self.inner.close().await.map_err(|e| {
            let err = TransportError::Connection {
                message: e.to_string(),
            };
            tracing::error!(
                mcp.error = %e,
                "MCP close failed"
            );
            err
        })
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn metadata(&self) -> TransportMetadata {
        self.inner.metadata()
    }
}

/// Context propagation utilities for distributed tracing.
pub mod propagation {
    use std::collections::HashMap;

    /// Extract trace context from headers.
    #[must_use]
    pub fn extract_context(headers: &HashMap<String, String>) -> Option<TraceContext> {
        // W3C Trace Context format
        let traceparent = headers.get("traceparent")?;
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        Some(TraceContext {
            version: parts[0].to_string(),
            trace_id: parts[1].to_string(),
            parent_id: parts[2].to_string(),
            flags: parts[3].to_string(),
            tracestate: headers.get("tracestate").cloned(),
        })
    }

    /// Inject trace context into headers.
    pub fn inject_context(context: &TraceContext, headers: &mut HashMap<String, String>) {
        headers.insert(
            "traceparent".to_string(),
            format!(
                "{}-{}-{}-{}",
                context.version, context.trace_id, context.parent_id, context.flags
            ),
        );
        if let Some(ref state) = context.tracestate {
            headers.insert("tracestate".to_string(), state.clone());
        }
    }

    /// W3C Trace Context.
    #[derive(Debug, Clone)]
    pub struct TraceContext {
        /// Version (always "00").
        pub version: String,
        /// 32-character hex trace ID.
        pub trace_id: String,
        /// 16-character hex parent span ID.
        pub parent_id: String,
        /// Trace flags.
        pub flags: String,
        /// Optional tracestate header.
        pub tracestate: Option<String>,
    }

    impl TraceContext {
        /// Check if sampling is enabled.
        #[must_use]
        pub fn is_sampled(&self) -> bool {
            self.flags.ends_with('1')
        }
    }
}

/// OpenTelemetry SDK integration.
///
/// This module provides integration with the OpenTelemetry SDK for exporting
/// traces, metrics, and logs to OpenTelemetry-compatible backends.
///
/// # Feature Flag
///
/// This module requires the `opentelemetry` feature flag to be enabled:
///
/// ```toml
/// mcpkit-transport = { version = "0.3", features = ["opentelemetry"] }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_transport::telemetry::otel::{init_tracing, OtelConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Initialize OpenTelemetry with OTLP exporter
///     let config = OtelConfig::new("my-mcp-service")
///         .with_otlp_endpoint("http://localhost:4317");
///
///     let _guard = init_tracing(config)?;
///
///     // Your MCP service code here...
///     // Traces will be automatically exported to the OTLP endpoint
///
///     Ok(())
/// }
/// ```
#[cfg(feature = "opentelemetry")]
pub mod otel {
    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{Resource, trace::SdkTracerProvider};
    use tracing_opentelemetry::OpenTelemetryLayer;
    use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

    /// Configuration for OpenTelemetry integration.
    #[derive(Debug, Clone)]
    pub struct OtelConfig {
        /// Service name for traces and metrics.
        pub service_name: String,
        /// OTLP endpoint URL (e.g., `http://localhost:4317`).
        pub otlp_endpoint: Option<String>,
        /// Service version.
        pub service_version: Option<String>,
        /// Deployment environment (e.g., "production", "staging").
        pub environment: Option<String>,
        /// Enable trace sampling (1.0 = always sample, 0.0 = never sample).
        pub sample_ratio: f64,
        /// Log filter directive (e.g., "info,mcpkit=debug").
        pub log_filter: String,
    }

    impl OtelConfig {
        /// Create a new OpenTelemetry configuration.
        #[must_use]
        pub fn new(service_name: impl Into<String>) -> Self {
            Self {
                service_name: service_name.into(),
                otlp_endpoint: None,
                service_version: None,
                environment: None,
                sample_ratio: 1.0,
                log_filter: "info".to_string(),
            }
        }

        /// Set the OTLP exporter endpoint.
        #[must_use]
        pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
            self.otlp_endpoint = Some(endpoint.into());
            self
        }

        /// Set the service version.
        #[must_use]
        pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
            self.service_version = Some(version.into());
            self
        }

        /// Set the deployment environment.
        #[must_use]
        pub fn with_environment(mut self, env: impl Into<String>) -> Self {
            self.environment = Some(env.into());
            self
        }

        /// Set the trace sample ratio (0.0 to 1.0).
        #[must_use]
        pub fn with_sample_ratio(mut self, ratio: f64) -> Self {
            self.sample_ratio = ratio.clamp(0.0, 1.0);
            self
        }

        /// Set the log filter directive.
        #[must_use]
        pub fn with_log_filter(mut self, filter: impl Into<String>) -> Self {
            self.log_filter = filter.into();
            self
        }
    }

    impl Default for OtelConfig {
        fn default() -> Self {
            Self::new("mcp-service")
        }
    }

    /// Guard that shuts down the tracer provider when dropped.
    pub struct TracingGuard {
        provider: SdkTracerProvider,
    }

    impl Drop for TracingGuard {
        fn drop(&mut self) {
            if let Err(e) = self.provider.shutdown() {
                eprintln!("Failed to shutdown tracer provider: {e:?}");
            }
        }
    }

    /// Initialize OpenTelemetry tracing with the given configuration.
    ///
    /// Returns a guard that must be held for the lifetime of the application.
    /// When dropped, it will flush and shutdown the tracer provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the tracer provider fails to initialize.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use mcpkit_transport::telemetry::otel::{init_tracing, OtelConfig};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = OtelConfig::new("my-service")
    ///         .with_otlp_endpoint("http://localhost:4317")
    ///         .with_environment("production");
    ///
    ///     let _guard = init_tracing(config)?;
    ///
    ///     // Traces are now being exported...
    ///     tracing::info!("Service started");
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn init_tracing(
        config: OtelConfig,
    ) -> Result<TracingGuard, Box<dyn std::error::Error + Send + Sync>> {
        // Build resource attributes
        let mut attributes = vec![KeyValue::new("service.name", config.service_name.clone())];

        if let Some(version) = &config.service_version {
            attributes.push(KeyValue::new("service.version", version.clone()));
        }

        if let Some(env) = &config.environment {
            attributes.push(KeyValue::new("deployment.environment", env.clone()));
        }

        let resource = Resource::builder().with_attributes(attributes).build();

        // Build the tracer provider
        let mut provider_builder = SdkTracerProvider::builder()
            .with_resource(resource)
            .with_sampler(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(
                config.sample_ratio,
            ));

        // Add OTLP exporter if endpoint is configured
        if let Some(endpoint) = &config.otlp_endpoint {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.clone())
                .build()?;

            provider_builder = provider_builder.with_batch_exporter(exporter);
        }

        let provider = provider_builder.build();
        let tracer = provider.tracer(config.service_name.clone());

        // Create the OpenTelemetry layer
        let otel_layer = OpenTelemetryLayer::new(tracer);

        // Build the subscriber with env filter
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&config.log_filter));

        tracing_subscriber::registry()
            .with(filter)
            .with(otel_layer)
            .with(tracing_subscriber::fmt::layer())
            .init();

        Ok(TracingGuard { provider })
    }

    /// Initialize OpenTelemetry tracing with default configuration.
    ///
    /// This uses the service name "mcp-service" and reads the OTLP endpoint
    /// from the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.
    ///
    /// # Errors
    ///
    /// Returns an error if the tracer provider fails to initialize.
    pub fn init_tracing_default() -> Result<TracingGuard, Box<dyn std::error::Error + Send + Sync>>
    {
        let mut config = OtelConfig::default();

        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config = config.with_otlp_endpoint(endpoint);
        }

        if let Ok(service_name) = std::env::var("OTEL_SERVICE_NAME") {
            config.service_name = service_name;
        }

        init_tracing(config)
    }
}

/// Prometheus metrics integration.
///
/// This module provides Prometheus-compatible metrics for MCP transports.
///
/// # Feature Flag
///
/// This module requires the `prometheus` feature flag to be enabled:
///
/// ```toml
/// mcpkit-transport = { version = "0.3", features = ["prometheus"] }
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_transport::telemetry::prom::{McpMetrics, MetricsExporter};
///
/// // Create metrics registry
/// let metrics = McpMetrics::new();
///
/// // Record some operations
/// metrics.record_request("tools/call");
/// metrics.record_response_time("tools/call", 0.05);
///
/// // Export metrics in Prometheus text format
/// let exporter = MetricsExporter::new(metrics.registry());
/// let output = exporter.gather()?;
/// println!("{}", output);
/// ```
#[cfg(feature = "prometheus")]
pub mod prom {
    use prometheus::{
        Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
        TextEncoder,
    };
    use std::sync::Arc;

    /// MCP-specific Prometheus metrics.
    #[derive(Clone)]
    pub struct McpMetrics {
        registry: Arc<Registry>,
        /// Total messages sent.
        pub messages_sent: Counter,
        /// Total messages received.
        pub messages_received: Counter,
        /// Total bytes sent.
        pub bytes_sent: Counter,
        /// Total bytes received.
        pub bytes_received: Counter,
        /// Request count by method.
        pub requests_total: CounterVec,
        /// Error count by type.
        pub errors_total: CounterVec,
        /// Response time histogram by method.
        pub response_time_seconds: HistogramVec,
        /// Active connections gauge.
        pub active_connections: Gauge,
        /// Connection state gauge (by state).
        pub connection_state: GaugeVec,
    }

    impl McpMetrics {
        /// Create a new MCP metrics instance with a custom registry.
        ///
        /// # Errors
        ///
        /// Returns an error if metrics fail to register.
        pub fn new() -> Result<Self, prometheus::Error> {
            Self::with_registry(Registry::new())
        }

        /// Create metrics with a custom registry.
        ///
        /// # Errors
        ///
        /// Returns an error if metrics fail to register.
        pub fn with_registry(registry: Registry) -> Result<Self, prometheus::Error> {
            let messages_sent = Counter::new(
                "mcp_messages_sent_total",
                "Total number of MCP messages sent",
            )?;
            registry.register(Box::new(messages_sent.clone()))?;

            let messages_received = Counter::new(
                "mcp_messages_received_total",
                "Total number of MCP messages received",
            )?;
            registry.register(Box::new(messages_received.clone()))?;

            let bytes_sent = Counter::new(
                "mcp_bytes_sent_total",
                "Total bytes sent over MCP transport",
            )?;
            registry.register(Box::new(bytes_sent.clone()))?;

            let bytes_received = Counter::new(
                "mcp_bytes_received_total",
                "Total bytes received over MCP transport",
            )?;
            registry.register(Box::new(bytes_received.clone()))?;

            let requests_total = CounterVec::new(
                Opts::new("mcp_requests_total", "Total MCP requests by method"),
                &["method"],
            )?;
            registry.register(Box::new(requests_total.clone()))?;

            let errors_total = CounterVec::new(
                Opts::new("mcp_errors_total", "Total MCP errors by type"),
                &["error_type"],
            )?;
            registry.register(Box::new(errors_total.clone()))?;

            let response_time_seconds = HistogramVec::new(
                HistogramOpts::new("mcp_response_time_seconds", "MCP response time in seconds")
                    .buckets(vec![
                        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
                    ]),
                &["method"],
            )?;
            registry.register(Box::new(response_time_seconds.clone()))?;

            let active_connections =
                Gauge::new("mcp_active_connections", "Number of active MCP connections")?;
            registry.register(Box::new(active_connections.clone()))?;

            let connection_state = GaugeVec::new(
                Opts::new("mcp_connection_state", "MCP connection state"),
                &["state"],
            )?;
            registry.register(Box::new(connection_state.clone()))?;

            Ok(Self {
                registry: Arc::new(registry),
                messages_sent,
                messages_received,
                bytes_sent,
                bytes_received,
                requests_total,
                errors_total,
                response_time_seconds,
                active_connections,
                connection_state,
            })
        }

        /// Get the metrics registry.
        #[must_use]
        pub fn registry(&self) -> &Registry {
            &self.registry
        }

        /// Record a message sent.
        pub fn record_send(&self, size: usize) {
            self.messages_sent.inc();
            self.bytes_sent.inc_by(size as f64);
        }

        /// Record a message received.
        pub fn record_receive(&self, size: usize) {
            self.messages_received.inc();
            self.bytes_received.inc_by(size as f64);
        }

        /// Record a request by method.
        pub fn record_request(&self, method: &str) {
            self.requests_total.with_label_values(&[method]).inc();
        }

        /// Record response time for a method.
        pub fn record_response_time(&self, method: &str, seconds: f64) {
            self.response_time_seconds
                .with_label_values(&[method])
                .observe(seconds);
        }

        /// Record an error by type.
        pub fn record_error(&self, error_type: &str) {
            self.errors_total.with_label_values(&[error_type]).inc();
        }

        /// Set the active connection count.
        pub fn set_active_connections(&self, count: i64) {
            self.active_connections.set(count as f64);
        }

        /// Increment active connections.
        pub fn connection_opened(&self) {
            self.active_connections.inc();
        }

        /// Decrement active connections.
        pub fn connection_closed(&self) {
            self.active_connections.dec();
        }

        /// Set connection state.
        pub fn set_connection_state(&self, state: &str, value: f64) {
            self.connection_state.with_label_values(&[state]).set(value);
        }
    }

    impl std::fmt::Debug for McpMetrics {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("McpMetrics")
                .field("registry", &"<Registry>")
                .finish()
        }
    }

    /// Prometheus metrics exporter.
    ///
    /// Provides a way to export metrics in Prometheus text format.
    pub struct MetricsExporter {
        registry: Arc<Registry>,
        encoder: TextEncoder,
    }

    impl MetricsExporter {
        /// Create a new metrics exporter for the given registry.
        #[must_use]
        pub fn new(registry: &Registry) -> Self {
            Self {
                registry: Arc::new(registry.clone()),
                encoder: TextEncoder::new(),
            }
        }

        /// Create an exporter from [`McpMetrics`].
        #[must_use]
        pub fn from_metrics(metrics: &McpMetrics) -> Self {
            Self {
                registry: Arc::clone(&metrics.registry),
                encoder: TextEncoder::new(),
            }
        }

        /// Gather and encode all metrics in Prometheus text format.
        ///
        /// # Errors
        ///
        /// Returns an error if encoding fails.
        pub fn gather(&self) -> Result<String, prometheus::Error> {
            let metric_families = self.registry.gather();
            let mut buffer = Vec::new();
            self.encoder.encode(&metric_families, &mut buffer)?;
            Ok(String::from_utf8_lossy(&buffer).to_string())
        }

        /// Get the content type for the encoded metrics.
        #[must_use]
        pub fn content_type(&self) -> &str {
            self.encoder.format_type()
        }
    }

    impl std::fmt::Debug for MetricsExporter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MetricsExporter")
                .field("registry", &"<Registry>")
                .finish()
        }
    }

    /// Create a default global metrics registry with MCP metrics.
    ///
    /// # Errors
    ///
    /// Returns an error if metrics registration fails.
    pub fn create_default_metrics() -> Result<McpMetrics, prometheus::Error> {
        McpMetrics::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config() {
        let config = TelemetryConfig::new("my-service")
            .with_message_content()
            .with_max_recorded_size(1024)
            .with_span_prefix("custom");

        assert_eq!(config.service_name, "my-service");
        assert!(config.record_message_content);
        assert_eq!(config.max_recorded_message_size, 1024);
        assert_eq!(config.span_prefix, "custom");
    }

    #[test]
    fn test_metrics_recording() {
        let metrics = TelemetryMetrics::new();

        metrics.record_send(100);
        metrics.record_send(200);
        metrics.record_receive(150);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.messages_sent, 2);
        assert_eq!(snapshot.messages_received, 1);
        assert_eq!(snapshot.bytes_sent, 300);
        assert_eq!(snapshot.bytes_received, 150);
    }

    #[test]
    fn test_metrics_snapshot() {
        let snapshot = MetricsSnapshot {
            messages_sent: 100,
            messages_received: 100,
            bytes_sent: 10000,
            bytes_received: 5000,
            errors: 5,
            connection_errors: 2,
            serialization_errors: 2,
            timeout_errors: 1,
        };

        assert!((snapshot.error_rate() - 0.025).abs() < 0.001);
        assert!((snapshot.avg_message_size() - 75.0).abs() < 0.001);
    }

    #[test]
    fn test_latency_histogram() {
        let histogram = LatencyHistogram::new();

        histogram.record(Duration::from_millis(5));
        histogram.record(Duration::from_millis(10));
        histogram.record(Duration::from_millis(50));
        histogram.record(Duration::from_millis(100));
        histogram.record(Duration::from_millis(500));

        assert_eq!(histogram.count(), 5);
        assert!((histogram.average_ms() - 133.0).abs() < 0.1);
    }

    #[test]
    fn test_trace_context_extraction() -> Result<(), Box<dyn std::error::Error>> {
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );

        let context = propagation::extract_context(&headers);
        assert!(context.is_some());

        let ctx = context.ok_or("Context extraction failed")?;
        assert_eq!(ctx.version, "00");
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_id, "00f067aa0ba902b7");
        assert!(ctx.is_sampled());
        Ok(())
    }

    #[test]
    fn test_trace_context_injection() -> Result<(), Box<dyn std::error::Error>> {
        let context = propagation::TraceContext {
            version: "00".to_string(),
            trace_id: "abc123".to_string(),
            parent_id: "def456".to_string(),
            flags: "01".to_string(),
            tracestate: Some("vendor=value".to_string()),
        };

        let mut headers = std::collections::HashMap::new();
        propagation::inject_context(&context, &mut headers);

        assert_eq!(
            headers.get("traceparent").ok_or("traceparent not found")?,
            "00-abc123-def456-01"
        );
        assert_eq!(
            headers.get("tracestate").ok_or("tracestate not found")?,
            "vendor=value"
        );
        Ok(())
    }
}
