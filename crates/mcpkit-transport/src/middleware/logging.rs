//! Logging middleware for MCP transports.
//!
//! This middleware logs all messages sent and received through the transport,
//! useful for debugging and monitoring.

use crate::middleware::TransportLayer;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{Level, debug, trace};

/// A redaction hook: given a message, returns the sanitized JSON value to log.
///
/// Only invoked when content logging is enabled; the message forwarded to the
/// inner transport is never modified.
type Redactor = Arc<dyn Fn(&Message) -> Value + Send + Sync>;

/// A layer that adds logging to a transport.
///
/// Logs all sent and received messages at the configured level. When content
/// logging is enabled, configure a redactor ([`redact_keys`](Self::redact_keys)
/// or [`redact_with`](Self::redact_with)) to avoid leaking secrets into logs.
#[derive(Clone)]
pub struct LoggingLayer {
    /// The log level to use.
    level: Level,
    /// Whether to log message contents.
    log_contents: bool,
    /// Optional redaction applied to logged content (never to the forwarded
    /// message).
    redactor: Option<Redactor>,
}

impl fmt::Debug for LoggingLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoggingLayer")
            .field("level", &self.level)
            .field("log_contents", &self.log_contents)
            .field("redactor", &self.redactor.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl LoggingLayer {
    /// A recommended denylist of object keys whose values commonly carry
    /// secrets. Pass to [`redact_keys`](Self::redact_keys), or use
    /// [`with_redacted_contents`](Self::with_redacted_contents) to apply it.
    pub const DEFAULT_REDACT_KEYS: &'static [&'static str] = &[
        "authorization",
        "cookie",
        "set-cookie",
        "password",
        "passwd",
        "secret",
        "token",
        "access_token",
        "refresh_token",
        "id_token",
        "client_secret",
        "api_key",
        "apikey",
        "key",
        "private_key",
        "credential",
        "credentials",
    ];

    /// Create a new logging layer with the specified log level.
    #[must_use]
    pub const fn new(level: Level) -> Self {
        Self {
            level,
            log_contents: false,
            redactor: None,
        }
    }

    /// Configure whether to log full message contents.
    ///
    /// Warning: without a redactor this may log sensitive data. Prefer
    /// [`with_redacted_contents`](Self::with_redacted_contents), or pair this
    /// with [`redact_keys`](Self::redact_keys) / [`redact_with`](Self::redact_with).
    #[must_use]
    pub const fn with_contents(mut self, log_contents: bool) -> Self {
        self.log_contents = log_contents;
        self
    }

    /// Enable content logging with the [`DEFAULT_REDACT_KEYS`](Self::DEFAULT_REDACT_KEYS)
    /// denylist applied — the safe one-call path.
    #[must_use]
    pub fn with_redacted_contents(self) -> Self {
        self.with_contents(true)
            .redact_keys(Self::DEFAULT_REDACT_KEYS.iter().copied())
    }

    /// Redact logged content by masking object values whose key matches (case-
    /// insensitively) any key in `keys`, recursively, with `"<redacted>"`.
    ///
    /// Only affects the logged representation; the forwarded message is
    /// unchanged. Has no effect unless content logging is enabled.
    #[must_use]
    pub fn redact_keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let keys: HashSet<String> = keys
            .into_iter()
            .map(|k| k.into().to_ascii_lowercase())
            .collect();
        self.redactor = Some(Arc::new(move |msg: &Message| redact_message(msg, &keys)));
        self
    }

    /// Redact logged content with a custom hook returning the JSON value to log.
    ///
    /// Only affects the logged representation; the forwarded message is
    /// unchanged. Has no effect unless content logging is enabled.
    #[must_use]
    pub fn redact_with<F>(mut self, redactor: F) -> Self
    where
        F: Fn(&Message) -> Value + Send + Sync + 'static,
    {
        self.redactor = Some(Arc::new(redactor));
        self
    }
}

/// Serialize `msg` and mask values under sensitive keys. On the (near-
/// impossible) serialization failure, returns a placeholder rather than
/// exposing the raw message.
fn redact_message(msg: &Message, keys: &HashSet<String>) -> Value {
    match serde_json::to_value(msg) {
        Ok(mut value) => {
            redact_in_place(&mut value, keys);
            value
        }
        Err(_) => Value::String("<redaction failed>".to_string()),
    }
}

fn redact_in_place(value: &mut Value, keys: &HashSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if keys.contains(&key.to_ascii_lowercase()) {
                    *val = Value::String("<redacted>".to_string());
                } else {
                    redact_in_place(val, keys);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                redact_in_place(item, keys);
            }
        }
        _ => {}
    }
}

impl Default for LoggingLayer {
    fn default() -> Self {
        Self::new(Level::DEBUG)
    }
}

impl<T: Transport> TransportLayer<T> for LoggingLayer {
    type Transport = LoggingTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        LoggingTransport {
            inner,
            level: self.level,
            log_contents: self.log_contents,
            redactor: self.redactor.clone(),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }
}

/// A transport wrapped with logging.
pub struct LoggingTransport<T> {
    inner: T,
    level: Level,
    log_contents: bool,
    redactor: Option<Redactor>,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
}

impl<T> LoggingTransport<T> {
    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }
}

impl<T: Transport> Transport for LoggingTransport<T> {
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let count = self.messages_sent.fetch_add(1, Ordering::Relaxed) + 1;

        if self.log_contents {
            match &self.redactor {
                Some(redact) => {
                    let redacted = redact(&msg);
                    match self.level {
                        Level::TRACE => trace!(count, ?redacted, "sending message"),
                        Level::DEBUG => debug!(count, ?redacted, "sending message"),
                        _ => debug!(count, "sending message"),
                    }
                }
                None => match self.level {
                    Level::TRACE => trace!(count, ?msg, "sending message"),
                    Level::DEBUG => debug!(count, ?msg, "sending message"),
                    _ => debug!(count, "sending message"),
                },
            }
        } else {
            let method = msg.method().unwrap_or("<response>");
            match self.level {
                Level::TRACE => trace!(count, method, "sending message"),
                Level::DEBUG => debug!(count, method, "sending message"),
                _ => debug!(count, "sending message"),
            }
        }

        self.inner.send(msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        let result = self.inner.recv().await?;

        if let Some(ref msg) = result {
            let count = self.messages_received.fetch_add(1, Ordering::Relaxed) + 1;

            if self.log_contents {
                match &self.redactor {
                    Some(redact) => {
                        let redacted = redact(msg);
                        match self.level {
                            Level::TRACE => trace!(count, ?redacted, "received message"),
                            Level::DEBUG => debug!(count, ?redacted, "received message"),
                            _ => debug!(count, "received message"),
                        }
                    }
                    None => match self.level {
                        Level::TRACE => trace!(count, ?msg, "received message"),
                        Level::DEBUG => debug!(count, ?msg, "received message"),
                        _ => debug!(count, "received message"),
                    },
                }
            } else {
                let method = msg.method().unwrap_or("<response>");
                match self.level {
                    Level::TRACE => trace!(count, method, "received message"),
                    Level::DEBUG => debug!(count, method, "received message"),
                    _ => debug!(count, "received message"),
                }
            }
        }

        Ok(result)
    }

    async fn close(&self) -> Result<(), Self::Error> {
        debug!(
            sent = self.messages_sent(),
            received = self.messages_received(),
            "closing transport"
        );
        self.inner.close().await
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn metadata(&self) -> TransportMetadata {
        self.inner.metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryTransport;
    use mcpkit_core::protocol::{Request, RequestId};
    use serde_json::json;
    use std::sync::atomic::AtomicBool;

    fn request_with(params: Value) -> Message {
        Message::Request(Request::with_params(
            "tools/call".to_string(),
            RequestId::Number(1),
            params,
        ))
    }

    #[test]
    fn test_logging_layer_creation() {
        let layer = LoggingLayer::new(Level::DEBUG);
        assert!(!layer.log_contents);
        assert!(layer.redactor.is_none());

        let layer = layer.with_contents(true);
        assert!(layer.log_contents);
    }

    #[test]
    fn redact_keys_masks_nested_secrets_case_insensitively() {
        let keys: HashSet<String> = LoggingLayer::DEFAULT_REDACT_KEYS
            .iter()
            .map(|k| (*k).to_string())
            .collect();
        let msg = request_with(json!({
            "Authorization": "Bearer abc",
            "arguments": {
                "PASSWORD": "hunter2",
                "nested": { "access_token": "xyz", "safe": "keep" }
            },
            "list": [ { "token": "t1" }, { "ok": "v" } ]
        }));

        let redacted = redact_message(&msg, &keys);
        let s = serde_json::to_string(&redacted).expect("serialize");

        // Secrets gone (case-insensitive, nested, and inside arrays).
        assert!(!s.contains("Bearer abc"), "{s}");
        assert!(!s.contains("hunter2"), "{s}");
        assert!(!s.contains("xyz"), "{s}");
        assert!(!s.contains("t1"), "{s}");
        assert!(s.contains("<redacted>"), "{s}");
        // Non-sensitive values preserved.
        assert!(s.contains("keep"), "{s}");
        assert!(s.contains("\"ok\":\"v\""), "{s}");
    }

    #[test]
    fn redact_with_uses_custom_hook() {
        let layer = LoggingLayer::new(Level::DEBUG)
            .with_contents(true)
            .redact_with(|_msg| json!("CUSTOM"));
        let redactor = layer.redactor.as_ref().expect("redactor set");
        assert_eq!(redactor(&request_with(json!({}))), json!("CUSTOM"));
    }

    #[test]
    fn with_redacted_contents_enables_and_sets_redactor() {
        let layer = LoggingLayer::new(Level::DEBUG).with_redacted_contents();
        assert!(layer.log_contents);
        assert!(layer.redactor.is_some());
    }

    #[tokio::test]
    async fn log_contents_false_never_invokes_redactor() {
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let (a, _b) = MemoryTransport::pair();
        // Content logging is off by default: the redactor must not run.
        let transport = LoggingLayer::new(Level::DEBUG)
            .redact_with(move |_| {
                flag.store(true, Ordering::SeqCst);
                Value::Null
            })
            .layer(a);
        transport
            .send(request_with(json!({ "token": "secret" })))
            .await
            .expect("send");
        assert!(
            !called.load(Ordering::SeqCst),
            "redactor must not run when log_contents is false"
        );
    }

    #[tokio::test]
    async fn log_contents_true_invokes_redactor() {
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let (a, _b) = MemoryTransport::pair();
        let transport = LoggingLayer::new(Level::DEBUG)
            .with_contents(true)
            .redact_with(move |_| {
                flag.store(true, Ordering::SeqCst);
                Value::Null
            })
            .layer(a);
        transport
            .send(request_with(json!({ "token": "secret" })))
            .await
            .expect("send");
        assert!(
            called.load(Ordering::SeqCst),
            "redactor must run when log_contents is true"
        );
    }
}
