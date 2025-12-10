//! JSON Schema utilities for MCP type validation.
//!
//! This module provides utilities for working with JSON Schema, which is used
//! throughout the MCP protocol for defining tool input schemas, resource
//! schemas, and elicitation schemas.
//!
//! # Features
//!
//! - Schema building with a fluent API
//! - Common schema patterns (string, number, object, array)
//! - Schema validation helpers
//!
//! # Example
//!
//! ```rust
//! use mcp_core::schema::{SchemaBuilder, SchemaType};
//!
//! // Build a schema for a search tool input
//! let schema = SchemaBuilder::object()
//!     .property("query", SchemaBuilder::string().description("Search query"))
//!     .property("limit", SchemaBuilder::integer().minimum(1).maximum(100).default_value(10))
//!     .required(["query"])
//!     .build();
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// JSON Schema types as defined by the JSON Schema specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    /// A string value.
    String,
    /// A numeric value (integer or float).
    Number,
    /// An integer value.
    Integer,
    /// A boolean value.
    Boolean,
    /// An array value.
    Array,
    /// An object value.
    Object,
    /// A null value.
    Null,
}

impl std::fmt::Display for SchemaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Number => write!(f, "number"),
            Self::Integer => write!(f, "integer"),
            Self::Boolean => write!(f, "boolean"),
            Self::Array => write!(f, "array"),
            Self::Object => write!(f, "object"),
            Self::Null => write!(f, "null"),
        }
    }
}

/// A JSON Schema definition.
///
/// This struct represents a JSON Schema that can be used for validating
/// tool inputs, resource contents, or elicitation responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    /// The schema type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<SchemaType>,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Default value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,

    /// Enumerated allowed values.
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    /// Constant value (must be exactly this value).
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<Value>,

    // String constraints
    /// Minimum string length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,

    /// Maximum string length.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,

    /// Regex pattern for string validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Format hint (e.g., "email", "uri", "date-time").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    // Numeric constraints
    /// Minimum numeric value (inclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Maximum numeric value (inclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// Minimum numeric value (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<f64>,

    /// Maximum numeric value (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<f64>,

    /// Value must be a multiple of this number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f64>,

    // Array constraints
    /// Schema for array items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,

    /// Minimum number of items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,

    /// Maximum number of items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,

    /// Whether items must be unique.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,

    // Object constraints
    /// Property schemas for object type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, Schema>>,

    /// Required property names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    /// Schema for additional properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<AdditionalProperties>,

    /// Minimum number of properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_properties: Option<u64>,

    /// Maximum number of properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_properties: Option<u64>,

    // Composition
    /// All of these schemas must match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_of: Option<Vec<Schema>>,

    /// Any of these schemas must match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<Schema>>,

    /// Exactly one of these schemas must match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Schema>>,

    /// This schema must not match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<Schema>>,
}

/// Represents the `additionalProperties` field which can be a boolean or a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdditionalProperties {
    /// Boolean: true allows any additional properties, false forbids them.
    Boolean(bool),
    /// Schema: additional properties must match this schema.
    Schema(Box<Schema>),
}

impl Schema {
    /// Create an empty schema (matches anything).
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// Create a string schema.
    #[must_use]
    pub fn string() -> Self {
        Self {
            schema_type: Some(SchemaType::String),
            ..Default::default()
        }
    }

    /// Create a number schema.
    #[must_use]
    pub fn number() -> Self {
        Self {
            schema_type: Some(SchemaType::Number),
            ..Default::default()
        }
    }

    /// Create an integer schema.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            schema_type: Some(SchemaType::Integer),
            ..Default::default()
        }
    }

    /// Create a boolean schema.
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            schema_type: Some(SchemaType::Boolean),
            ..Default::default()
        }
    }

    /// Create an array schema.
    #[must_use]
    pub fn array() -> Self {
        Self {
            schema_type: Some(SchemaType::Array),
            ..Default::default()
        }
    }

    /// Create an object schema.
    #[must_use]
    pub fn object() -> Self {
        Self {
            schema_type: Some(SchemaType::Object),
            ..Default::default()
        }
    }

    /// Create a null schema.
    #[must_use]
    pub fn null() -> Self {
        Self {
            schema_type: Some(SchemaType::Null),
            ..Default::default()
        }
    }

    /// Convert this schema to a JSON value.
    #[must_use]
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Object(serde_json::Map::new()))
    }
}

/// A fluent builder for constructing JSON Schemas.
///
/// # Example
///
/// ```rust
/// use mcp_core::schema::SchemaBuilder;
///
/// let schema = SchemaBuilder::object()
///     .title("SearchInput")
///     .description("Input for search tool")
///     .property("query", SchemaBuilder::string().min_length(1))
///     .property("page", SchemaBuilder::integer().minimum(1).default_value(1))
///     .required(["query"])
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct SchemaBuilder {
    schema: Schema,
}

impl SchemaBuilder {
    /// Create a new schema builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a string schema builder.
    #[must_use]
    pub fn string() -> Self {
        Self {
            schema: Schema::string(),
        }
    }

    /// Create a number schema builder.
    #[must_use]
    pub fn number() -> Self {
        Self {
            schema: Schema::number(),
        }
    }

    /// Create an integer schema builder.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            schema: Schema::integer(),
        }
    }

    /// Create a boolean schema builder.
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            schema: Schema::boolean(),
        }
    }

    /// Create an array schema builder.
    #[must_use]
    pub fn array() -> Self {
        Self {
            schema: Schema::array(),
        }
    }

    /// Create an object schema builder.
    #[must_use]
    pub fn object() -> Self {
        Self {
            schema: Schema::object(),
        }
    }

    /// Create a null schema builder.
    #[must_use]
    pub fn null() -> Self {
        Self {
            schema: Schema::null(),
        }
    }

    /// Set the schema title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.schema.title = Some(title.into());
        self
    }

    /// Set the schema description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.schema.description = Some(description.into());
        self
    }

    /// Set the default value.
    #[must_use]
    pub fn default_value(mut self, default: impl Into<Value>) -> Self {
        self.schema.default = Some(default.into());
        self
    }

    /// Set enumerated allowed values.
    #[must_use]
    pub fn enum_values<I, V>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = V>,
        V: Into<Value>,
    {
        self.schema.enum_values = Some(values.into_iter().map(Into::into).collect());
        self
    }

    /// Set a constant value.
    #[must_use]
    pub fn const_value(mut self, value: impl Into<Value>) -> Self {
        self.schema.const_value = Some(value.into());
        self
    }

    // String constraints

    /// Set minimum string length.
    #[must_use]
    pub fn min_length(mut self, min: u64) -> Self {
        self.schema.min_length = Some(min);
        self
    }

    /// Set maximum string length.
    #[must_use]
    pub fn max_length(mut self, max: u64) -> Self {
        self.schema.max_length = Some(max);
        self
    }

    /// Set regex pattern.
    #[must_use]
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.schema.pattern = Some(pattern.into());
        self
    }

    /// Set string format hint.
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.schema.format = Some(format.into());
        self
    }

    // Numeric constraints

    /// Set minimum value (inclusive).
    #[must_use]
    pub fn minimum(mut self, min: impl Into<f64>) -> Self {
        self.schema.minimum = Some(min.into());
        self
    }

    /// Set maximum value (inclusive).
    #[must_use]
    pub fn maximum(mut self, max: impl Into<f64>) -> Self {
        self.schema.maximum = Some(max.into());
        self
    }

    /// Set exclusive minimum value.
    #[must_use]
    pub fn exclusive_minimum(mut self, min: impl Into<f64>) -> Self {
        self.schema.exclusive_minimum = Some(min.into());
        self
    }

    /// Set exclusive maximum value.
    #[must_use]
    pub fn exclusive_maximum(mut self, max: impl Into<f64>) -> Self {
        self.schema.exclusive_maximum = Some(max.into());
        self
    }

    /// Set multiple-of constraint.
    #[must_use]
    pub fn multiple_of(mut self, multiple: impl Into<f64>) -> Self {
        self.schema.multiple_of = Some(multiple.into());
        self
    }

    // Array constraints

    /// Set schema for array items.
    #[must_use]
    pub fn items(mut self, items: SchemaBuilder) -> Self {
        self.schema.items = Some(Box::new(items.schema));
        self
    }

    /// Set minimum number of items.
    #[must_use]
    pub fn min_items(mut self, min: u64) -> Self {
        self.schema.min_items = Some(min);
        self
    }

    /// Set maximum number of items.
    #[must_use]
    pub fn max_items(mut self, max: u64) -> Self {
        self.schema.max_items = Some(max);
        self
    }

    /// Require unique items.
    #[must_use]
    pub fn unique_items(mut self, unique: bool) -> Self {
        self.schema.unique_items = Some(unique);
        self
    }

    // Object constraints

    /// Add a property to the schema.
    #[must_use]
    pub fn property(mut self, name: impl Into<String>, schema: SchemaBuilder) -> Self {
        let properties = self.schema.properties.get_or_insert_with(HashMap::new);
        properties.insert(name.into(), schema.schema);
        self
    }

    /// Set required properties.
    #[must_use]
    pub fn required<I, S>(mut self, required: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.schema.required = Some(required.into_iter().map(Into::into).collect());
        self
    }

    /// Set whether additional properties are allowed.
    #[must_use]
    pub fn additional_properties(mut self, allowed: bool) -> Self {
        self.schema.additional_properties = Some(AdditionalProperties::Boolean(allowed));
        self
    }

    /// Set schema for additional properties.
    #[must_use]
    pub fn additional_properties_schema(mut self, schema: SchemaBuilder) -> Self {
        self.schema.additional_properties =
            Some(AdditionalProperties::Schema(Box::new(schema.schema)));
        self
    }

    /// Set minimum number of properties.
    #[must_use]
    pub fn min_properties(mut self, min: u64) -> Self {
        self.schema.min_properties = Some(min);
        self
    }

    /// Set maximum number of properties.
    #[must_use]
    pub fn max_properties(mut self, max: u64) -> Self {
        self.schema.max_properties = Some(max);
        self
    }

    // Composition

    /// Require all of the given schemas to match.
    #[must_use]
    pub fn all_of<I>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = SchemaBuilder>,
    {
        self.schema.all_of = Some(schemas.into_iter().map(|b| b.schema).collect());
        self
    }

    /// Require any of the given schemas to match.
    #[must_use]
    pub fn any_of<I>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = SchemaBuilder>,
    {
        self.schema.any_of = Some(schemas.into_iter().map(|b| b.schema).collect());
        self
    }

    /// Require exactly one of the given schemas to match.
    #[must_use]
    pub fn one_of<I>(mut self, schemas: I) -> Self
    where
        I: IntoIterator<Item = SchemaBuilder>,
    {
        self.schema.one_of = Some(schemas.into_iter().map(|b| b.schema).collect());
        self
    }

    /// Require the given schema to not match.
    #[must_use]
    pub fn not(mut self, schema: SchemaBuilder) -> Self {
        self.schema.not = Some(Box::new(schema.schema));
        self
    }

    /// Build the schema.
    #[must_use]
    pub fn build(self) -> Schema {
        self.schema
    }

    /// Build and convert to a JSON value.
    #[must_use]
    pub fn to_value(self) -> Value {
        self.schema.to_value()
    }
}

/// Common string format hints.
pub mod formats {
    /// Email address format.
    pub const EMAIL: &str = "email";
    /// URI format.
    pub const URI: &str = "uri";
    /// URI reference format.
    pub const URI_REFERENCE: &str = "uri-reference";
    /// Date-time format (RFC 3339).
    pub const DATE_TIME: &str = "date-time";
    /// Date format.
    pub const DATE: &str = "date";
    /// Time format.
    pub const TIME: &str = "time";
    /// Duration format (ISO 8601).
    pub const DURATION: &str = "duration";
    /// UUID format.
    pub const UUID: &str = "uuid";
    /// Hostname format.
    pub const HOSTNAME: &str = "hostname";
    /// IPv4 address format.
    pub const IPV4: &str = "ipv4";
    /// IPv6 address format.
    pub const IPV6: &str = "ipv6";
    /// JSON Pointer format.
    pub const JSON_POINTER: &str = "json-pointer";
    /// Regex format.
    pub const REGEX: &str = "regex";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_schema() {
        let schema = SchemaBuilder::string()
            .description("A test string")
            .min_length(1)
            .max_length(100)
            .pattern(r"^[a-z]+$")
            .build();

        assert_eq!(schema.schema_type, Some(SchemaType::String));
        assert_eq!(schema.description.as_deref(), Some("A test string"));
        assert_eq!(schema.min_length, Some(1));
        assert_eq!(schema.max_length, Some(100));
    }

    #[test]
    fn test_number_schema() {
        let schema = SchemaBuilder::number()
            .minimum(0)
            .maximum(100)
            .build();

        assert_eq!(schema.schema_type, Some(SchemaType::Number));
        assert_eq!(schema.minimum, Some(0.0));
        assert_eq!(schema.maximum, Some(100.0));
    }

    #[test]
    fn test_object_schema() {
        let schema = SchemaBuilder::object()
            .property("name", SchemaBuilder::string())
            .property("age", SchemaBuilder::integer().minimum(0))
            .required(["name"])
            .additional_properties(false)
            .build();

        assert_eq!(schema.schema_type, Some(SchemaType::Object));
        assert!(schema.properties.is_some());
        let props = schema.properties.as_ref().unwrap();
        assert!(props.contains_key("name"));
        assert!(props.contains_key("age"));
        assert_eq!(schema.required, Some(vec!["name".to_string()]));
    }

    #[test]
    fn test_array_schema() {
        let schema = SchemaBuilder::array()
            .items(SchemaBuilder::string())
            .min_items(1)
            .unique_items(true)
            .build();

        assert_eq!(schema.schema_type, Some(SchemaType::Array));
        assert!(schema.items.is_some());
        assert_eq!(schema.min_items, Some(1));
        assert_eq!(schema.unique_items, Some(true));
    }

    #[test]
    fn test_enum_schema() {
        let schema = SchemaBuilder::string()
            .enum_values(["red", "green", "blue"])
            .build();

        assert!(schema.enum_values.is_some());
        let values = schema.enum_values.as_ref().unwrap();
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_composition() {
        let schema = SchemaBuilder::new()
            .one_of([
                SchemaBuilder::string(),
                SchemaBuilder::integer(),
            ])
            .build();

        assert!(schema.one_of.is_some());
        assert_eq!(schema.one_of.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_to_value() {
        let schema = SchemaBuilder::object()
            .property("query", SchemaBuilder::string())
            .required(["query"])
            .to_value();

        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }

    #[test]
    fn test_tool_input_schema() {
        // Example: Schema for a search tool
        let schema = SchemaBuilder::object()
            .title("SearchInput")
            .description("Input parameters for the search tool")
            .property(
                "query",
                SchemaBuilder::string()
                    .description("The search query")
                    .min_length(1),
            )
            .property(
                "limit",
                SchemaBuilder::integer()
                    .description("Maximum number of results")
                    .minimum(1)
                    .maximum(100)
                    .default_value(10),
            )
            .property(
                "filters",
                SchemaBuilder::array()
                    .items(SchemaBuilder::string())
                    .description("Optional filter tags"),
            )
            .required(["query"])
            .additional_properties(false)
            .build();

        let value = schema.to_value();
        assert!(value.is_object());

        // Verify structure
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
        assert_eq!(
            obj.get("title").and_then(|v| v.as_str()),
            Some("SearchInput")
        );
    }
}
