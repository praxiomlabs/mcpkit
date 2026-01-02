# mcpkit-template

Compile-time validated prompt templates for the mcpkit-forge orchestration layer.

## Overview

`mcpkit-template` provides type-safe prompt templates with compile-time validation. Unlike runtime template engines, errors are caught at compile time, preventing runtime failures.

## Features

- **Compile-time validation** - Template variables are validated against struct fields
- **Type-safe interpolation** - All variables must implement `Display`
- **Template composition** - Compose templates from smaller pieces
- **Custom formatting** - Per-variable format specifiers
- **Few-shot support** - Built-in support for few-shot prompt patterns

## Quick Start

### Compile-Time Templates (Derive Macro)

```rust
use mcpkit_template::{Template, PromptBuilder};

#[derive(Template)]
#[template(source = "You are a {{role}}. {{instructions}}")]
struct SystemPrompt {
    role: String,
    instructions: String,
}

fn main() {
    let prompt = SystemPrompt {
        role: "helpful assistant".into(),
        instructions: "Be concise and accurate.".into(),
    };

    // Compile-time validated rendering
    println!("{}", prompt.render());
}
```

### Templates from Files

Load templates from external files (relative to `Cargo.toml`):

```rust
use mcpkit_template::Template;

// File: templates/system_prompt.txt
// Contents: "You are a {{role}}. {{instructions}}"

#[derive(Template)]
#[template(path = "templates/system_prompt.txt")]
struct SystemPrompt {
    role: String,
    instructions: String,
}
```

### Inline Template Macro

For quick, one-off templates without defining a struct:

```rust
use mcpkit_template_derive::template;

fn main() {
    let name = "World";
    let greeting = template!("Hello, {{name}}!", name = name);
    println!("{}", greeting); // "Hello, World!"

    // Expressions work too
    let result = template!("Sum: {{sum}}", sum = 1 + 2 + 3);
    println!("{}", result); // "Sum: 6"
}
```

### Runtime Templates

For user-provided or dynamic templates:

```rust
use mcpkit_template::RuntimeTemplate;
use std::collections::HashMap;

fn main() {
    let template = RuntimeTemplate::new("Hello, {{name}}! You are {{role}}.").unwrap();

    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "Alice".to_string());
    vars.insert("role".to_string(), "an engineer".to_string());

    let result = template.render(&vars).unwrap();
    println!("{}", result);
}
```

## Prompt Building

Build conversation prompts with the fluent API:

```rust
use mcpkit_template::{PromptBuilder, Role};

let messages = PromptBuilder::new()
    .system("You are a helpful assistant.")
    .user("What is 2 + 2?")
    .assistant("2 + 2 = 4")
    .user("What about 3 + 3?")
    .build();
```

## Few-Shot Templates

Create reusable prompt patterns with examples:

```rust
use mcpkit_template::prompt::PromptTemplate;

let translator = PromptTemplate::new()
    .system("You are a translator. Translate English to French.")
    .example("Hello", "Bonjour")
    .example("Goodbye", "Au revoir")
    .example("Thank you", "Merci");

// Apply template to new input
let messages = translator.apply("Good morning").build();
```

## Template Extension Trait

Convert templates directly to messages:

```rust
use mcpkit_template::{Template, TemplateExt};

#[derive(Template)]
#[template(source = "You are a {{role}} assistant.")]
struct SystemMessage {
    role: String,
}

let prompt = SystemMessage { role: "coding".into() };

// Convert to system message
let message = prompt.as_system();
```

## Custom Formatting

Use the `#[var]` attribute for custom formatting:

```rust
#[derive(Template)]
#[template(source = "Price: ${{price}}")]
struct PriceDisplay {
    #[var(format = ".2")]
    price: f64,
}
```

## Feature Flags

```toml
[dependencies]
mcpkit-template = { version = "0.5", features = ["derive", "runtime"] }
```

- `runtime` (default) - Runtime template parsing and rendering
- `derive` - Derive macro for compile-time templates

## Compile-Time vs Runtime

| Feature | Compile-Time (Derive) | Compile-Time (Macro) | Runtime |
|---------|----------------------|----------------------|---------|
| Error detection | At compile | At compile | At runtime |
| Performance | Fastest | Fast | Slightly slower |
| Dynamic templates | No | No | Yes |
| Type safety | Full | Full | String-only |
| File-based | Yes (`path`) | No | No |
| Reusable | Yes (struct) | No (inline) | Yes |

**Use compile-time templates when possible.** Reserve runtime templates for user-provided or configuration-driven templates. Use the `template!` macro for quick, one-off interpolations.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
