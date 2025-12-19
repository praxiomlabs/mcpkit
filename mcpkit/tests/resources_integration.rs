//! Integration tests for resource handling.

use mcpkit::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit::protocol::RequestId;
use mcpkit::protocol_version::ProtocolVersion;
use mcpkit::types::resource::ResourceContents;
use mcpkit_server::capability::resources::{
    ResourceBuilder, ResourceService, ResourceTemplateBuilder,
};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::ResourceHandler;

fn make_test_context() -> (
    RequestId,
    ClientCapabilities,
    ServerCapabilities,
    ProtocolVersion,
    NoOpPeer,
) {
    (
        RequestId::Number(1),
        ClientCapabilities::default(),
        ServerCapabilities::default(),
        ProtocolVersion::LATEST,
        NoOpPeer,
    )
}

#[tokio::test]
async fn test_resource_service_basic() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    let resource = ResourceBuilder::new("file:///config.json", "Config")
        .description("Application configuration")
        .mime_type("application/json")
        .build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, r#"{"debug": true}"#)) }
    });

    assert!(!service.is_empty());
    assert_eq!(service.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_resource_read() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    let resource = ResourceBuilder::new("file:///data.txt", "Data File")
        .description("Sample data")
        .mime_type("text/plain")
        .build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "Hello, World!")) }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let result = service.read("file:///data.txt", &ctx).await;
    assert!(result.is_ok());

    let contents = result?;
    assert_eq!(contents.uri, "file:///data.txt");
    Ok(())
}

#[tokio::test]
async fn test_resource_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let service = ResourceService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let result = service.read("file:///nonexistent.txt", &ctx).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn test_resource_template() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    let template = ResourceTemplateBuilder::new("db://users/{id}", "User Record")
        .description("User data by ID")
        .mime_type("application/json")
        .build();

    service.register_template(template, |uri, _ctx| {
        let uri = uri.to_string();
        async move {
            // Extract ID from URI (simplified)
            let id = uri.strip_prefix("db://users/").unwrap_or("unknown");
            Ok(ResourceContents::text(
                uri.clone(),
                format!(r#"{{"id": "{id}", "name": "User {id}"}}"#),
            ))
        }
    });

    assert_eq!(service.template_count(), 1);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let result = service.read("db://users/123", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_resource_handler_trait() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    let resource = ResourceBuilder::new("mem://test", "Test Resource")
        .description("A test resource")
        .build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "test content")) }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Use the ResourceHandler trait
    let resources = service.list_resources(&ctx).await?;
    assert_eq!(resources.len(), 1);

    let contents = service.read_resource("mem://test", &ctx).await?;
    assert_eq!(contents.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_resource_builder() -> Result<(), Box<dyn std::error::Error>> {
    let resource = ResourceBuilder::new("file:///example.md", "Example")
        .description("An example file")
        .mime_type("text/markdown")
        .build();

    assert_eq!(resource.uri, "file:///example.md");
    assert_eq!(resource.name, "Example");
    assert_eq!(resource.description.as_deref(), Some("An example file"));
    assert_eq!(resource.mime_type.as_deref(), Some("text/markdown"));
    Ok(())
}

#[tokio::test]
async fn test_resource_template_builder() -> Result<(), Box<dyn std::error::Error>> {
    let template = ResourceTemplateBuilder::new("api://data/{category}/{id}", "API Data")
        .description("Fetch data from API")
        .mime_type("application/json")
        .build();

    assert_eq!(template.uri_template, "api://data/{category}/{id}");
    assert_eq!(template.name, "API Data");
    assert_eq!(template.description.as_deref(), Some("Fetch data from API"));
    Ok(())
}

#[tokio::test]
async fn test_multiple_resources() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    for i in 1..=5 {
        let resource =
            ResourceBuilder::new(format!("file:///file{i}.txt"), format!("File {i}")).build();

        service.register(resource, move |uri, _ctx| {
            let uri = uri.to_string();
            async move { Ok(ResourceContents::text(uri, "Content of file".to_string())) }
        });
    }

    assert_eq!(service.len(), 5);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let resources = service.list_resources(&ctx).await?;
    assert_eq!(resources.len(), 5);
    Ok(())
}

#[tokio::test]
async fn test_binary_resource() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    let resource = ResourceBuilder::new("file:///image.png", "Image")
        .mime_type("image/png")
        .build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move {
            // Simulate binary content as blob
            Ok(ResourceContents::blob(
                uri,
                &[0x89, 0x50, 0x4E, 0x47],
                "image/png",
            ))
        }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    let result = service.read("file:///image.png", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_resource_template_uri_matching() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    // Register a template with multiple path segments
    let template =
        ResourceTemplateBuilder::new("db://{database}/tables/{table}/rows/{id}", "DB Row")
            .description("Database row by ID")
            .mime_type("application/json")
            .build();

    service.register_template(template, |uri, _ctx| {
        let uri = uri.to_string();
        async move {
            Ok(ResourceContents::text(
                uri.clone(),
                format!(r#"{{"uri": "{uri}"}}"#),
            ))
        }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Should match with specific values
    let result = service.read("db://mydb/tables/users/rows/123", &ctx).await;
    assert!(result.is_ok());

    // Should also match with different values
    let result = service.read("db://prod/tables/orders/rows/456", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_resource_template_with_special_characters() -> Result<(), Box<dyn std::error::Error>>
{
    let mut service = ResourceService::new();

    // Template that might receive URL-encoded values
    let template = ResourceTemplateBuilder::new("search://{query}", "Search")
        .description("Search query")
        .build();

    service.register_template(template, |uri, _ctx| {
        let uri = uri.to_string();
        async move {
            Ok(ResourceContents::text(
                uri.clone(),
                "Search results".to_string(),
            ))
        }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Should work with simple query
    let result = service.read("search://hello", &ctx).await;
    assert!(result.is_ok());

    // Should work with query containing special chars (URL encoded)
    let result = service.read("search://hello%20world", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_resource_template_priority_over_exact() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    // Register an exact resource
    let resource = ResourceBuilder::new("file:///exact.txt", "Exact File").build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "Exact content".to_string())) }
    });

    // Register a template that could also match
    let template = ResourceTemplateBuilder::new("file:///{filename}", "Any File").build();

    service.register_template(template, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "Template content".to_string())) }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(
        &req_id,
        None,
        &client_caps,
        &server_caps,
        protocol_version,
        &peer,
    );

    // Exact match should take priority
    let result = service.read("file:///exact.txt", &ctx).await;
    assert!(result.is_ok());
    let contents = result?;
    // The exact resource handler returns "Exact content"
    let text = contents.text.as_ref().ok_or("Expected text content")?;
    assert!(text.contains("Exact") || text.contains("exact"));

    // Non-exact should fall through to template
    let result = service.read("file:///other.txt", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_list_resource_templates() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    // Register multiple templates
    let template1 = ResourceTemplateBuilder::new("api://v1/{endpoint}", "API v1")
        .description("Version 1 API")
        .build();

    let template2 = ResourceTemplateBuilder::new("api://v2/{endpoint}", "API v2")
        .description("Version 2 API")
        .build();

    service.register_template(template1, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "v1 response".to_string())) }
    });

    service.register_template(template2, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "v2 response".to_string())) }
    });

    let templates = service.list_templates();
    assert_eq!(templates.len(), 2);

    // Check template names
    let names: Vec<_> = templates.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"API v1"));
    assert!(names.contains(&"API v2"));
    Ok(())
}
