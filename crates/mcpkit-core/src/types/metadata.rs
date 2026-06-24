//! Shared display-metadata types.
//!
//! These types back the `icons` field that the MCP 2025-11-25 spec adds to
//! several objects (tools, resources, prompts, and the client/server
//! implementation info) via the shared `Icons` mixin.

use serde::{Deserialize, Serialize};

/// The background theme an icon is designed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconTheme {
    /// Designed for a light background.
    Light,
    /// Designed for a dark background.
    Dark,
}

/// An icon a client can display for a tool, resource, prompt, or
/// implementation.
///
/// # Example
///
/// ```rust
/// use mcpkit_core::types::{Icon, IconTheme};
///
/// let icon = Icon::new("https://example.com/icon.png")
///     .mime_type("image/png")
///     .sizes(["48x48", "96x96"])
///     .theme(IconTheme::Dark);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Icon {
    /// A URI pointing to the icon resource (an HTTP/HTTPS URL or a `data:`
    /// URI with Base64-encoded image data).
    pub src: String,
    /// Optional MIME type override if the source MIME type is missing or
    /// generic (e.g. `"image/png"`, `"image/svg+xml"`).
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional sizes the icon can be used at, each in `WxH` format (e.g.
    /// `"48x48"`) or `"any"` for scalable formats like SVG.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<String>>,
    /// Optional theme the icon is designed for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<IconTheme>,
}

impl Icon {
    /// Create a new icon from its source URI.
    #[must_use]
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            mime_type: None,
            sizes: None,
            theme: None,
        }
    }

    /// Set the MIME type override.
    #[must_use]
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Set the sizes the icon can be used at.
    #[must_use]
    pub fn sizes<I, S>(mut self, sizes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.sizes = Some(sizes.into_iter().map(Into::into).collect());
        self
    }

    /// Set the theme the icon is designed for.
    #[must_use]
    pub const fn theme(mut self, theme: IconTheme) -> Self {
        self.theme = Some(theme);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_serializes_with_camelcase_and_skips_none() {
        let icon = Icon::new("https://example.com/i.png");
        let j = serde_json::to_value(&icon).unwrap();
        assert_eq!(j["src"], "https://example.com/i.png");
        assert!(j.get("mimeType").is_none());
        assert!(j.get("sizes").is_none());
        assert!(j.get("theme").is_none());

        let icon = Icon::new("data:image/png;base64,AAAA")
            .mime_type("image/png")
            .sizes(["48x48", "any"])
            .theme(IconTheme::Dark);
        let j = serde_json::to_value(&icon).unwrap();
        assert_eq!(j["mimeType"], "image/png");
        assert_eq!(j["sizes"], serde_json::json!(["48x48", "any"]));
        assert_eq!(j["theme"], "dark");
    }
}
