//! Resource types for MCP servers.
//!
//! Resources represent data that MCP servers expose to AI assistants.
//! They can be files, database entries, API responses, or any other
//! addressable content.

use serde::{Deserialize, Serialize};

/// A resource exposed by an MCP server.
///
/// Resources are identified by URIs and can represent various types
/// of data: files, database entries, API endpoints, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// URI identifying the resource (e.g., "<file:///path>", "<myserver://data/123>").
    pub uri: String,
    /// Human-readable name for the resource.
    pub name: String,
    /// Description of what the resource contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource content.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Size in bytes, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ResourceAnnotations>,
}

impl Resource {
    /// Create a new resource with a URI and name.
    #[must_use]
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            size: None,
            annotations: None,
        }
    }

    /// Set the resource description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    #[must_use]
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Set the size.
    #[must_use]
    pub const fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Set the resource description (alias for chaining).
    #[must_use]
    pub fn with_description(self, description: impl Into<String>) -> Self {
        self.description(description)
    }

    /// Set the MIME type (alias for chaining).
    #[must_use]
    pub fn with_mime_type(self, mime_type: impl Into<String>) -> Self {
        self.mime_type(mime_type)
    }
}

/// Annotations for resources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceAnnotations {
    /// Audience for this resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    /// Priority level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
}

/// A template for dynamic resource URIs.
///
/// Resource templates allow servers to expose parameterized resources
/// where the URI contains placeholders like `{id}` or `{query}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTemplate {
    /// URI template with placeholders (e.g., "<myserver://users/{userId>}").
    #[serde(rename = "uriTemplate")]
    pub uri_template: String,
    /// Human-readable name for this resource type.
    pub name: String,
    /// Description of the resource template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of resources matching this template.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ResourceAnnotations>,
}

impl ResourceTemplate {
    /// Create a new resource template.
    #[must_use]
    pub fn new(uri_template: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri_template: uri_template.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            annotations: None,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the MIME type.
    #[must_use]
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

/// The contents of a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContents {
    /// URI of the resource.
    pub uri: String,
    /// MIME type of the content.
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Text content (mutually exclusive with blob).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Binary content as base64 (mutually exclusive with text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

impl ResourceContents {
    /// Create text resource contents.
    #[must_use]
    pub fn text(uri: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: Some("text/plain".to_string()),
            text: Some(text.into()),
            blob: None,
        }
    }

    /// Create JSON resource contents.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn json<T: Serialize>(
        uri: impl Into<String>,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_string_pretty(value)?;
        Ok(Self {
            uri: uri.into(),
            mime_type: Some("application/json".to_string()),
            text: Some(json),
            blob: None,
        })
    }

    /// Create binary resource contents.
    #[must_use]
    pub fn blob(uri: impl Into<String>, data: &[u8], mime_type: impl Into<String>) -> Self {
        use base64::Engine;
        Self {
            uri: uri.into(),
            mime_type: Some(mime_type.into()),
            text: None,
            blob: Some(base64::engine::general_purpose::STANDARD.encode(data)),
        }
    }

    /// Check if this is text content.
    #[must_use]
    pub const fn is_text(&self) -> bool {
        self.text.is_some()
    }

    /// Check if this is binary content.
    #[must_use]
    pub const fn is_blob(&self) -> bool {
        self.blob.is_some()
    }

    /// Get the text content.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// Decode and get the binary content.
    ///
    /// # Errors
    ///
    /// Returns an error if base64 decoding fails.
    pub fn decode_blob(&self) -> Result<Option<Vec<u8>>, base64::DecodeError> {
        use base64::Engine;
        self.blob
            .as_ref()
            .map(|b| base64::engine::general_purpose::STANDARD.decode(b))
            .transpose()
    }
}

/// Request parameters for listing resources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListResourcesRequest {
    /// Cursor for pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for listing resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResult {
    /// The list of available resources.
    pub resources: Vec<Resource>,
    /// Cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Response for listing resource templates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourceTemplatesResult {
    /// The list of resource templates.
    #[serde(rename = "resourceTemplates")]
    pub resource_templates: Vec<ResourceTemplate>,
    /// Cursor for the next page.
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Request parameters for reading a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceRequest {
    /// URI of the resource to read.
    pub uri: String,
}

/// Response for reading a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    /// The resource contents.
    pub contents: Vec<ResourceContents>,
}

/// Notification that a resource has changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedNotification {
    /// URI of the updated resource.
    pub uri: String,
}

/// Notification that the resource list has changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceListChangedNotification {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_builder() {
        let resource = Resource::new("file:///test.txt", "Test File")
            .description("A test file")
            .mime_type("text/plain")
            .size(1024);

        assert_eq!(resource.uri, "file:///test.txt");
        assert_eq!(resource.name, "Test File");
        assert_eq!(resource.size, Some(1024));
    }

    #[test]
    fn test_resource_template() {
        let template = ResourceTemplate::new("myserver://users/{userId}", "User")
            .description("A user record")
            .mime_type("application/json");

        assert!(template.uri_template.contains("{userId}"));
    }

    #[test]
    fn test_resource_contents_text() {
        let contents = ResourceContents::text("test://resource", "Hello, world!");
        assert!(contents.is_text());
        assert!(!contents.is_blob());
        assert_eq!(contents.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_resource_contents_blob() {
        let data = b"binary data";
        let contents = ResourceContents::blob("test://binary", data, "application/octet-stream");
        assert!(contents.is_blob());
        assert!(!contents.is_text());

        let decoded = contents.decode_blob().unwrap().unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_resource_contents_json() {
        #[derive(Serialize)]
        struct Data {
            name: String,
            value: i32,
        }

        let data = Data {
            name: "test".to_string(),
            value: 42,
        };
        let contents = ResourceContents::json("test://json", &data).unwrap();
        assert!(contents.is_text());
        assert!(contents.as_text().unwrap().contains("\"name\""));
    }
}
