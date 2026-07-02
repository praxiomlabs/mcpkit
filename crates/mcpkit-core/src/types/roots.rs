//! Roots — the filesystem/URI roots a client exposes to servers.

use super::meta::Meta;
use serde::{Deserialize, Serialize};

/// A root a client exposes to servers (typically a project directory).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// URI of the root (e.g. `file:///home/user/project`).
    pub uri: String,
    /// Optional human-readable name for the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Root {
    /// Create a root with the given URI and no name.
    #[must_use]
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: None,
        }
    }

    /// Set the human-readable name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Result of a `roots/list` request — the roots the client exposes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListRootsResult {
    /// The client's roots.
    pub roots: Vec<Root>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// Notification that the client's root list has changed
/// (`notifications/roots/list_changed`).
///
/// The notification is sent with no params; this type is a marker for coverage
/// and deserialization symmetry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RootsListChangedNotification {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn root_omits_name_when_absent() {
        assert_eq!(
            serde_json::to_value(Root::new("file:///p")).unwrap(),
            json!({ "uri": "file:///p" })
        );
        assert_eq!(
            serde_json::to_value(Root::new("file:///p").name("proj")).unwrap(),
            json!({ "uri": "file:///p", "name": "proj" })
        );
    }

    #[test]
    fn list_roots_result_round_trips_and_omits_meta() {
        let result = ListRootsResult {
            roots: vec![Root::new("file:///a"), Root::new("file:///b").name("b")],
            meta: None,
        };
        let wire = serde_json::to_value(&result).unwrap();
        assert_eq!(
            wire,
            json!({ "roots": [ { "uri": "file:///a" }, { "uri": "file:///b", "name": "b" } ] })
        );
        let back: ListRootsResult = serde_json::from_value(wire).unwrap();
        assert_eq!(back.roots.len(), 2);
        assert_eq!(back.roots[1].name.as_deref(), Some("b"));
    }
}
