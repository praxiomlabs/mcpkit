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
async fn test_resource_service_basic() {
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
}

#[tokio::test]
async fn test_resource_read() {
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
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.read("file:///data.txt", &ctx).await;
    assert!(result.is_ok());

    let contents = result.unwrap();
    assert_eq!(contents.uri, "file:///data.txt");
}

#[tokio::test]
async fn test_resource_not_found() {
    let service = ResourceService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.read("file:///nonexistent.txt", &ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_resource_template() {
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
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.read("db://users/123", &ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_resource_handler_trait() {
    let mut service = ResourceService::new();

    let resource = ResourceBuilder::new("mem://test", "Test Resource")
        .description("A test resource")
        .build();

    service.register(resource, |uri, _ctx| {
        let uri = uri.to_string();
        async move { Ok(ResourceContents::text(uri, "test content")) }
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // Use the ResourceHandler trait
    let resources = service.list_resources(&ctx).await.unwrap();
    assert_eq!(resources.len(), 1);

    let contents = service.read_resource("mem://test", &ctx).await.unwrap();
    assert_eq!(contents.len(), 1);
}

#[tokio::test]
async fn test_resource_builder() {
    let resource = ResourceBuilder::new("file:///example.md", "Example")
        .description("An example file")
        .mime_type("text/markdown")
        .build();

    assert_eq!(resource.uri, "file:///example.md");
    assert_eq!(resource.name, "Example");
    assert_eq!(resource.description.as_deref(), Some("An example file"));
    assert_eq!(resource.mime_type.as_deref(), Some("text/markdown"));
}

#[tokio::test]
async fn test_resource_template_builder() {
    let template = ResourceTemplateBuilder::new("api://data/{category}/{id}", "API Data")
        .description("Fetch data from API")
        .mime_type("application/json")
        .build();

    assert_eq!(template.uri_template, "api://data/{category}/{id}");
    assert_eq!(template.name, "API Data");
    assert_eq!(template.description.as_deref(), Some("Fetch data from API"));
}

#[tokio::test]
async fn test_multiple_resources() {
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
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let resources = service.list_resources(&ctx).await.unwrap();
    assert_eq!(resources.len(), 5);
}

#[tokio::test]
async fn test_binary_resource() {
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
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.read("file:///image.png", &ctx).await;
    assert!(result.is_ok());
}
