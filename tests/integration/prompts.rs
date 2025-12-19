//! Integration tests for prompt handling.

use mcpkit_core::types::prompt::{GetPromptResult, Prompt, PromptArgument, PromptMessage};
use mcpkit_core::protocol_version::ProtocolVersion;
use mcpkit_server::capability::prompts::{PromptBuilder, PromptResultBuilder, PromptService};
use mcpkit_server::context::{Context, NoOpPeer};
use mcpkit_server::handler::PromptHandler;
use mcpkit_core::capability::{ClientCapabilities, ServerCapabilities};
use mcpkit_core::protocol::RequestId;
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
async fn test_prompt_service_basic() -> Result<(), Box<dyn std::error::Error>> {
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
            .user_text(format!("Hello, {}!", name))
            .build())
    });

    assert!(service.contains("greeting"));
    assert_eq!(service.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_prompt_render() -> Result<(), Box<dyn std::error::Error>> {
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
            .unwrap_or("No code provided");

        let language = args
            .and_then(|v| v.get("language").and_then(|l| l.as_str()).map(String::from))
            .unwrap_or_else(|| "unknown".to_string());

        Ok(PromptResultBuilder::new()
            .description("Code review prompt")
            .user_text(format!("Please review the following {} code:\n{}", language, code))
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
    let prompt_result = result?;
    assert!(!prompt_result.messages.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_prompt_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let service = PromptService::new();

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.render("nonexistent", None, &ctx).await;
    assert!(result.is_err());
    Ok(())
}

#[tokio::test]
async fn test_prompt_handler_trait() -> Result<(), Box<dyn std::error::Error>> {
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
            .user_text(format!("Please summarize: {}", text))
            .build())
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    // Use the PromptHandler trait
    let prompts = service.list_prompts(&ctx).await?;
    assert_eq!(prompts.len(), 1);

    let mut args = serde_json::Map::new();
    args.insert("text".to_string(), Value::String("Long text...".to_string()));

    let result = service.get_prompt("summarize", Some(args), &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_prompt_builder() -> Result<(), Box<dyn std::error::Error>> {
    let prompt = PromptBuilder::new("test_prompt")
        .description("A test prompt")
        .required_arg("input", "Required input")
        .optional_arg("format", "Output format")
        .build();

    assert_eq!(prompt.name, "test_prompt");
    assert_eq!(prompt.description.as_deref(), Some("A test prompt"));

    let args = prompt.arguments.ok_or("Arguments should be present")?;
    assert_eq!(args.len(), 2);

    let required_arg = args.iter().find(|a| a.name == "input").ok_or("Required arg 'input' not found")?;
    assert_eq!(required_arg.required, Some(true));

    let optional_arg = args.iter().find(|a| a.name == "format").ok_or("Optional arg 'format' not found")?;
    assert_eq!(optional_arg.required, Some(false));
    Ok(())
}

#[tokio::test]
async fn test_prompt_result_builder() -> Result<(), Box<dyn std::error::Error>> {
    let result = PromptResultBuilder::new()
        .description("Test result")
        .user_text("User message")
        .assistant_text("Assistant response")
        .build();

    assert_eq!(result.description.as_deref(), Some("Test result"));
    assert_eq!(result.messages.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_multiple_prompts() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = PromptService::new();

    for name in ["analyze", "translate", "explain", "debug"] {
        let prompt = PromptBuilder::new(name)
            .description(format!("{} something", name))
            .build();

        service.register(prompt, |_args, _ctx| async {
            Ok(PromptResultBuilder::new()
                .user_text("Test")
                .build())
        });
    }

    assert_eq!(service.len(), 4);

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let prompts = service.list_prompts(&ctx).await?;
    assert_eq!(prompts.len(), 4);
    Ok(())
}

#[tokio::test]
async fn test_prompt_with_no_args() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = PromptService::new();

    let prompt = PromptBuilder::new("help")
        .description("Get help")
        .build();

    service.register(prompt, |_args, _ctx| async {
        Ok(PromptResultBuilder::new()
            .user_text("How can I help you?")
            .build())
    });

    let (req_id, client_caps, server_caps, protocol_version, peer) = make_test_context();
    let ctx = Context::new(&req_id, None, &client_caps, &server_caps, protocol_version, &peer);

    let result = service.render("help", None, &ctx).await;
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_prompt_messages() -> Result<(), Box<dyn std::error::Error>> {
    // Test PromptMessage creation
    let user_msg = PromptMessage::user("User content".to_string());
    let assistant_msg = PromptMessage::assistant("Assistant content".to_string());

    // Both messages should be valid
    assert!(!user_msg.content.content.is_empty());
    assert!(!assistant_msg.content.content.is_empty());
    Ok(())
}
