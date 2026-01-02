//! Compile-time validated prompt templates for mcpkit-forge.
//!
//! This crate provides type-safe prompt templates with compile-time validation
//! of variable interpolation. Unlike runtime template engines, errors are caught
//! at compile time, preventing runtime failures.
//!
//! # Features
//!
//! - **Compile-time validation** - Template variables are validated against struct fields
//! - **Type-safe interpolation** - All variables must implement `Display`
//! - **Template composition** - Compose templates from smaller pieces
//! - **Custom formatting** - Per-variable format specifiers
//!
//! # Example
//!
//! ```ignore
//! use mcpkit_template::{Template, PromptBuilder};
//!
//! #[derive(Template)]
//! #[template(source = "You are a {{role}}. {{instructions}}")]
//! struct SystemPrompt {
//!     role: String,
//!     instructions: String,
//! }
//!
//! let prompt = SystemPrompt {
//!     role: "helpful assistant".into(),
//!     instructions: "Be concise and accurate.".into(),
//! };
//!
//! assert_eq!(
//!     prompt.render(),
//!     "You are a helpful assistant. Be concise and accurate."
//! );
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

mod error;
mod prompt;

#[cfg(feature = "runtime")]
mod runtime;

pub use error::{TemplateError, TemplateResult};
pub use prompt::{Message, PromptBuilder, PromptTemplate, Role};

#[cfg(feature = "runtime")]
pub use runtime::{extract_variables, validate_template, RuntimeTemplate, TemplateBuilder};

#[cfg(feature = "derive")]
pub use mcpkit_template_derive::Template;

/// A compiled prompt template.
///
/// This trait is automatically implemented by the `#[derive(Template)]` macro.
/// It provides compile-time validated template rendering.
pub trait Template {
    /// Render the template with the current values.
    fn render(&self) -> String;

    /// Get the list of variable names in the template.
    fn variables() -> &'static [&'static str];

    /// Get the source template string.
    fn source() -> &'static str;
}

/// Extension trait for templates.
pub trait TemplateExt: Template {
    /// Render the template as a formatted message.
    fn as_message(&self, role: Role) -> Message {
        Message {
            role,
            content: self.render(),
        }
    }

    /// Render as a system message.
    fn as_system(&self) -> Message {
        self.as_message(Role::System)
    }

    /// Render as a user message.
    fn as_user(&self) -> Message {
        self.as_message(Role::User)
    }

    /// Render as an assistant message.
    fn as_assistant(&self) -> Message {
        self.as_message(Role::Assistant)
    }
}

impl<T: Template> TemplateExt for T {}

/// Prelude module for convenient imports.
pub mod prelude {
    pub use super::{
        Message, PromptBuilder, PromptTemplate, Role, Template, TemplateError, TemplateExt,
        TemplateResult,
    };

    #[cfg(feature = "runtime")]
    pub use super::{extract_variables, validate_template, RuntimeTemplate, TemplateBuilder};

    // The Template derive macro is already re-exported at the crate level (lib.rs line 54)
    // when the derive feature is enabled, so no need to re-export it here.
}

#[cfg(test)]
mod tests {
    use super::*;

    // Manual implementation for testing without derive macro
    struct TestTemplate {
        name: String,
        value: i32,
    }

    impl Template for TestTemplate {
        fn render(&self) -> String {
            format!("Hello, {}! Value: {}", self.name, self.value)
        }

        fn variables() -> &'static [&'static str] {
            &["name", "value"]
        }

        fn source() -> &'static str {
            "Hello, {{name}}! Value: {{value}}"
        }
    }

    #[test]
    fn test_manual_template() {
        let template = TestTemplate {
            name: "World".into(),
            value: 42,
        };

        assert_eq!(template.render(), "Hello, World! Value: 42");
        assert_eq!(TestTemplate::variables(), &["name", "value"]);
    }

    #[test]
    fn test_template_ext() {
        let template = TestTemplate {
            name: "Assistant".into(),
            value: 100,
        };

        let msg = template.as_system();
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content, "Hello, Assistant! Value: 100");
    }

    #[test]
    fn test_prompt_builder() {
        let builder = PromptBuilder::new()
            .system("You are a helpful assistant.")
            .user("Hello!")
            .assistant("Hi there!");

        let messages = builder.build();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
    }
}
