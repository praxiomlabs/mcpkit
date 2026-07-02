//! Content types for MCP messages.
//!
//! Content represents the payload in tool results, resource contents,
//! and prompt messages. MCP supports text, images, audio, and embedded resources.

use super::resource::ResourceContents;
use serde::{Deserialize, Serialize};

/// Content that can be included in messages and results.
///
/// Content is polymorphic - it can be text, images, audio, or
/// references to other resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    /// Plain text content.
    Text(TextContent),
    /// Image content (base64 encoded).
    Image(ImageContent),
    /// Audio content (base64 encoded).
    Audio(AudioContent),
    /// Embedded resource reference.
    Resource(ResourceContent),
    /// A link to a resource (not embedded).
    #[serde(rename = "resource_link")]
    ResourceLink(ResourceLinkContent),
}

impl Content {
    /// Create text content.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(TextContent {
            text: text.into(),
            annotations: None,
        })
    }

    /// Create text content with annotations.
    #[must_use]
    pub fn text_with_annotations(text: impl Into<String>, annotations: ContentAnnotations) -> Self {
        Self::Text(TextContent {
            text: text.into(),
            annotations: Some(annotations),
        })
    }

    /// Create image content from base64 data.
    #[must_use]
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Image(ImageContent {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
        })
    }

    /// Create audio content from base64 data.
    #[must_use]
    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Audio(AudioContent {
            data: data.into(),
            mime_type: mime_type.into(),
            annotations: None,
        })
    }

    /// Create a resource reference.
    #[must_use]
    pub fn resource(uri: impl Into<String>) -> Self {
        Self::Resource(ResourceContent {
            resource: ResourceContents {
                uri: uri.into(),
                mime_type: None,
                text: None,
                blob: None,
            },
            annotations: None,
        })
    }

    /// Check if this is text content.
    #[must_use]
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Check if this is image content.
    #[must_use]
    pub const fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    /// Check if this is audio content.
    #[must_use]
    pub const fn is_audio(&self) -> bool {
        matches!(self, Self::Audio(_))
    }

    /// Check if this is a resource reference.
    #[must_use]
    pub const fn is_resource(&self) -> bool {
        matches!(self, Self::Resource(_))
    }

    /// Create a resource link.
    #[must_use]
    pub fn resource_link(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self::ResourceLink(ResourceLinkContent {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            annotations: None,
        })
    }

    /// Check if this is a resource link.
    #[must_use]
    pub const fn is_resource_link(&self) -> bool {
        matches!(self, Self::ResourceLink(_))
    }

    /// Get the text if this is text content.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(&t.text),
            _ => None,
        }
    }
}

/// Text content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    /// The text content.
    pub text: String,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContentAnnotations>,
}

/// Image content (base64 encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    /// Base64-encoded image data.
    pub data: String,
    /// MIME type (e.g., "image/png", "image/jpeg").
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContentAnnotations>,
}

/// Audio content (base64 encoded).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioContent {
    /// Base64-encoded audio data.
    pub data: String,
    /// MIME type (e.g., "audio/wav", "audio/mp3").
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContentAnnotations>,
}

/// Embedded resource content.
///
/// The resource payload is nested under `resource` to match the spec's
/// `EmbeddedResource` (`{ "type": "resource", "resource": { "uri": .. } }`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    /// The embedded resource contents.
    pub resource: ResourceContents,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContentAnnotations>,
}

/// A link to a resource (not embedded inline, just a URI reference).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLinkContent {
    /// URI of the resource.
    pub uri: String,
    /// Name of the resource.
    pub name: String,
    /// Description of the resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ContentAnnotations>,
}

/// A tool call the model wants to make, in a sampling message
/// (`type: "tool_use"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseContent {
    /// Unique id correlating this call with its later `tool_result`.
    pub id: String,
    /// The name of the tool to call.
    pub name: String,
    /// The tool arguments.
    pub input: serde_json::Value,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<super::meta::Meta>,
}

/// The result of a tool call, fed back into a sampling message
/// (`type: "tool_result"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultContent {
    /// The id of the `tool_use` this result corresponds to.
    #[serde(rename = "toolUseId")]
    pub tool_use_id: String,
    /// The unstructured result content (normal content blocks).
    pub content: Vec<Content>,
    /// Optional structured result.
    #[serde(
        rename = "structuredContent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub structured_content: Option<serde_json::Value>,
    /// Whether the tool call ended in an error.
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<super::meta::Meta>,
}

/// Annotations that can be attached to content.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentAnnotations {
    /// Audience for this content (e.g., "user", "assistant").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,
    /// Priority level (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
}

impl ContentAnnotations {
    /// Create annotations for user-facing content.
    #[must_use]
    pub fn for_user() -> Self {
        Self {
            audience: Some(vec![Role::User]),
            priority: None,
        }
    }

    /// Create annotations for assistant-facing content.
    #[must_use]
    pub fn for_assistant() -> Self {
        Self {
            audience: Some(vec![Role::Assistant]),
            priority: None,
        }
    }

    /// Set the priority.
    #[must_use]
    pub fn with_priority(mut self, priority: f64) -> Self {
        self.priority = Some(priority.clamp(0.0, 1.0));
        self
    }
}

/// The role of a message participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// User/human participant.
    User,
    /// AI assistant participant.
    Assistant,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_content() {
        let content = Content::text("Hello, world!");
        assert!(content.is_text());
        assert_eq!(content.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_content_serialization() -> Result<(), Box<dyn std::error::Error>> {
        let content = Content::text("Test");
        let json = serde_json::to_string(&content)?;
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"text\":\"Test\""));
        Ok(())
    }

    #[test]
    fn test_image_content() -> Result<(), Box<dyn std::error::Error>> {
        let content = Content::image("base64data", "image/png");
        assert!(content.is_image());
        let json = serde_json::to_string(&content)?;
        assert!(json.contains("\"type\":\"image\""));
        assert!(json.contains("\"mimeType\":\"image/png\""));
        Ok(())
    }

    #[test]
    fn test_annotations() -> Result<(), Box<dyn std::error::Error>> {
        let annotations = ContentAnnotations::for_user().with_priority(0.8);
        assert_eq!(annotations.priority, Some(0.8));
        assert!(
            annotations
                .audience
                .as_ref()
                .ok_or("Expected audience")?
                .contains(&Role::User)
        );
        Ok(())
    }

    #[test]
    fn test_embedded_resource_nests_under_resource_key() -> Result<(), Box<dyn std::error::Error>> {
        // Spec EmbeddedResource: { "type": "resource", "resource": { "uri": .. } }
        let content = Content::resource("file:///main.rs");
        assert!(content.is_resource());
        let j = serde_json::to_value(&content)?;
        assert_eq!(j["type"], "resource");
        assert_eq!(j["resource"]["uri"], "file:///main.rs");
        // The uri must NOT be hoisted to the top level (the old, non-conformant shape).
        assert!(j.get("uri").is_none());

        // Round-trips from the spec shape.
        let back: Content = serde_json::from_value(j)?;
        assert!(back.is_resource());
        Ok(())
    }

    #[test]
    fn test_resource_link_content() -> Result<(), Box<dyn std::error::Error>> {
        let content = Content::resource_link("https://example.com/file.pdf", "My File");
        assert!(content.is_resource_link());
        let json = serde_json::to_string(&content)?;
        let parsed: Content = serde_json::from_str(&json)?;
        assert!(parsed.is_resource_link());
        assert!(json.contains("\"type\":\"resource_link\""));
        assert!(json.contains("\"uri\":\"https://example.com/file.pdf\""));
        assert!(json.contains("\"name\":\"My File\""));
        Ok(())
    }
}
