//! Integration tests for the Template derive macro.
//!
//! These tests verify that the derive macro correctly generates Template
//! implementations and validates templates at compile time.

use mcpkit_template::{template, Role, Template, TemplateExt};

/// Basic template with two variables.
#[derive(Template)]
#[template(source = "Hello, {{name}}! You are {{age}} years old.")]
struct Greeting {
    name: String,
    age: u32,
}

#[test]
fn test_basic_template() {
    let greeting = Greeting {
        name: "Alice".into(),
        age: 30,
    };

    assert_eq!(
        greeting.render(),
        "Hello, Alice! You are 30 years old."
    );
}

#[test]
fn test_template_variables() {
    let vars = Greeting::variables();
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"name"));
    assert!(vars.contains(&"age"));
}

#[test]
fn test_template_source() {
    assert_eq!(
        Greeting::source(),
        "Hello, {{name}}! You are {{age}} years old."
    );
}

/// Template with a single variable.
#[derive(Template)]
#[template(source = "Welcome, {{user}}!")]
struct Welcome {
    user: String,
}

#[test]
fn test_single_variable() {
    let welcome = Welcome {
        user: "Bob".into(),
    };
    assert_eq!(welcome.render(), "Welcome, Bob!");
}

/// Template with no variables (static text).
#[derive(Template)]
#[template(source = "This is a static message with no variables.")]
struct StaticMessage {}

#[test]
fn test_no_variables() {
    let msg = StaticMessage {};
    assert_eq!(
        msg.render(),
        "This is a static message with no variables."
    );
    assert!(StaticMessage::variables().is_empty());
}

/// Template with multiple occurrences of the same variable.
#[derive(Template)]
#[template(source = "{{name}} says: Hello, {{name}}!")]
struct EchoName {
    name: String,
}

#[test]
fn test_repeated_variable() {
    let echo = EchoName {
        name: "Charlie".into(),
    };
    assert_eq!(echo.render(), "Charlie says: Hello, Charlie!");
}

/// Template with special characters in the text.
#[derive(Template)]
#[template(source = "User: {{username}} | Email: {{email}}")]
struct UserInfo {
    username: String,
    email: String,
}

#[test]
fn test_special_characters() {
    let info = UserInfo {
        username: "john_doe".into(),
        email: "john@example.com".into(),
    };
    assert_eq!(info.render(), "User: john_doe | Email: john@example.com");
}

/// Template with numeric types.
#[derive(Template)]
#[template(source = "x={{x}}, y={{y}}, z={{z}}")]
struct Coordinates {
    x: f64,
    y: f64,
    z: f64,
}

#[test]
fn test_numeric_types() {
    let coords = Coordinates {
        x: 1.5,
        y: 2.0,
        z: -3.25,
    };
    assert_eq!(coords.render(), "x=1.5, y=2, z=-3.25");
}

/// Template with boolean type.
#[derive(Template)]
#[template(source = "Active: {{is_active}}, Verified: {{is_verified}}")]
struct Status {
    is_active: bool,
    is_verified: bool,
}

#[test]
fn test_boolean_type() {
    let status = Status {
        is_active: true,
        is_verified: false,
    };
    assert_eq!(status.render(), "Active: true, Verified: false");
}

/// Template using TemplateExt methods.
#[derive(Template)]
#[template(source = "You are a {{role}}. {{task}}")]
struct SystemPrompt {
    role: String,
    task: String,
}

#[test]
fn test_template_ext_as_system() {
    let prompt = SystemPrompt {
        role: "helpful assistant".into(),
        task: "Help the user with coding.".into(),
    };

    let msg = prompt.as_system();
    assert_eq!(msg.role, Role::System);
    assert_eq!(
        msg.content,
        "You are a helpful assistant. Help the user with coding."
    );
}

#[test]
fn test_template_ext_as_user() {
    let prompt = SystemPrompt {
        role: "questioner".into(),
        task: "Ask about Rust.".into(),
    };

    let msg = prompt.as_user();
    assert_eq!(msg.role, Role::User);
}

#[test]
fn test_template_ext_as_assistant() {
    let prompt = SystemPrompt {
        role: "responder".into(),
        task: "Provide an answer.".into(),
    };

    let msg = prompt.as_assistant();
    assert_eq!(msg.role, Role::Assistant);
}

/// Template with newlines and multi-line content.
#[derive(Template)]
#[template(source = "Line 1: {{line1}}\nLine 2: {{line2}}")]
struct MultiLine {
    line1: String,
    line2: String,
}

#[test]
fn test_multiline_template() {
    let ml = MultiLine {
        line1: "First".into(),
        line2: "Second".into(),
    };
    assert_eq!(ml.render(), "Line 1: First\nLine 2: Second");
}

/// Template with unicode characters.
#[derive(Template)]
#[template(source = "🎉 {{message}} 🎉")]
struct UnicodeTemplate {
    message: String,
}

#[test]
fn test_unicode_template() {
    let t = UnicodeTemplate {
        message: "Hello, 世界!".into(),
    };
    assert_eq!(t.render(), "🎉 Hello, 世界! 🎉");
}

// Generic template tests are skipped because the compile-time validation
// const fn doesn't work well with type parameters. The derive macro would
// need to be enhanced to handle generics properly.

/// Test template with custom Display implementation.
struct CustomType {
    inner: String,
}

impl std::fmt::Display for CustomType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.inner)
    }
}

#[derive(Template)]
#[template(source = "Custom: {{custom}}")]
struct CustomDisplay {
    custom: CustomType,
}

#[test]
fn test_custom_display() {
    let t = CustomDisplay {
        custom: CustomType {
            inner: "wrapped".into(),
        },
    };
    assert_eq!(t.render(), "Custom: [wrapped]");
}

/// Test empty string interpolation.
#[derive(Template)]
#[template(source = "Before{{middle}}After")]
struct EmptyMiddle {
    middle: String,
}

#[test]
fn test_empty_interpolation() {
    let t = EmptyMiddle {
        middle: String::new(),
    };
    assert_eq!(t.render(), "BeforeAfter");
}

/// Test whitespace handling.
#[derive(Template)]
#[template(source = "  {{spaced}}  ")]
struct WhitespaceTemplate {
    spaced: String,
}

#[test]
fn test_whitespace_handling() {
    let t = WhitespaceTemplate {
        spaced: "value".into(),
    };
    assert_eq!(t.render(), "  value  ");
}

/// Test with format specifier via var attribute.
#[derive(Template)]
#[template(source = "Price: {{price}}")]
struct FormattedPrice {
    #[var(format = ".2")]
    price: f64,
}

#[test]
fn test_format_specifier() {
    let t = FormattedPrice { price: 19.9 };
    assert_eq!(t.render(), "Price: 19.90");
}

/// Test with width format specifier.
#[derive(Template)]
#[template(source = "[{{padded}}]")]
struct PaddedTemplate {
    #[var(format = ">10")]
    padded: String,
}

#[test]
fn test_width_format() {
    let t = PaddedTemplate {
        padded: "test".into(),
    };
    assert_eq!(t.render(), "[      test]");
}

// Tests for template from file (path attribute)

/// Template loaded from a file.
#[derive(Template)]
#[template(path = "test_templates/greeting.txt")]
struct FileGreeting {
    name: String,
    location: String,
}

#[test]
fn test_path_attribute() {
    let greeting = FileGreeting {
        name: "Alice".into(),
        location: "Rust Land".into(),
    };
    assert_eq!(greeting.render(), "Hello, Alice! Welcome to Rust Land.");
}

#[test]
fn test_path_template_source() {
    assert_eq!(
        FileGreeting::source(),
        "Hello, {{name}}! Welcome to {{location}}."
    );
}

#[test]
fn test_path_template_variables() {
    let vars = FileGreeting::variables();
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"name"));
    assert!(vars.contains(&"location"));
}

// Tests for the template! macro
// (template! is imported above from mcpkit_template)

#[test]
fn test_template_macro_basic() {
    let name = "World";
    let result = template!("Hello, {{name}}!", name = name);
    assert_eq!(result, "Hello, World!");
}

#[test]
fn test_template_macro_multiple_vars() {
    let x = 10;
    let y = 20;
    let result = template!("Point: ({{x}}, {{y}})", x = x, y = y);
    assert_eq!(result, "Point: (10, 20)");
}

#[test]
fn test_template_macro_expression() {
    let result = template!("Sum: {{sum}}", sum = 1 + 2 + 3);
    assert_eq!(result, "Sum: 6");
}

#[test]
fn test_template_macro_method_call() {
    let s = "hello";
    let result = template!("Upper: {{upper}}", upper = s.to_uppercase());
    assert_eq!(result, "Upper: HELLO");
}

#[test]
fn test_template_macro_no_vars() {
    let result = template!("Static text");
    assert_eq!(result, "Static text");
}

#[test]
fn test_template_macro_unicode() {
    let emoji = "🎉";
    let result = template!("Celebrate: {{emoji}}", emoji = emoji);
    assert_eq!(result, "Celebrate: 🎉");
}
