//! Error types for template operations.

use std::fmt;

/// Result type for template operations.
pub type TemplateResult<T> = Result<T, TemplateError>;

/// Errors that can occur during template operations.
#[derive(Debug, Clone)]
pub enum TemplateError {
    /// A required variable is missing.
    MissingVariable {
        /// The name of the missing variable.
        name: String,
    },

    /// A variable has an invalid type.
    InvalidType {
        /// The name of the variable.
        name: String,
        /// Expected type description.
        expected: String,
        /// Actual type description.
        actual: String,
    },

    /// Template syntax error.
    SyntaxError {
        /// Description of the syntax error.
        message: String,
        /// Position in the template where the error occurred.
        position: Option<usize>,
    },

    /// Template rendering failed.
    RenderError {
        /// Description of the render error.
        message: String,
    },

    /// Template composition error.
    CompositionError {
        /// Description of the composition error.
        message: String,
    },
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVariable { name } => {
                write!(f, "missing template variable: {name}")
            }
            Self::InvalidType {
                name,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "invalid type for variable '{name}': expected {expected}, got {actual}"
                )
            }
            Self::SyntaxError { message, position } => {
                if let Some(pos) = position {
                    write!(f, "template syntax error at position {pos}: {message}")
                } else {
                    write!(f, "template syntax error: {message}")
                }
            }
            Self::RenderError { message } => {
                write!(f, "template render error: {message}")
            }
            Self::CompositionError { message } => {
                write!(f, "template composition error: {message}")
            }
        }
    }
}

impl std::error::Error for TemplateError {}

impl TemplateError {
    /// Create a missing variable error.
    #[must_use]
    pub fn missing_variable(name: impl Into<String>) -> Self {
        Self::MissingVariable { name: name.into() }
    }

    /// Create an invalid type error.
    #[must_use]
    pub fn invalid_type(
        name: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::InvalidType {
            name: name.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Create a syntax error.
    #[must_use]
    pub fn syntax_error(message: impl Into<String>) -> Self {
        Self::SyntaxError {
            message: message.into(),
            position: None,
        }
    }

    /// Create a syntax error with position.
    #[must_use]
    pub fn syntax_error_at(message: impl Into<String>, position: usize) -> Self {
        Self::SyntaxError {
            message: message.into(),
            position: Some(position),
        }
    }

    /// Create a render error.
    #[must_use]
    pub fn render_error(message: impl Into<String>) -> Self {
        Self::RenderError {
            message: message.into(),
        }
    }

    /// Create a composition error.
    #[must_use]
    pub fn composition_error(message: impl Into<String>) -> Self {
        Self::CompositionError {
            message: message.into(),
        }
    }
}
