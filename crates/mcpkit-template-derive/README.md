# mcpkit-template-derive

Derive macros for compile-time validated prompt templates.

## Overview

`mcpkit-template-derive` provides procedural macros for defining type-safe prompt templates with compile-time validation. This is a companion crate to `mcpkit-template` - you typically won't depend on it directly.

## Installation

This crate is re-exported by `mcpkit-template` when the `derive` feature is enabled (default):

```toml
[dependencies]
mcpkit-template = "0.5"
```

## Derive Macro

The `Template` derive macro generates an implementation of the `Template` trait:

```rust
use mcpkit_template::Template;

#[derive(Template)]
#[template(source = "Hello, {{name}}! You are {{age}} years old.")]
struct Greeting {
    name: String,
    age: u32,
}

let greeting = Greeting { name: "Alice".into(), age: 30 };
assert_eq!(greeting.render(), "Hello, Alice! You are 30 years old.");
```

### Template from File

Load templates from external files (relative to `Cargo.toml`):

```rust
use mcpkit_template::Template;

// File: templates/greeting.txt
// Contents: "Hello, {{name}}! Welcome to {{location}}."

#[derive(Template)]
#[template(path = "templates/greeting.txt")]
struct FileGreeting {
    name: String,
    location: String,
}
```

### Custom Formatting

Use the `#[var]` attribute for custom format specifiers:

```rust
use mcpkit_template::Template;

#[derive(Template)]
#[template(source = "Price: ${{price}}")]
struct PriceDisplay {
    #[var(format = ".2")]
    price: f64,
}

let display = PriceDisplay { price: 19.99 };
assert_eq!(display.render(), "Price: $19.99");
```

## Inline Template Macro

For quick, one-off templates without defining a struct:

```rust
use mcpkit_template_derive::template;

let name = "World";
let greeting = template!("Hello, {{name}}!", name = name);
assert_eq!(greeting, "Hello, World!");

// Expressions work too
let result = template!("Sum: {{sum}}", sum = 1 + 2 + 3);
assert_eq!(result, "Sum: 6");
```

## Compile-Time Validation

All templates are validated at compile time:

```rust,compile_fail
#[derive(Template)]
#[template(source = "Hello, {{missing_field}}!")]
struct Invalid {
    name: String,  // Error: no field matches {{missing_field}}
}
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
