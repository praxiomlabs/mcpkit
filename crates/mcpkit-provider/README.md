# mcpkit-provider

Multi-LLM provider abstraction for the mcpkit-forge orchestration layer.

## Overview

`mcpkit-provider` provides a unified interface for interacting with various LLM providers, enabling provider-agnostic code that works with OpenAI, Anthropic, Ollama, or any other compatible provider.

## Features

- **Unified `Provider` trait** - Single interface for all LLM providers
- **Streaming support** - Token-by-token streaming with backpressure and cancellation
- **Tool/function calling** - Unified interface across providers
- **Retry policies** - Configurable retry with exponential backoff
- **Rate limiting** - Built-in token bucket rate limiting
- **Cost tracking** - Token usage and cost estimation

## Supported Providers

| Provider | Feature Flag | Status |
|----------|--------------|--------|
| OpenAI | `openai` (default) | Full support |
| Anthropic | `anthropic` (default) | Full support |
| Ollama | `ollama` | Full support |

## Quick Start

```rust
use mcpkit_provider::{Provider, CompletionRequest, Message};
use mcpkit_provider::openai::OpenAiProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a provider
    let provider = OpenAiProvider::new("your-api-key")?;

    // Build a request
    let request = CompletionRequest::new()
        .model("gpt-4o")
        .message(Message::user("What is the capital of France?"))
        .max_tokens(100);

    // Get a completion
    let response = provider.complete(request).await?;
    println!("Response: {}", response.text().unwrap_or_default());

    Ok(())
}
```

## Streaming

```rust
use futures::StreamExt;
use mcpkit_provider::{Provider, CompletionRequest, Message, StreamEvent};

async fn stream_response(provider: &impl Provider) {
    let request = CompletionRequest::new()
        .message(Message::user("Tell me a story"))
        .stream();

    let mut stream = provider.complete_stream(request).await.unwrap();

    while let Some(event) = stream.next().await {
        match event {
            Ok(StreamEvent::ContentDelta { delta, .. }) => {
                // Handle streaming content
            }
            Ok(StreamEvent::Stop { usage, .. }) => {
                println!("Total tokens: {}", usage.total_tokens);
            }
            _ => {}
        }
    }
}
```

## Tool Calling

```rust
use mcpkit_provider::{Provider, CompletionRequest, Message, ToolDefinition};

async fn with_tools(provider: &impl Provider) {
    let tool = ToolDefinition::new("get_weather")
        .description("Get the current weather for a location")
        .input_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            },
            "required": ["location"]
        }));

    let request = CompletionRequest::new()
        .message(Message::user("What's the weather in Paris?"))
        .tools(vec![tool]);

    let response = provider.complete(request).await.unwrap();

    for tool_call in response.tool_calls() {
        println!("Tool: {}, Args: {}", tool_call.name, tool_call.arguments);
    }
}
```

## Vision / Image Support

Send images to vision-capable models:

```rust
use mcpkit_provider::{Provider, CompletionRequest, Message, ImageSource};

async fn analyze_image(provider: &impl Provider, base64_image_data: &str) {
    // From URL
    let image = ImageSource::from_url("https://example.com/image.jpg");

    // Or from base64-encoded data
    let image = ImageSource::from_base64(base64_image_data, "image/jpeg");

    let request = CompletionRequest::new()
        .model("gpt-4o")  // Use a vision-capable model
        .message(Message::user("What's in this image?").with_image(image));

    let response = provider.complete(request).await.unwrap();
    println!("{}", response.text().unwrap_or_default());
}
```

## JSON Mode / Structured Outputs

Request JSON-formatted responses:

```rust
use mcpkit_provider::{Provider, CompletionRequest, Message, ResponseFormat};

async fn get_json_response(provider: &impl Provider) {
    // Simple JSON mode
    let request = CompletionRequest::new()
        .message(Message::user("List 3 colors as a JSON array"))
        .response_format(ResponseFormat::JsonObject);

    // Or with a schema (structured outputs)
    let request = CompletionRequest::new()
        .message(Message::user("Extract the person's name and age"))
        .response_format(ResponseFormat::JsonSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "age": { "type": "integer" }
                },
                "required": ["name", "age"]
            }),
            strict: true,
        });

    let response = provider.complete(request).await.unwrap();
    let data: serde_json::Value = serde_json::from_str(&response.text().unwrap()).unwrap();
}
```

## Embeddings

Generate text embeddings for semantic search and similarity:

```rust
use mcpkit_provider::{Provider, EmbeddingRequest};

async fn create_embeddings(provider: &impl Provider) {
    // Single embedding
    let request = EmbeddingRequest::new("Hello world")
        .model("text-embedding-3-small");

    // Multiple embeddings (batch)
    let request = EmbeddingRequest::batch(vec![
        "Hello world".into(),
        "How are you?".into(),
    ]).model("text-embedding-3-small");

    let response = provider.embed(request).await.unwrap();

    for embedding in &response.embeddings {
        println!("Dimensions: {}", embedding.len());
    }

    // Calculate cosine similarity between first two embeddings
    let similarity = response.cosine_similarity(0, 1).unwrap();
    println!("Similarity: {:.4}", similarity);
}
```

## Provider-Agnostic Code

```rust
use mcpkit_provider::{Provider, CompletionRequest, Message};

// Works with any provider
async fn get_response(provider: &impl Provider) -> String {
    let request = CompletionRequest::new()
        .message(Message::user("Hello!"))
        .max_tokens(100);

    provider
        .complete(request)
        .await
        .unwrap()
        .text()
        .unwrap_or_default()
}
```

## Configuration

### OpenAI

```rust
use mcpkit_provider::openai::{OpenAiConfig, OpenAiProvider};

let config = OpenAiConfig::new("api-key")
    .base_url("https://api.openai.com/v1")  // Custom base URL
    .organization("org-123")                  // Organization ID
    .default_model("gpt-4o")                 // Default model
    .timeout(Duration::from_secs(120));      // Request timeout

let provider = OpenAiProvider::with_config(config)?;
```

### Anthropic

```rust
use mcpkit_provider::anthropic::{AnthropicConfig, AnthropicProvider};

let config = AnthropicConfig::new("api-key")
    .default_model("claude-sonnet-4-20250514")
    .default_max_tokens(4096);  // Anthropic requires max_tokens

let provider = AnthropicProvider::with_config(config)?;
```

### Ollama (Local)

```rust
use mcpkit_provider::ollama::{OllamaConfig, OllamaProvider};

let config = OllamaConfig::new()
    .base_url("http://localhost:11434")
    .default_model("llama3.2")
    .timeout(Duration::from_secs(300));  // Longer for local inference

let provider = OllamaProvider::with_config(config)?;
```

## Feature Flags

```toml
[dependencies]
mcpkit-provider = { version = "0.5", default-features = false, features = ["anthropic"] }
```

- `openai` (default) - OpenAI provider
- `anthropic` (default) - Anthropic provider
- `ollama` - Ollama provider for local models
- `all-providers` - All providers

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
