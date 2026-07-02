//! MCP logging utility types (`logging/setLevel`, `notifications/message`).

use super::meta::Meta;
use serde::{Deserialize, Serialize};

/// Logging severity, ordered least → most severe (the syslog levels the MCP
/// spec uses). `Ord` reflects severity, so `level >= min` filters messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    /// Debug-level, most verbose.
    Debug,
    /// Informational messages.
    #[default]
    Info,
    /// Normal but significant conditions.
    Notice,
    /// Warning conditions.
    Warning,
    /// Error conditions.
    Error,
    /// Critical conditions.
    Critical,
    /// Action must be taken immediately.
    Alert,
    /// System is unusable, most severe.
    Emergency,
}

impl std::fmt::Display for LoggingLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Notice => "notice",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
            Self::Alert => "alert",
            Self::Emergency => "emergency",
        };
        f.write_str(s)
    }
}

/// Request parameters for `logging/setLevel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelRequest {
    /// The minimum severity the client wants to receive.
    pub level: LoggingLevel,
}

/// Params for `notifications/message` — a log message emitted by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingMessageNotificationParams {
    /// Severity of this message.
    pub level: LoggingLevel,
    /// Optional name of the logger issuing the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// The log payload (any JSON value — string, object, etc.).
    pub data: serde_json::Value,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl LoggingMessageNotificationParams {
    /// Create log params at `level` with `data` (no logger name).
    #[must_use]
    pub const fn new(level: LoggingLevel, data: serde_json::Value) -> Self {
        Self {
            level,
            logger: None,
            data,
            meta: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn level_serializes_lowercase_and_orders_by_severity() {
        assert_eq!(
            serde_json::to_value(LoggingLevel::Warning).unwrap(),
            json!("warning")
        );
        let back: LoggingLevel = serde_json::from_value(json!("emergency")).unwrap();
        assert_eq!(back, LoggingLevel::Emergency);
        assert!(LoggingLevel::Debug < LoggingLevel::Error);
        assert!(LoggingLevel::Emergency > LoggingLevel::Alert);
    }

    #[test]
    fn set_level_request_round_trips() {
        let wire = serde_json::to_value(SetLevelRequest {
            level: LoggingLevel::Notice,
        })
        .unwrap();
        assert_eq!(wire, json!({ "level": "notice" }));
        let back: SetLevelRequest = serde_json::from_value(wire).unwrap();
        assert_eq!(back.level, LoggingLevel::Notice);
    }

    #[test]
    fn message_params_omit_absent_logger_and_meta() {
        let params = LoggingMessageNotificationParams::new(LoggingLevel::Info, json!("hello"));
        let wire = serde_json::to_value(&params).unwrap();
        assert_eq!(wire, json!({ "level": "info", "data": "hello" }));

        let with_logger = LoggingMessageNotificationParams {
            logger: Some("db".into()),
            ..LoggingMessageNotificationParams::new(LoggingLevel::Error, json!({ "code": 5 }))
        };
        let wire = serde_json::to_value(&with_logger).unwrap();
        assert_eq!(
            wire,
            json!({ "level": "error", "logger": "db", "data": { "code": 5 } })
        );
    }
}
