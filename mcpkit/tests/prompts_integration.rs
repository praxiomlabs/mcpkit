//! Integration tests for prompt handling.

use mcpkit::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit::protocol::RequestId;
use mcpkit::protocol_version::ProtocolVersion;
use mcpkit::types::prompt::PromptMessage;
use mcpkit_server::capability::prompts::{PromptBuilder, PromptResultBuilder, PromptService};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::PromptHandler;
use serde_json::Value;

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
async fn test_prompt_service_basic() {
    let mut service = PromptService::new();

    let prompt = PromptBuilder::new("greeting")
        .description("Generate a greeting")
        .required_arg("name", "Name to greet")
        .build();

    service.register(prompt, |args, _ctx| async move {
        let name = args
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
            .unwrap_or_else(|| "World".to_string());

        Ok(PromptResultBuilder::new()
            .user_text(format!("Hello, {name}!"))
            .build())
    });

    assert!(service.contains("greeting"));
    assert_eq!(service.len(), 1);
}

#[tokio::test]
async fn test_prompt_render() {
    let mut service = PromptService::new();

    let prompt = PromptBuilder::new("code_review")
        .description("Review code for issues")
        .required_arg("code", "The code to review")
        .optional_arg("language", "Programming language")
        .build();

    service.register(prompt, |args, _ctx| async move {
        let code = args
            .as_ref()
            .and_then(|v| v.get("code").and_then(|c| c.as_str()))
            .unwrap_or("No code provided")
            .to_string();

        let language = args
            .as_ref()
            .and_then(|v| v.get("language").and_then(|l| l.as_str()))
            .unwrap_or("unknown")
            .to_string();

        Ok(PromptResultBuilder::new()
            .description("Code review prompt")
            .user_text(format!(
                "Please review the following {language} code:\n{code}"
            ))
            .build())
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service
        .render(
            "code_review",
            Some(serde_json::json!({"code": "fn main() {}", "language": "Rust"})),
            &ctx,
        )
        .await;

    assert!(result.is_ok());
    let prompt_result = result.unwrap();
    assert!(!prompt_result.messages.is_empty());
}

#[tokio::test]
async fn test_prompt_not_found() {
    let service = PromptService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.render("nonexistent", None, &ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_prompt_handler_trait() {
    let mut service = PromptService::new();

    let prompt = PromptBuilder::new("summarize")
        .description("Summarize text")
        .required_arg("text", "Text to summarize")
        .build();

    service.register(prompt, |args, _ctx| async move {
        let text = args
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default();

        Ok(PromptResultBuilder::new()
            .user_text(format!("Please summarize: {text}"))
            .build())
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // Use the PromptHandler trait
    let prompts = service.list_prompts(&ctx).await.unwrap();
    assert_eq!(prompts.len(), 1);

    let mut args = serde_json::Map::new();
    args.insert(
        "text".to_string(),
        Value::String("Long text...".to_string()),
    );

    let result = service.get_prompt("summarize", Some(args), &ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_prompt_builder() {
    let prompt = PromptBuilder::new("test_prompt")
        .description("A test prompt")
        .required_arg("input", "Required input")
        .optional_arg("format", "Output format")
        .build();

    assert_eq!(prompt.name, "test_prompt");
    assert_eq!(prompt.description.as_deref(), Some("A test prompt"));

    let args = prompt.arguments.unwrap();
    assert_eq!(args.len(), 2);

    let required_arg = args.iter().find(|a| a.name == "input").unwrap();
    assert_eq!(required_arg.required, Some(true));

    let optional_arg = args.iter().find(|a| a.name == "format").unwrap();
    assert_eq!(optional_arg.required, Some(false));
}

#[tokio::test]
async fn test_prompt_result_builder() {
    let result = PromptResultBuilder::new()
        .description("Test result")
        .user_text("User message")
        .assistant_text("Assistant response")
        .build();

    assert_eq!(result.description.as_deref(), Some("Test result"));
    assert_eq!(result.messages.len(), 2);
}

#[tokio::test]
async fn test_multiple_prompts() {
    let mut service = PromptService::new();

    for name in ["analyze", "translate", "explain", "debug"] {
        let prompt = PromptBuilder::new(name)
            .description(format!("{name} something"))
            .build();

        service.register(prompt, |_args, _ctx| async {
            Ok(PromptResultBuilder::new().user_text("Test").build())
        });
    }

    assert_eq!(service.len(), 4);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let prompts = service.list_prompts(&ctx).await.unwrap();
    assert_eq!(prompts.len(), 4);
}

#[tokio::test]
async fn test_prompt_with_no_args() {
    let mut service = PromptService::new();

    let prompt = PromptBuilder::new("help").description("Get help").build();

    service.register(prompt, |_args, _ctx| async {
        Ok(PromptResultBuilder::new()
            .user_text("How can I help you?")
            .build())
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.render("help", None, &ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_prompt_messages() {
    // Test PromptMessage creation
    let user_msg = PromptMessage::user("User content");
    let assistant_msg = PromptMessage::assistant("Assistant content");

    // Messages should have valid roles
    // Just verify they're created successfully and are different
    let user_json = serde_json::to_value(&user_msg).unwrap();
    let assistant_json = serde_json::to_value(&assistant_msg).unwrap();

    assert_eq!(user_json["role"], "user");
    assert_eq!(assistant_json["role"], "assistant");
}
