//! Integration tests for OpenAI provider using wiremock.
//!
//! These tests verify that the OpenAI provider correctly handles
//! API responses including completions, streaming, tool calls, and errors.

use mcpkit_provider::openai::{OpenAiConfig, OpenAiProvider};
use mcpkit_provider::{
    CompletionRequest, ContentDelta, EmbeddingRequest, Message, Provider, ResponseFormat,
    StreamEvent, ToolDefinition,
};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to create a provider pointing at the mock server.
fn create_provider(mock_server: &MockServer) -> OpenAiProvider {
    let config = OpenAiConfig::new("test-api-key")
        .base_url(mock_server.uri())
        .default_model("gpt-4o");

    OpenAiProvider::with_config(config).expect("Failed to create provider")
}

#[tokio::test]
async fn test_basic_completion() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "chatcmpl-123",
        "model": "gpt-4o",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 8,
            "total_tokens": 18
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new()
        .model("gpt-4o")
        .message(Message::user("Hello!"))
        .max_tokens(100);

    let response = provider.complete(request).await.expect("Completion failed");

    assert_eq!(response.id, "chatcmpl-123");
    assert_eq!(response.model, "gpt-4o");
    assert_eq!(
        response.text().unwrap(),
        "Hello! How can I help you today?"
    );
    assert_eq!(response.usage.total_tokens, 18);
}

#[tokio::test]
async fn test_system_and_user_messages() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "chatcmpl-456",
        "model": "gpt-4o",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "I'm a pirate, arr!"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 20,
            "completion_tokens": 6,
            "total_tokens": 26
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new()
        .message(Message::system("You are a helpful pirate."))
        .message(Message::user("Who are you?"));

    let response = provider.complete(request).await.expect("Completion failed");

    assert_eq!(response.text().unwrap(), "I'm a pirate, arr!");
}

#[tokio::test]
async fn test_tool_calling() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "chatcmpl-789",
        "model": "gpt-4o",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "get_weather",
                        "arguments": "{\"location\": \"Paris\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": {
            "prompt_tokens": 25,
            "completion_tokens": 15,
            "total_tokens": 40
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let tool = ToolDefinition::new("get_weather")
        .description("Get the current weather")
        .input_schema(json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            },
            "required": ["location"]
        }));

    let request = CompletionRequest::new()
        .message(Message::user("What's the weather in Paris?"))
        .tools(vec![tool]);

    let response = provider.complete(request).await.expect("Completion failed");

    let tool_calls = response.tool_calls();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "get_weather");
    assert_eq!(tool_calls[0].arguments["location"], "Paris");
}

#[tokio::test]
async fn test_embeddings() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "data": [
            {
                "embedding": [0.1, 0.2, 0.3, 0.4, 0.5],
                "index": 0
            },
            {
                "embedding": [0.6, 0.7, 0.8, 0.9, 1.0],
                "index": 1
            }
        ],
        "model": "text-embedding-3-small",
        "usage": {
            "prompt_tokens": 5,
            "total_tokens": 5
        }
    });

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = EmbeddingRequest::batch(vec![
        "Hello world".to_string(),
        "How are you?".to_string(),
    ])
    .model("text-embedding-3-small");

    let response = provider.embed(request).await.expect("Embedding failed");

    assert_eq!(response.embeddings.len(), 2);
    assert_eq!(response.embeddings[0].embedding.len(), 5);
    assert_eq!(response.embeddings[1].embedding.len(), 5);
    assert_eq!(response.usage.total_tokens, 5);
}

#[tokio::test]
async fn test_embedding_vectors() {
    let mock_server = MockServer::start().await;

    // Create two vectors with known values
    let response_body = json!({
        "data": [
            { "embedding": [1.0, 0.0, 0.0], "index": 0 },
            { "embedding": [0.0, 1.0, 0.0], "index": 1 },
            { "embedding": [1.0, 0.0, 0.0], "index": 2 }
        ],
        "model": "text-embedding-3-small",
        "usage": { "prompt_tokens": 6, "total_tokens": 6 }
    });

    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = EmbeddingRequest::batch(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
    ]);
    let response = provider.embed(request).await.expect("Embedding failed");

    // Verify we got the expected embeddings
    assert_eq!(response.embeddings.len(), 3);
    assert_eq!(response.embeddings[0].embedding, vec![1.0, 0.0, 0.0]);
    assert_eq!(response.embeddings[1].embedding, vec![0.0, 1.0, 0.0]);
    assert_eq!(response.embeddings[2].embedding, vec![1.0, 0.0, 0.0]);

    // Helper to compute cosine similarity
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    }

    // Orthogonal vectors should have 0 similarity
    let sim_01 = cosine_similarity(
        &response.embeddings[0].embedding,
        &response.embeddings[1].embedding,
    );
    assert!((sim_01 - 0.0).abs() < 0.001);

    // Identical vectors should have 1.0 similarity
    let sim_02 = cosine_similarity(
        &response.embeddings[0].embedding,
        &response.embeddings[2].embedding,
    );
    assert!((sim_02 - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn test_list_models() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "data": [
            { "id": "gpt-4o" },
            { "id": "gpt-3.5-turbo" },
            { "id": "dall-e-3" },
            { "id": "whisper-1" }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let models = provider.list_models().await.expect("List models failed");

    // Should filter to only gpt models
    assert_eq!(models.len(), 2);
    assert!(models.iter().any(|m| m.id == "gpt-4o"));
    assert!(models.iter().any(|m| m.id == "gpt-3.5-turbo"));
}

#[tokio::test]
async fn test_auth_error() {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "error": {
            "message": "Invalid API key",
            "type": "invalid_request_error",
            "code": "invalid_api_key"
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(&error_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new().message(Message::user("Hello"));

    let result = provider.complete(request).await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_str = format!("{error}");
    assert!(error_str.contains("authentication") || error_str.contains("Invalid API key"));
}

#[tokio::test]
async fn test_rate_limit_error() {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "error": {
            "message": "Rate limit exceeded",
            "type": "rate_limit_error",
            "code": "rate_limit_exceeded"
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_json(&error_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new().message(Message::user("Hello"));

    let result = provider.complete(request).await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_str = format!("{error}");
    assert!(error_str.contains("rate") || error_str.contains("limit"));
}

#[tokio::test]
async fn test_context_length_error() {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "error": {
            "message": "This model's maximum context_length is 8192 tokens",
            "type": "invalid_request_error",
            "code": "context_length_exceeded"
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new().message(Message::user("Hello"));

    let result = provider.complete(request).await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    let error_str = format!("{error}");
    assert!(error_str.contains("context"));
}

#[tokio::test]
async fn test_provider_info() {
    let mock_server = MockServer::start().await;
    let provider = create_provider(&mock_server);

    let info = provider.info();

    assert_eq!(info.name, "openai");
    assert!(info.capabilities.streaming);
    assert!(info.capabilities.tools);
    assert!(info.capabilities.vision);
    assert!(info.capabilities.json_mode);
    assert!(info.capabilities.embeddings);
}

#[tokio::test]
async fn test_get_model_info() {
    let mock_server = MockServer::start().await;
    let provider = create_provider(&mock_server);

    let model = provider
        .get_model("gpt-4o")
        .await
        .expect("Get model failed");

    assert_eq!(model.id, "gpt-4o");
    assert_eq!(model.context_length, Some(128_000));
    assert!(model.supports_tools);
    assert!(model.supports_vision);
}

#[tokio::test]
async fn test_json_mode() {
    let mock_server = MockServer::start().await;

    let response_body = json!({
        "id": "chatcmpl-json",
        "model": "gpt-4o",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "{\"colors\": [\"red\", \"green\", \"blue\"]}"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 15,
            "completion_tokens": 10,
            "total_tokens": 25
        }
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new()
        .message(Message::user("List 3 colors as JSON"))
        .response_format(ResponseFormat::JsonObject);

    let response = provider.complete(request).await.expect("Completion failed");

    let content = response.text().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("Invalid JSON");
    assert!(parsed["colors"].is_array());
}

#[tokio::test]
async fn test_streaming_completion() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    // SSE response format
    let sse_body = r#"data: {"id":"chatcmpl-stream","choices":[{"delta":{"role":"assistant","content":"Hello"},"index":0}]}

data: {"id":"chatcmpl-stream","choices":[{"delta":{"content":" there!"},"index":0}]}

data: {"id":"chatcmpl-stream","choices":[{"delta":{},"finish_reason":"stop","index":0}]}

data: [DONE]

"#;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new()
        .message(Message::user("Hi"))
        .stream();

    let mut stream = provider
        .complete_stream(request)
        .await
        .expect("Stream failed");

    let mut collected_text = String::new();
    let mut got_start = false;
    let mut got_stop = false;

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::Start { id, .. }) => {
                got_start = true;
                assert_eq!(id, "chatcmpl-stream");
            }
            Ok(StreamEvent::ContentDelta { delta, .. }) => {
                if let ContentDelta::Text { text } = delta {
                    collected_text.push_str(&text);
                }
            }
            Ok(StreamEvent::Stop { .. }) => {
                got_stop = true;
            }
            Err(e) => panic!("Stream error: {e}"),
            _ => {}
        }
    }

    assert!(got_start);
    assert!(got_stop);
    assert_eq!(collected_text, "Hello there!");
}

#[tokio::test]
async fn test_streaming_tool_calls() {
    use futures::StreamExt;

    let mock_server = MockServer::start().await;

    // SSE response with streaming tool call
    let sse_body = r#"data: {"id":"chatcmpl-tool","choices":[{"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"get_weather","arguments":""}}]},"index":0}]}

data: {"id":"chatcmpl-tool","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"loc"}}]},"index":0}]}

data: {"id":"chatcmpl-tool","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ation\":"}}]},"index":0}]}

data: {"id":"chatcmpl-tool","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Paris\"}"}}]},"index":0}]}

data: {"id":"chatcmpl-tool","choices":[{"delta":{},"finish_reason":"tool_calls","index":0}]}

data: [DONE]

"#;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let provider = create_provider(&mock_server);

    let request = CompletionRequest::new()
        .message(Message::user("Weather in Paris?"))
        .tools(vec![ToolDefinition::new("get_weather")])
        .stream();

    let mut stream = provider
        .complete_stream(request)
        .await
        .expect("Stream failed");

    let mut got_tool_start = false;
    let mut collected_args = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::ToolUseStart { id, name, .. }) => {
                got_tool_start = true;
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
            }
            Ok(StreamEvent::ContentDelta { delta, .. }) => {
                if let ContentDelta::ToolInput { partial_json } = delta {
                    collected_args.push_str(&partial_json);
                }
            }
            Err(e) => panic!("Stream error: {e}"),
            _ => {}
        }
    }

    assert!(got_tool_start);
    assert_eq!(collected_args, "{\"location\":\"Paris\"}");
}
