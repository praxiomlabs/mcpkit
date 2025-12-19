//! Integration tests for resource handling.

use mcpkit_core::types::resource::{Resource, ResourceContents, ResourceTemplate};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::capability::resources::{ResourceBuilder, ResourceService, ResourceTemplateBuilder};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::ResourceHandler;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;

fn make_test_context() -> (RequestId, ClientCapabilities, ServerCapabilities, ProtocolVersion, NoOpPeer) {
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

    service.register(resource, |_uri, _ctx| async {
        Ok(ResourceContents::text(
            "file:///config.json",
            r#"{"debug": true}"#,
        ))
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

    service.register(resource, |uri, _ctx| async move {
        Ok(ResourceContents::text(uri, "Hello, World!"))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

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
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

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

    service.register_template(template, |uri, _ctx| async move {
        // Extract ID from URI (simplified)
        let id = uri.strip_prefix("db://users/").unwrap_or("unknown");
        Ok(ResourceContents::text(
            uri,
            format!(r#"{{"id": "{}", "name": "User {}"}}"#, id, id),
        ))
    });

    assert_eq!(service.template_count(), 1);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

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

    service.register(resource, |uri, _ctx| async move {
        Ok(ResourceContents::text(uri, "test content"))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

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
        let resource = ResourceBuilder::new(
            format!("file:///file{}.txt", i),
            format!("File {}", i),
        )
        .build();

        let i_copy = i;
        service.register(resource, move |uri, _ctx| async move {
            Ok(ResourceContents::text(uri, format!("Content of file {}", i_copy)))
        });
    }

    assert_eq!(service.len(), 5);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

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

    service.register(resource, |uri, _ctx| async move {
        // Simulate binary content as base64
        Ok(ResourceContents::blob(uri, vec![0x89, 0x50, 0x4E, 0x47]))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.read("file:///image.png", &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_list_resource_templates() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = ResourceService::new();

    // Register a static resource
    let resource = ResourceBuilder::new("config://server", "Server Config")
        .description("Static configuration")
        .mime_type("application/json")
        .build();

    service.register(resource, |uri, _ctx| async move {
        Ok(ResourceContents::text(uri, r#"{"port": 8080}"#))
    });

    // Register a template resource
    let template = ResourceTemplateBuilder::new("file://{path}", "File Contents")
        .description("Read any file by path")
        .mime_type("text/plain")
        .build();

    service.register_template(template, |uri, _ctx| async move {
        Ok(ResourceContents::text(uri, "file contents"))
    });

    // Register another template
    let template2 = ResourceTemplateBuilder::new("db://users/{id}", "User Record")
        .description("User data by ID")
        .mime_type("application/json")
        .build();

    service.register_template(template2, |uri, _ctx| async move {
        let id = uri.strip_prefix("db://users/").unwrap_or("unknown");
        Ok(ResourceContents::text(uri, format!(r#"{{"id": "{}"}}"#, id)))
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // list_resources should only return static resources
    let resources = service.list_resources(&ctx).await?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "config://server");

    // list_resource_templates should return the template resources
    let templates = service.list_resource_templates(&ctx).await?;
    assert_eq!(templates.len(), 2);
    assert!(templates.iter().any(|t| t.uri_template == "file://{path}"));
    assert!(templates.iter().any(|t| t.uri_template == "db://users/{id}"));
    Ok(())
}
