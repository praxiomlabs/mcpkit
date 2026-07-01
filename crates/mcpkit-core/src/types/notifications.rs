//! Typed params for the base-protocol progress and cancellation notifications.

use super::meta::Meta;
use crate::protocol::{ProgressToken, RequestId};
use serde::{Deserialize, Serialize};

/// Params for `notifications/progress` — an out-of-band progress update for a
/// long-running request whose caller supplied a `progressToken` in its
/// `_meta`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressNotificationParams {
    /// The progress token from the originating request's `_meta.progressToken`.
    #[serde(rename = "progressToken")]
    pub progress_token: ProgressToken,
    /// Progress so far. May be fractional and should increase with each update.
    pub progress: f64,
    /// Total amount of work, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,
    /// Optional human-readable progress message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl ProgressNotificationParams {
    /// Create progress params for `token` at `progress`, with no total/message.
    #[must_use]
    pub const fn new(progress_token: ProgressToken, progress: f64) -> Self {
        Self {
            progress_token,
            progress,
            total: None,
            message: None,
            meta: None,
        }
    }
}

/// Params for `notifications/cancelled` — a request-cancellation signal.
///
/// `request_id` is optional on the wire; per the spec it MUST be provided when
/// cancelling non-task requests (and MUST NOT be used to cancel tasks).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CancelledNotificationParams {
    /// The id of the request being cancelled.
    #[serde(rename = "requestId", default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    /// Optional human-readable cancellation reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn progress_params_round_trip_and_omit_absent() {
        let params = ProgressNotificationParams::new(ProgressToken::Number(1), 0.5);
        let wire = serde_json::to_value(&params).unwrap();
        assert_eq!(wire, json!({ "progressToken": 1, "progress": 0.5 }));
        let back: ProgressNotificationParams = serde_json::from_value(wire).unwrap();
        assert_eq!(back, params);

        let full = ProgressNotificationParams {
            total: Some(1.0),
            message: Some("half".into()),
            ..ProgressNotificationParams::new(ProgressToken::String("t".into()), 0.5)
        };
        let wire = serde_json::to_value(&full).unwrap();
        assert_eq!(
            wire,
            json!({ "progressToken": "t", "progress": 0.5, "total": 1.0, "message": "half" })
        );
    }

    #[test]
    fn cancelled_params_optional_request_id() {
        // Absent request_id serializes to `{}` and parses back.
        let empty = CancelledNotificationParams::default();
        assert_eq!(serde_json::to_value(&empty).unwrap(), json!({}));

        let params = CancelledNotificationParams {
            request_id: Some(RequestId::Number(7)),
            reason: Some("user aborted".into()),
            meta: None,
        };
        let wire = serde_json::to_value(&params).unwrap();
        assert_eq!(wire, json!({ "requestId": 7, "reason": "user aborted" }));
        let back: CancelledNotificationParams = serde_json::from_value(wire).unwrap();
        assert_eq!(back, params);
    }
}
