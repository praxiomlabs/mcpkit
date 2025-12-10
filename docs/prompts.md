# Working with Prompts

Prompts are reusable message templates that AI assistants can use to structure their interactions. They're useful for standardizing common workflows.

## Basic Prompt Definition

Use the `#[prompt]` attribute to define a prompt handler:

```rust
use mcp::prelude::*;

struct PromptServer;

#[mcp_server(name = "prompts", version = "1.0.0")]
impl PromptServer {
    #[prompt(description = "Generate a greeting message")]
    async fn greeting(&self, name: String) -> GetPromptResult {
        GetPromptResult {
            description: Some("A friendly greeting".to_string()),
            messages: vec![
                PromptMessage::user(format!("Please greet {} warmly.", name))
            ],
        }
    }
}
```

## Prompt Attributes

```rust
#[prompt(
    description = "What this prompt does",  // Required
    name = "custom_name",                   // Optional: override method name
)]
```

## Prompt Arguments

### Required Arguments

```rust
#[prompt(description = "Code review prompt")]
async fn code_review(&self, code: String, language: String) -> GetPromptResult {
    GetPromptResult {
        description: Some("Review code for issues".to_string()),
        messages: vec![
            PromptMessage::user(format!(
                "Please review this {} code for bugs and improvements:\n\n```{}\n{}\n```",
                language, language, code
            ))
        ],
    }
}
```

### Optional Arguments

```rust
#[prompt(description = "Code review with optional focus areas")]
async fn code_review(
    &self,
    code: String,
    language: Option<String>,
    focus: Option<String>,
) -> GetPromptResult {
    let lang = language.unwrap_or_else(|| "unknown".to_string());
    let focus_text = focus
        .map(|f| format!("\n\nFocus especially on: {}", f))
        .unwrap_or_default();

    GetPromptResult {
        description: Some("Code review prompt".to_string()),
        messages: vec![
            PromptMessage::user(format!(
                "Review this {} code:\n```\n{}\n```{}",
                lang, code, focus_text
            ))
        ],
    }
}
```

## Message Types

### User Messages

```rust
PromptMessage::user("This is from the user")
```

### Assistant Messages

```rust
PromptMessage::assistant("This is the assistant's response")
```

### System Messages (if supported)

```rust
PromptMessage::system("System-level instructions")
```

## Multi-Turn Prompts

Create conversation-style prompts with multiple messages:

```rust
#[prompt(description = "Interactive debugging session")]
async fn debug_session(&self, error_message: String) -> GetPromptResult {
    GetPromptResult {
        description: Some("Step-by-step debugging".to_string()),
        messages: vec![
            PromptMessage::system(
                "You are a helpful debugging assistant. Guide the user through \
                 fixing their error step by step."
            ),
            PromptMessage::user(format!(
                "I'm getting this error:\n\n```\n{}\n```\n\nCan you help me fix it?",
                error_message
            )),
            PromptMessage::assistant(
                "I'd be happy to help! Let me analyze this error. \
                 First, could you tell me:\n\n\
                 1. What were you trying to do when this error occurred?\n\
                 2. What changes did you make recently?"
            ),
        ],
    }
}
```

## Including Resources in Prompts

Prompts can reference resources:

```rust
#[prompt(description = "Analyze a file")]
async fn analyze_file(&self, file_path: String) -> GetPromptResult {
    // Read the file content
    let content = std::fs::read_to_string(&file_path)
        .unwrap_or_else(|_| "Could not read file".to_string());

    GetPromptResult {
        description: Some(format!("Analysis of {}", file_path)),
        messages: vec![
            PromptMessage::user(format!(
                "Please analyze this file ({}):\n\n```\n{}\n```",
                file_path, content
            ))
        ],
    }
}
```

## Dynamic Prompts

Build prompts dynamically based on context:

```rust
#[prompt(description = "Generate tests for code")]
async fn generate_tests(
    &self,
    code: String,
    framework: Option<String>,
    coverage: Option<String>,
) -> GetPromptResult {
    let framework = framework.unwrap_or_else(|| "appropriate".to_string());
    let coverage = coverage.unwrap_or_else(|| "comprehensive".to_string());

    let mut instructions = vec![
        format!("Generate {} tests using {} testing framework.", coverage, framework),
        "Include edge cases and error scenarios.".to_string(),
        "Add comments explaining what each test verifies.".to_string(),
    ];

    GetPromptResult {
        description: Some("Test generation prompt".to_string()),
        messages: vec![
            PromptMessage::user(format!(
                "{}\n\nCode to test:\n```\n{}\n```",
                instructions.join("\n"),
                code
            ))
        ],
    }
}
```

## Complete Example

```rust
use mcp::prelude::*;

struct WritingAssistant;

#[mcp_server(name = "writing-assistant", version = "1.0.0")]
impl WritingAssistant {
    /// Help improve writing quality
    #[prompt(description = "Improve writing clarity and style")]
    async fn improve_writing(
        &self,
        text: String,
        style: Option<String>,
        audience: Option<String>,
    ) -> GetPromptResult {
        let style = style.unwrap_or_else(|| "professional".to_string());
        let audience = audience.unwrap_or_else(|| "general".to_string());

        GetPromptResult {
            description: Some("Writing improvement suggestions".to_string()),
            messages: vec![
                PromptMessage::system(format!(
                    "You are an expert editor. Improve text for a {} audience \
                     using a {} style. Preserve the original meaning while \
                     enhancing clarity, flow, and impact.",
                    audience, style
                )),
                PromptMessage::user(format!(
                    "Please improve this text:\n\n{}\n\n\
                     Provide the improved version followed by a brief explanation \
                     of the key changes you made.",
                    text
                )),
            ],
        }
    }

    /// Summarize long content
    #[prompt(description = "Create a summary of content")]
    async fn summarize(
        &self,
        content: String,
        length: Option<String>,
        format: Option<String>,
    ) -> GetPromptResult {
        let length = length.unwrap_or_else(|| "medium".to_string());
        let format = format.unwrap_or_else(|| "paragraph".to_string());

        let length_instruction = match length.as_str() {
            "short" => "Create a 1-2 sentence summary.",
            "medium" => "Create a 1 paragraph summary.",
            "long" => "Create a detailed multi-paragraph summary.",
            _ => "Create an appropriately sized summary.",
        };

        let format_instruction = match format.as_str() {
            "bullets" => "Use bullet points.",
            "paragraph" => "Use prose paragraphs.",
            "outline" => "Use an outline format with headings.",
            _ => "",
        };

        GetPromptResult {
            description: Some("Content summary".to_string()),
            messages: vec![
                PromptMessage::user(format!(
                    "{} {}\n\nContent to summarize:\n\n{}",
                    length_instruction, format_instruction, content
                )),
            ],
        }
    }

    /// Translate content to another language
    #[prompt(description = "Translate text to another language")]
    async fn translate(
        &self,
        text: String,
        target_language: String,
        preserve_tone: Option<bool>,
    ) -> GetPromptResult {
        let tone_instruction = if preserve_tone.unwrap_or(true) {
            "Preserve the original tone and style."
        } else {
            "Adapt the tone for the target language and culture."
        };

        GetPromptResult {
            description: Some(format!("Translation to {}", target_language)),
            messages: vec![
                PromptMessage::user(format!(
                    "Translate the following text to {}. {}\n\n\
                     Text:\n{}\n\n\
                     Provide the translation followed by any cultural notes \
                     if relevant.",
                    target_language, tone_instruction, text
                )),
            ],
        }
    }
}
```

## Best Practices

1. **Clear Descriptions**: Help AI understand when to use each prompt
2. **Sensible Defaults**: Use `Option` with good defaults for flexibility
3. **Structured Output**: Guide the format of expected responses
4. **Context Setting**: Use system messages to establish context
5. **Modular Design**: Create focused prompts that do one thing well
6. **Test Prompts**: Verify prompts produce expected behavior
