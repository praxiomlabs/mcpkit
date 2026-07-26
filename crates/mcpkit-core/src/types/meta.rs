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
/// `_meta` is an open, string-keyed map, but key *names* are constrained — see
/// [`validate_key`]. In particular the spec reserves any prefix whose **second
/// label** is `modelcontextprotocol` or `mcp`: `io.modelcontextprotocol/`,
/// `dev.mcp/`, `org.modelcontextprotocol.api/` and `com.mcp.tools/` are all
/// reserved, while `com.example.mcp/` is not. Namespace your own keys under a
/// domain you control.
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

    /// Attach `_meta.progressToken` to (possibly absent) request params so the
    /// server will emit progress notifications for the call.
    ///
    /// Existing params and any existing `_meta` object entries are preserved; a
    /// missing params object becomes `{ "_meta": { "progressToken": … } }`.
    /// Params that are not a JSON object, or a pre-existing non-object `_meta`
    /// (both invalid for MCP), are replaced so the token is always attached.
    #[must_use]
    pub fn with_progress_token_in_params(params: Option<Value>, token: &ProgressToken) -> Value {
        let mut obj = match params {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        let meta = obj
            .entry("_meta")
            .or_insert_with(|| Value::Object(Map::new()));
        // A pre-existing non-object `_meta` is malformed; replace it so the token
        // is always inserted (the helper guarantees progress wiring).
        if !meta.is_object() {
            *meta = Value::Object(Map::new());
        }
        if let Value::Object(meta_obj) = meta {
            meta_obj.insert(
                PROGRESS_TOKEN_KEY.to_string(),
                serde_json::to_value(token).unwrap_or(Value::Null),
            );
        }
        Value::Object(obj)
    }
}

/// Why a `_meta` key is not valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaKeyError {
    /// The prefix uses a label the spec's grammar does not allow.
    ///
    /// Labels must start with a letter, end with a letter or digit, and use
    /// only letters, digits and hyphens in between.
    InvalidPrefixLabel(String),
    /// The prefix is reserved for MCP: its second label is `modelcontextprotocol`
    /// or `mcp`.
    ReservedPrefix(String),
    /// The name segment is not allowed.
    ///
    /// Unless empty it must begin and end alphanumeric, and may contain
    /// hyphens, underscores, dots and alphanumerics in between.
    InvalidName(String),
}

impl std::fmt::Display for MetaKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPrefixLabel(l) => {
                write!(
                    f,
                    "invalid `_meta` prefix label `{l}`: labels start with a letter, end alphanumeric, and contain only letters, digits and hyphens"
                )
            }
            Self::ReservedPrefix(p) => write!(
                f,
                "`_meta` prefix `{p}` is reserved for MCP (second label is `modelcontextprotocol` or `mcp`)"
            ),
            Self::InvalidName(n) => write!(
                f,
                "invalid `_meta` name `{n}`: must begin and end alphanumeric, with hyphens, underscores, dots or alphanumerics between"
            ),
        }
    }
}

impl std::error::Error for MetaKeyError {}

fn valid_prefix_label(label: &str) -> bool {
    let mut chars = label.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    // Interior may be alphanumeric or `-`; the final character may not be `-`.
    if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }
    label
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_alphanumeric())
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() {
        return true; // "Unless empty" — an empty name is allowed.
    }
    let ok_edge = |c: char| c.is_ascii_alphanumeric();
    let first = name.chars().next().is_some_and(ok_edge);
    let last = name.chars().last().is_some_and(ok_edge);
    let interior = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    first && last && interior
}

/// Check a `_meta` key against the 2025-11-25 key-name rules.
///
/// A key is an optional dot-separated **prefix** ending in `/`, followed by a
/// **name**. Any prefix whose second label is `modelcontextprotocol` or `mcp` is
/// reserved for MCP, so an implementation must not invent keys there.
///
/// This is offered rather than enforced: [`Meta::insert`] stays permissive so a
/// peer's key is never rejected on receipt. Use this when *producing* keys.
///
/// # Errors
///
/// Returns [`MetaKeyError`] describing which rule the key breaks.
///
/// # Examples
///
/// ```
/// use mcpkit_core::types::meta::validate_key;
///
/// assert!(validate_key("com.example/trace-id").is_ok());
/// assert!(validate_key("progressToken").is_ok());       // no prefix
/// assert!(validate_key("io.modelcontextprotocol/x").is_err()); // reserved
/// assert!(validate_key("com.example.mcp/x").is_ok());   // second label is `example`
/// ```
pub fn validate_key(key: &str) -> Result<(), MetaKeyError> {
    let (prefix, name) = match key.rsplit_once('/') {
        Some((p, n)) => (Some(p), n),
        None => (None, key),
    };

    if let Some(prefix) = prefix {
        let labels: Vec<&str> = prefix.split('.').collect();
        for label in &labels {
            if !valid_prefix_label(label) {
                return Err(MetaKeyError::InvalidPrefixLabel((*label).to_string()));
            }
        }
        // Reserved when the SECOND label is `modelcontextprotocol` or `mcp` —
        // so `io.modelcontextprotocol/` is reserved but `com.example.mcp/` is not.
        if labels
            .get(1)
            .is_some_and(|l| *l == "modelcontextprotocol" || *l == "mcp")
        {
            return Err(MetaKeyError::ReservedPrefix(prefix.to_string()));
        }
    }

    if !valid_name(name) {
        return Err(MetaKeyError::InvalidName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    /// The spec's own worked examples, verbatim from `basic/index.mdx`:
    /// "`io.modelcontextprotocol/`, `dev.mcp/`, `org.modelcontextprotocol.api/`,
    /// and `com.mcp.tools/` are all reserved. However, `com.example.mcp/` is NOT
    /// reserved, as the second label is `example`."
    #[test]
    fn reserved_prefixes_match_the_spec_examples() {
        for reserved in [
            "io.modelcontextprotocol/x",
            "dev.mcp/x",
            "org.modelcontextprotocol.api/x",
            "com.mcp.tools/x",
        ] {
            assert!(
                matches!(validate_key(reserved), Err(MetaKeyError::ReservedPrefix(_))),
                "{reserved} must be reserved"
            );
        }
        // Second label is `example`, so not reserved despite containing `mcp`.
        assert!(validate_key("com.example.mcp/x").is_ok());
    }

    #[test]
    fn prefix_label_grammar_is_enforced() {
        assert!(validate_key("com.example/k").is_ok());
        assert!(validate_key("com.ex-ample/k").is_ok());
        // Must start with a letter.
        assert!(matches!(
            validate_key("1com.example/k"),
            Err(MetaKeyError::InvalidPrefixLabel(_))
        ));
        // Must not end with a hyphen.
        assert!(matches!(
            validate_key("com.example-/k"),
            Err(MetaKeyError::InvalidPrefixLabel(_))
        ));
        // Empty label.
        assert!(matches!(
            validate_key("com..example/k"),
            Err(MetaKeyError::InvalidPrefixLabel(_))
        ));
    }

    #[test]
    fn name_grammar_is_enforced() {
        assert!(validate_key("progressToken").is_ok(), "no prefix is fine");
        assert!(validate_key("com.example/a-b_c.d").is_ok());
        // Empty name is explicitly allowed ("Unless empty").
        assert!(validate_key("com.example/").is_ok());
        for bad in ["com.example/-x", "com.example/x-", "com.example/x y"] {
            assert!(
                matches!(validate_key(bad), Err(MetaKeyError::InvalidName(_))),
                "{bad} must be rejected"
            );
        }
    }

    /// The well-known key mcpkit itself relies on must pass its own validator.
    #[test]
    fn mcpkits_own_keys_validate() {
        assert!(validate_key(PROGRESS_TOKEN_KEY).is_ok());
        // The spec-defined related-task key is reserved *and that is correct* —
        // it is MCP's own key, not one an implementation may invent.
        assert!(matches!(
            validate_key(crate::tasks::RELATED_TASK_META_KEY),
            Err(MetaKeyError::ReservedPrefix(_))
        ));
    }

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
    fn with_progress_token_in_params_injects_and_preserves() {
        // Absent params -> just `_meta.progressToken`.
        let out = Meta::with_progress_token_in_params(None, &ProgressToken::Number(3));
        assert_eq!(out, json!({ "_meta": { "progressToken": 3 } }));

        // Existing params and existing `_meta` entries are preserved.
        let params = json!({ "name": "t", "_meta": { "keep": true } });
        let out =
            Meta::with_progress_token_in_params(Some(params), &ProgressToken::String("x".into()));
        assert_eq!(
            out,
            json!({ "name": "t", "_meta": { "keep": true, "progressToken": "x" } })
        );

        // A malformed non-object `_meta` is replaced so the token still attaches.
        let params = json!({ "name": "t", "_meta": false });
        let out = Meta::with_progress_token_in_params(Some(params), &ProgressToken::Number(1));
        assert_eq!(out, json!({ "name": "t", "_meta": { "progressToken": 1 } }));
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
