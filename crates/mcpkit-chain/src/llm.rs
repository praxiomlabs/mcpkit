//! LLM-specific runnables for provider integration.

use async_trait::async_trait;
use std::sync::Arc;

use mcpkit_provider::{CompletionRequest, Message, Provider};

use crate::error::{ChainError, ChainResult};
use crate::runnable::{ChainValue, Runnable};

/// A runnable that invokes an LLM provider.
///
/// Takes a string or message array input and returns the LLM response.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_chain::{LlmRunnable, ChainValue};
/// use mcpkit_provider::openai::OpenAiProvider;
///
/// let provider = OpenAiProvider::new(api_key)?;
/// let llm = LlmRunnable::new(provider).model("gpt-4o");
///
/// let result = llm.invoke(ChainValue::from("Hello!")).await?;
/// println!("{}", result.as_str().unwrap());
/// ```
pub struct LlmRunnable<P: Provider> {
    provider: Arc<P>,
    model: Option<String>,
    system: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    name: String,
}

impl<P: Provider> LlmRunnable<P> {
    /// Create a new LLM runnable with the given provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            model: None,
            system: None,
            temperature: None,
            max_tokens: None,
            name: "LLM".to_string(),
        }
    }

    /// Create from an Arc'd provider.
    pub fn from_arc(provider: Arc<P>) -> Self {
        Self {
            provider,
            model: None,
            system: None,
            temperature: None,
            max_tokens: None,
            name: "LLM".to_string(),
        }
    }

    /// Set the model to use.
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set a system prompt.
    #[must_use]
    pub fn system(mut self, system: impl Into<String>) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Set the temperature.
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the max tokens.
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Build a completion request from input.
    fn build_request(&self, input: &ChainValue) -> ChainResult<CompletionRequest> {
        let mut request = CompletionRequest::new();

        // Set model if specified
        if let Some(model) = &self.model {
            request = request.model(model.clone());
        }

        // Add system message if specified
        if let Some(system) = &self.system {
            request = request.message(Message::system(system.clone()));
        }

        // Set parameters
        if let Some(temp) = self.temperature {
            request = request.temperature(temp);
        }
        if let Some(max) = self.max_tokens {
            request = request.max_tokens(max);
        }

        // Handle different input types
        match input {
            ChainValue::String(s) => {
                request = request.message(Message::user(s.clone()));
            }
            ChainValue::Array(arr) => {
                // Array of messages
                for item in arr {
                    if let Some(s) = item.as_str() {
                        request = request.message(Message::user(s.to_string()));
                    }
                }
            }
            ChainValue::Object(obj) => {
                // Object with prompt field
                if let Some(prompt) = obj.get("prompt").and_then(|v| v.as_str()) {
                    request = request.message(Message::user(prompt.to_string()));
                }
                // Override system if provided
                if let Some(sys) = obj.get("system").and_then(|v| v.as_str()) {
                    request = request.message(Message::system(sys.to_string()));
                }
            }
            _ => {
                return Err(ChainError::type_error(
                    "string, array, or object",
                    input.type_name(),
                ));
            }
        }

        Ok(request)
    }
}

#[async_trait]
impl<P: Provider + 'static> Runnable for LlmRunnable<P> {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        let request = self.build_request(&input)?;
        let response = self.provider.complete(request).await?;

        match response.text() {
            Some(text) => Ok(ChainValue::String(text)),
            None => Ok(ChainValue::Null),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that formats a prompt template with input values.
///
/// Takes an object input and substitutes values into a template string.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_chain::{PromptRunnable, ChainValue};
///
/// let prompt = PromptRunnable::new("Hello, {name}! You are {age} years old.");
///
/// let input = ChainValue::Object([
///     ("name".to_string(), ChainValue::from("Alice")),
///     ("age".to_string(), ChainValue::from(30)),
/// ].into_iter().collect());
///
/// let result = prompt.invoke(input).await?;
/// assert_eq!(result.as_str(), Some("Hello, Alice! You are 30 years old."));
/// ```
#[derive(Debug, Clone)]
pub struct PromptRunnable {
    template: String,
    name: String,
}

impl PromptRunnable {
    /// Create a new prompt runnable with the given template.
    ///
    /// Variables are specified using `{variable_name}` syntax.
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            name: "Prompt".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Runnable for PromptRunnable {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        let mut result = self.template.clone();

        match &input {
            ChainValue::Object(obj) => {
                for (key, value) in obj {
                    let pattern = format!("{{{key}}}");
                    result = result.replace(&pattern, &value.to_string_value());
                }
            }
            ChainValue::String(s) => {
                // Single string input replaces {input}
                result = result.replace("{input}", s);
            }
            _ => {
                // Convert to string and replace {input}
                result = result.replace("{input}", &input.to_string_value());
            }
        }

        Ok(ChainValue::String(result))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that parses JSON from a string input.
#[derive(Debug, Clone, Default)]
pub struct JsonParseRunnable {
    name: String,
}

impl JsonParseRunnable {
    /// Create a new JSON parse runnable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "JsonParse".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Runnable for JsonParseRunnable {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        let text = input.to_string_value();

        // Try to find JSON in the text (handles markdown code blocks)
        let json_str = extract_json(&text).unwrap_or(&text);

        let value: serde_json::Value = serde_json::from_str(json_str)?;
        Ok(ChainValue::from(value))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Extract JSON from text that might contain markdown code blocks.
fn extract_json(text: &str) -> Option<&str> {
    // Try to find ```json ... ``` blocks
    if let Some(start) = text.find("```json") {
        let start = start + 7;
        if let Some(end) = text[start..].find("```") {
            return Some(text[start..start + end].trim());
        }
    }

    // Try to find ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let start = start + 3;
        // Skip optional language identifier on same line
        let start = text[start..]
            .find('\n')
            .map(|i| start + i + 1)
            .unwrap_or(start);
        if let Some(end) = text[start..].find("```") {
            return Some(text[start..start + end].trim());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prompt_object_input() {
        let prompt = PromptRunnable::new("Hello, {name}! You are {age} years old.");
        let input = ChainValue::Object(
            [
                ("name".to_string(), ChainValue::from("Alice")),
                ("age".to_string(), ChainValue::from(30)),
            ]
            .into_iter()
            .collect(),
        );

        let result = prompt.invoke(input).await.unwrap();
        assert_eq!(
            result.as_str(),
            Some("Hello, Alice! You are 30 years old.")
        );
    }

    #[tokio::test]
    async fn test_prompt_string_input() {
        let prompt = PromptRunnable::new("Process this: {input}");
        let result = prompt
            .invoke(ChainValue::from("my data"))
            .await
            .unwrap();
        assert_eq!(result.as_str(), Some("Process this: my data"));
    }

    #[tokio::test]
    async fn test_json_parse() {
        let parser = JsonParseRunnable::new();
        let result = parser
            .invoke(ChainValue::from(r#"{"name": "Alice", "age": 30}"#))
            .await
            .unwrap();

        assert_eq!(result.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(result.get("age").and_then(|v| v.as_int()), Some(30));
    }

    #[tokio::test]
    async fn test_json_parse_with_markdown() {
        let parser = JsonParseRunnable::new();
        let input = r#"
Here is the result:

```json
{"result": "success"}
```

That's the output.
"#;
        let result = parser.invoke(ChainValue::from(input)).await.unwrap();
        assert_eq!(
            result.get("result").and_then(|v| v.as_str()),
            Some("success")
        );
    }

    #[test]
    fn test_extract_json() {
        let text = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(text), Some("{\"key\": \"value\"}"));

        let text = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(text), Some("{\"key\": \"value\"}"));

        let text = "{\"key\": \"value\"}";
        assert_eq!(extract_json(text), None);
    }
}
