//! Protocol `_meta` metadata.
//!
//! The MCP 2025-11-25 schema attaches an optional `_meta` object to request
//! params, notification params, and results. It is an open, string-keyed map for
//! protocol- and implementation-defined metadata. This module provides the
//! [`Meta`] type plus helpers for the one well-known key, `progressToken`.

use crate::protocol::ProgressToken;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The well-known request `_meta` key carrying a progress token.
const PROGRESS_TOKEN_KEY: &str = "progressToken";

/// The `_meta` field carried by MCP requests, notifications, and results.
///
/// `_meta` is an open, string-keyed map. Keys beginning with
/// `modelcontextprotocol.io/` (and the bare `modelcontextprotocol.io` label) are
/// reserved by the MCP spec; namespace your own keys (e.g. by a domain you
/// control) to avoid collisions.
///
/// On a **request**, `_meta.progressToken` associates progress notifications
/// with the call — see [`with_progress_token`](Self::with_progress_token) and
/// [`progress_token`](Self::progress_token).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Meta(pub Map<String, Value>);

impl Meta {
    /// Create an empty `_meta` map.
    #[must_use]
    pub fn new() -> Self {
        Self(Map::new())
    }

    /// Whether there are no metadata entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get a raw metadata value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// Iterate over the `(key, value)` metadata entries.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.0.iter()
    }

    /// Insert a raw metadata entry, returning the previous value if any.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    /// Insert a raw metadata entry, returning `self` for chaining.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: Value) -> Self {
        self.0.insert(key.into(), value);
        self
    }

    /// The request progress token (`_meta.progressToken`), if present and valid.
    #[must_use]
    pub fn progress_token(&self) -> Option<ProgressToken> {
        self.0
            .get(PROGRESS_TOKEN_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Set the progress token.
    pub fn set_progress_token(&mut self, token: ProgressToken) {
        // `ProgressToken` serializes to a string or number, so this is infallible.
        if let Ok(value) = serde_json::to_value(token) {
            self.0.insert(PROGRESS_TOKEN_KEY.to_string(), value);
        }
    }

    /// Set the progress token, returning `self` for chaining.
    #[must_use]
    pub fn with_progress_token(mut self, token: ProgressToken) -> Self {
        self.set_progress_token(token);
        self
    }

    /// Extract a request's progress token directly from raw params
    /// (`params._meta.progressToken`) without deserializing the whole `_meta`.
    ///
    /// This is the typed replacement for hand-parsing progress tokens out of raw
    /// request params.
    #[must_use]
    pub fn progress_token_from_params(params: &Value) -> Option<ProgressToken> {
        params
            .get("_meta")?
            .get(PROGRESS_TOKEN_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn progress_token_round_trips() {
        let meta = Meta::new().with_progress_token(ProgressToken::String("abc".into()));
        let wire = serde_json::to_value(&meta).unwrap();
        assert_eq!(wire, json!({ "progressToken": "abc" }));
        let back: Meta = serde_json::from_value(wire).unwrap();
        assert_eq!(
            back.progress_token(),
            Some(ProgressToken::String("abc".into()))
        );
    }

    #[test]
    fn numeric_progress_token_round_trips() {
        let meta = Meta::new().with_progress_token(ProgressToken::Number(7));
        assert_eq!(meta.progress_token(), Some(ProgressToken::Number(7)));
    }

    #[test]
    fn extracts_progress_token_from_raw_params() {
        let params = json!({ "name": "t", "_meta": { "progressToken": 42 } });
        assert_eq!(
            Meta::progress_token_from_params(&params),
            Some(ProgressToken::Number(42))
        );
        // No _meta -> None.
        assert_eq!(
            Meta::progress_token_from_params(&json!({ "name": "t" })),
            None
        );
    }

    #[test]
    fn empty_meta_is_empty() {
        assert!(Meta::new().is_empty());
        assert!(!Meta::new().with("k", json!(1)).is_empty());
    }

    #[test]
    fn iter_yields_entries() {
        let meta = Meta::new().with("a", json!(1)).with("b", json!(2));
        let mut keys: Vec<&str> = meta.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["a", "b"]);
    }

    #[test]
    fn result_meta_serializes_as_underscore_meta_and_omits_when_none() {
        use crate::types::CallToolResult;

        // Present -> serialized under `_meta`, and round-trips back.
        let with_meta = CallToolResult {
            meta: Some(Meta::new().with("acme.com/trace", json!("id-1"))),
            ..CallToolResult::text("ok")
        };
        let wire = serde_json::to_value(&with_meta).unwrap();
        assert_eq!(wire["_meta"], json!({ "acme.com/trace": "id-1" }));
        let back: CallToolResult = serde_json::from_value(wire).unwrap();
        assert_eq!(
            back.meta.and_then(|m| m.get("acme.com/trace").cloned()),
            Some(json!("id-1"))
        );

        // Absent -> `_meta` omitted from the wire.
        let no_meta = serde_json::to_value(CallToolResult::text("ok")).unwrap();
        assert!(no_meta.get("_meta").is_none());
    }
}
