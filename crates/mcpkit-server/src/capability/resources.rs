//! Resource capability implementation.
//!
//! This module provides utilities for managing and serving resources
//! in an MCP server.

use crate::context::Context;
use crate::handler::ResourceHandler;
use mcpkit_core::error::McpError;
use mcpkit_core::types::resource::{Resource, ResourceContents, ResourceTemplate};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

/// A boxed async function for resource reading.
pub type BoxedResourceFn = Box<
    dyn for<'a> Fn(
            &'a str,
            &'a Context<'a>,
        )
            -> Pin<Box<dyn Future<Output = Result<ResourceContents, McpError>> + Send + 'a>>
        + Send
        + Sync,
>;

/// A registered resource with metadata and handler.
pub struct RegisteredResource {
    /// Resource metadata.
    pub resource: Resource,
    /// Handler function for reading.
    pub handler: BoxedResourceFn,
}

/// A registered resource template.
pub struct RegisteredTemplate {
    /// Template metadata.
    pub template: ResourceTemplate,
    /// Handler function for reading with URI parameters.
    pub handler: BoxedResourceFn,
}

/// Service for managing resources.
///
/// This provides a registry for static resources and dynamic
/// resource templates.
pub struct ResourceService {
    /// Static resources by URI.
    resources: HashMap<String, RegisteredResource>,
    /// Resource templates by URI pattern.
    templates: HashMap<String, RegisteredTemplate>,
}

impl Default for ResourceService {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceService {
    /// Create a new empty resource service.
    #[must_use]
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            templates: HashMap::new(),
        }
    }

    /// Register a static resource.
    pub fn register<F, Fut>(&mut self, resource: Resource, handler: F)
    where
        F: Fn(&str, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ResourceContents, McpError>> + Send + 'static,
    {
        let uri = resource.uri.clone();
        let boxed: BoxedResourceFn = Box::new(move |u, ctx| Box::pin(handler(u, ctx)));
        self.resources.insert(
            uri,
            RegisteredResource {
                resource,
                handler: boxed,
            },
        );
    }

    /// Register a resource template.
    pub fn register_template<F, Fut>(&mut self, template: ResourceTemplate, handler: F)
    where
        F: Fn(&str, &Context<'_>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ResourceContents, McpError>> + Send + 'static,
    {
        let pattern = template.uri_template.clone();
        let boxed: BoxedResourceFn = Box::new(move |u, ctx| Box::pin(handler(u, ctx)));
        self.templates.insert(
            pattern,
            RegisteredTemplate {
                template,
                handler: boxed,
            },
        );
    }

    /// Get a static resource by URI.
    #[must_use]
    pub fn get(&self, uri: &str) -> Option<&RegisteredResource> {
        self.resources.get(uri)
    }

    /// List all static resources.
    #[must_use]
    pub fn list(&self) -> Vec<&Resource> {
        self.resources.values().map(|r| &r.resource).collect()
    }

    /// List all resource templates.
    #[must_use]
    pub fn list_templates(&self) -> Vec<&ResourceTemplate> {
        self.templates.values().map(|r| &r.template).collect()
    }

    /// Read a resource by URI.
    ///
    /// This will first try static resources, then templates.
    pub async fn read(&self, uri: &str, ctx: &Context<'_>) -> Result<ResourceContents, McpError> {
        // Try static resources first
        if let Some(registered) = self.resources.get(uri) {
            return (registered.handler)(uri, ctx).await;
        }

        // Try templates
        for registered in self.templates.values() {
            if Self::matches_template(&registered.template.uri_template, uri) {
                return (registered.handler)(uri, ctx).await;
            }
        }

        Err(McpError::invalid_params(
            "resources/read",
            format!("Unknown resource: {uri}"),
        ))
    }

    /// Check if a URI matches a template pattern.
    ///
    /// Simple implementation supporting `{param}` placeholders.
    /// Uses prefix matching for templates with parameters.
    fn matches_template(template: &str, uri: &str) -> bool {
        // For now, do a simple prefix match
        // A full implementation would use proper URI template matching (RFC 6570)
        if template.contains('{') {
            let prefix = template.split('{').next().unwrap_or("");
            uri.starts_with(prefix)
        } else {
            template == uri
        }
    }

    /// Get the number of registered resources.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Get the number of registered templates.
    #[must_use]
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// Check if the service has no resources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.templates.is_empty()
    }
}

impl ResourceHandler for ResourceService {
    async fn list_resources(&self, _ctx: &Context<'_>) -> Result<Vec<Resource>, McpError> {
        Ok(self.list().into_iter().cloned().collect())
    }

    async fn read_resource(
        &self,
        uri: &str,
        ctx: &Context<'_>,
    ) -> Result<Vec<ResourceContents>, McpError> {
        Ok(vec![self.read(uri, ctx).await?])
    }
}

/// Builder for creating resources with a fluent API.
pub struct ResourceBuilder {
    uri: String,
    name: String,
    description: Option<String>,
    mime_type: Option<String>,
}

impl ResourceBuilder {
    /// Create a new resource builder.
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
        }
    }

    /// Set the resource description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the MIME type.
    pub fn mime_type(mut self, mime: impl Into<String>) -> Self {
        self.mime_type = Some(mime.into());
        self
    }

    /// Build the resource.
    #[must_use]
    pub fn build(self) -> Resource {
        Resource {
            uri: self.uri,
            name: self.name,
            description: self.description,
            mime_type: self.mime_type,
            size: None,
            annotations: None,
        }
    }
}

/// Builder for creating resource templates.
pub struct ResourceTemplateBuilder {
    uri_template: String,
    name: String,
    description: Option<String>,
    mime_type: Option<String>,
}

impl ResourceTemplateBuilder {
    /// Create a new template builder.
    pub fn new(uri_template: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri_template: uri_template.into(),
            name: name.into(),
            description: None,
            mime_type: None,
        }
    }

    /// Set the template description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the MIME type.
    pub fn mime_type(mut self, mime: impl Into<String>) -> Self {
        self.mime_type = Some(mime.into());
        self
    }

    /// Build the template.
    #[must_use]
    pub fn build(self) -> ResourceTemplate {
        ResourceTemplate {
            uri_template: self.uri_template,
            name: self.name,
            description: self.description,
            mime_type: self.mime_type,
            annotations: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_builder() {
        let resource = ResourceBuilder::new("file:///test.txt", "Test File")
            .description("A test file")
            .mime_type("text/plain")
            .build();

        assert_eq!(resource.uri, "file:///test.txt");
        assert_eq!(resource.name, "Test File");
        assert_eq!(resource.description.as_deref(), Some("A test file"));
        assert_eq!(resource.mime_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn test_template_builder() {
        let template = ResourceTemplateBuilder::new("myserver://data/{id}", "Data Item")
            .description("Access data by ID")
            .mime_type("application/json")
            .build();

        assert_eq!(template.uri_template, "myserver://data/{id}");
        assert_eq!(template.name, "Data Item");
    }

    #[test]
    fn test_template_matching() {
        assert!(ResourceService::matches_template(
            "myserver://data/{id}",
            "myserver://data/123"
        ));
        assert!(ResourceService::matches_template(
            "file:///config.json",
            "file:///config.json"
        ));
        assert!(!ResourceService::matches_template(
            "file:///other.json",
            "file:///config.json"
        ));
    }
}
