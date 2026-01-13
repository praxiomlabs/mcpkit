//! Runtime template rendering.
//!
//! This module provides runtime template rendering for cases where
//! compile-time templates aren't suitable (e.g., user-provided templates).

use std::collections::HashMap;

use crate::error::{TemplateError, TemplateResult};

/// A runtime template that can be rendered with dynamic values.
///
/// Unlike compile-time templates, runtime templates are parsed and validated
/// at runtime. Use compile-time templates when possible for better performance
/// and earlier error detection.
///
/// # Example
///
/// ```
/// use mcpkit_template::RuntimeTemplate;
/// use std::collections::HashMap;
///
/// let template = RuntimeTemplate::new("Hello, {{name}}!").unwrap();
///
/// let mut vars = HashMap::new();
/// vars.insert("name".to_string(), "World".to_string());
///
/// let result = template.render(&vars).unwrap();
/// assert_eq!(result, "Hello, World!");
/// ```
#[derive(Debug, Clone)]
pub struct RuntimeTemplate {
    source: String,
    variables: Vec<String>,
    parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone)]
enum TemplatePart {
    Literal(String),
    Variable(String),
}

impl RuntimeTemplate {
    /// Parse a template string.
    ///
    /// # Errors
    ///
    /// Returns an error if the template has invalid syntax.
    pub fn new(source: impl Into<String>) -> TemplateResult<Self> {
        let source = source.into();
        let (parts, variables) = Self::parse(&source)?;

        Ok(Self {
            source,
            variables,
            parts,
        })
    }

    /// Get the source template string.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Get the list of variable names.
    #[must_use]
    pub fn variables(&self) -> &[String] {
        &self.variables
    }

    /// Render the template with the given variables.
    ///
    /// # Errors
    ///
    /// Returns an error if a required variable is missing.
    pub fn render(&self, vars: &HashMap<String, String>) -> TemplateResult<String> {
        let mut result = String::new();

        for part in &self.parts {
            match part {
                TemplatePart::Literal(s) => result.push_str(s),
                TemplatePart::Variable(name) => {
                    let value = vars
                        .get(name)
                        .ok_or_else(|| TemplateError::missing_variable(name))?;
                    result.push_str(value);
                }
            }
        }

        Ok(result)
    }

    /// Render with a single variable.
    ///
    /// Convenience method for templates with one variable.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable name doesn't match or rendering fails.
    pub fn render_with(&self, name: &str, value: &str) -> TemplateResult<String> {
        let mut vars = HashMap::new();
        vars.insert(name.to_string(), value.to_string());
        self.render(&vars)
    }

    /// Parse the template into parts.
    fn parse(source: &str) -> TemplateResult<(Vec<TemplatePart>, Vec<String>)> {
        let mut parts = Vec::new();
        let mut variables = Vec::new();
        let mut current_literal = String::new();
        let mut chars = source.chars().peekable();
        let mut position = 0;

        while let Some(c) = chars.next() {
            if c == '{' && chars.peek() == Some(&'{') {
                chars.next(); // consume second {
                position += 2;

                // Save current literal if any
                if !current_literal.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut current_literal)));
                }

                // Parse variable name
                let mut var_name = String::new();
                let start_pos = position;

                loop {
                    match chars.next() {
                        Some('}') if chars.peek() == Some(&'}') => {
                            chars.next(); // consume second }
                            position += 2;
                            break;
                        }
                        Some(c) if c.is_alphanumeric() || c == '_' => {
                            var_name.push(c);
                            position += 1;
                        }
                        Some(c) => {
                            return Err(TemplateError::syntax_error_at(
                                format!("invalid character '{c}' in variable name"),
                                start_pos,
                            ));
                        }
                        None => {
                            return Err(TemplateError::syntax_error_at(
                                "unclosed variable placeholder",
                                start_pos - 2,
                            ));
                        }
                    }
                }

                if var_name.is_empty() {
                    return Err(TemplateError::syntax_error_at(
                        "empty variable name",
                        start_pos,
                    ));
                }

                if !variables.contains(&var_name) {
                    variables.push(var_name.clone());
                }
                parts.push(TemplatePart::Variable(var_name));
            } else {
                current_literal.push(c);
                position += 1;
            }
        }

        // Add remaining literal if any
        if !current_literal.is_empty() {
            parts.push(TemplatePart::Literal(current_literal));
        }

        Ok((parts, variables))
    }
}

/// A template builder for constructing templates with validation.
#[derive(Debug, Default)]
pub struct TemplateBuilder {
    parts: Vec<String>,
}

impl TemplateBuilder {
    /// Create a new template builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add literal text.
    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.parts.push(text.into());
        self
    }

    /// Add a variable placeholder.
    #[must_use]
    pub fn var(mut self, name: impl Into<String>) -> Self {
        self.parts.push(format!("{{{{{}}}}}", name.into()));
        self
    }

    /// Add a newline.
    #[must_use]
    pub fn newline(self) -> Self {
        self.text("\n")
    }

    /// Build the template.
    ///
    /// # Errors
    ///
    /// Returns an error if the template has invalid syntax.
    pub fn build(self) -> TemplateResult<RuntimeTemplate> {
        let source = self.parts.join("");
        RuntimeTemplate::new(source)
    }
}

/// Validate a template string without parsing it.
///
/// # Errors
///
/// Returns an error if the template has invalid syntax.
pub fn validate_template(source: &str) -> TemplateResult<Vec<String>> {
    let template = RuntimeTemplate::new(source)?;
    Ok(template.variables().to_vec())
}

/// Extract variable names from a template string.
///
/// Returns an empty vector if the template has no variables or is invalid.
#[must_use]
pub fn extract_variables(source: &str) -> Vec<String> {
    RuntimeTemplate::new(source)
        .map(|t| t.variables().to_vec())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_template() {
        let template = RuntimeTemplate::new("Hello, {{name}}!").unwrap();

        assert_eq!(template.variables(), &["name"]);

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_multiple_variables() {
        let template = RuntimeTemplate::new("{{greeting}}, {{name}}!").unwrap();

        assert_eq!(template.variables(), &["greeting", "name"]);

        let mut vars = HashMap::new();
        vars.insert("greeting".to_string(), "Hello".to_string());
        vars.insert("name".to_string(), "World".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_missing_variable() {
        let template = RuntimeTemplate::new("Hello, {{name}}!").unwrap();
        let vars = HashMap::new();

        let result = template.render(&vars);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_variable_name() {
        let result = RuntimeTemplate::new("Hello, {{}}!");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_variable() {
        let result = RuntimeTemplate::new("Hello, {{name!");
        assert!(result.is_err());
    }

    #[test]
    fn test_template_builder() {
        let template = TemplateBuilder::new()
            .text("Hello, ")
            .var("name")
            .text("!")
            .build()
            .unwrap();

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "World".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_with() {
        let template = RuntimeTemplate::new("Hello, {{name}}!").unwrap();
        let result = template.render_with("name", "World").unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_no_variables() {
        let template = RuntimeTemplate::new("Hello, World!").unwrap();
        assert!(template.variables().is_empty());

        let result = template.render(&HashMap::new()).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_repeated_variable() {
        let template = RuntimeTemplate::new("{{name}} says: {{name}}!").unwrap();

        // Variable should only appear once in the list
        assert_eq!(template.variables(), &["name"]);

        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Alice".to_string());

        let result = template.render(&vars).unwrap();
        assert_eq!(result, "Alice says: Alice!");
    }

    #[test]
    fn test_validate_template() {
        let vars = validate_template("{{a}} and {{b}}").unwrap();
        assert_eq!(vars, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_variables() {
        let vars = extract_variables("{{x}} + {{y}} = {{z}}");
        assert_eq!(vars, vec!["x", "y", "z"]);
    }
}
