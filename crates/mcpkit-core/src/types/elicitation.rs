//! Elicitation types for MCP servers.
//!
//! Elicitation allows servers to request structured input from the user
//! through the client. This enables interactive workflows where servers
//! can gather user preferences, confirmations, or data.

use super::meta::Meta;
use serde::{Deserialize, Serialize};

/// The mode of an elicitation request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitMode {
    /// In-band form input against a requested schema.
    Form,
    /// Out-of-band interaction via a URL the user navigates to.
    Url,
}

/// A form-mode request to elicit information from the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitRequest {
    /// The elicitation mode. Absent is equivalent to [`ElicitMode::Form`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ElicitMode>,
    /// Message explaining what information is needed.
    pub message: String,
    /// The schema describing what input is expected.
    #[serde(rename = "requestedSchema")]
    pub requested_schema: ElicitationSchema,
}

impl ElicitRequest {
    /// Create a new elicitation request.
    #[must_use]
    pub fn new(message: impl Into<String>, schema: ElicitationSchema) -> Self {
        Self {
            mode: None,
            message: message.into(),
            requested_schema: schema,
        }
    }

    /// Create a simple text input request.
    #[must_use]
    pub fn text(message: impl Into<String>, field_name: impl Into<String>) -> Self {
        Self::new(
            message,
            ElicitationSchema::object().property(field_name, PropertySchema::string()),
        )
    }

    /// Create a confirmation request.
    #[must_use]
    pub fn confirm(message: impl Into<String>) -> Self {
        Self::new(
            message,
            ElicitationSchema::object().property("confirmed", PropertySchema::boolean()),
        )
    }

    /// Create a choice selection request.
    #[must_use]
    pub fn choice(
        message: impl Into<String>,
        field_name: impl Into<String>,
        options: Vec<String>,
    ) -> Self {
        Self::new(
            message,
            ElicitationSchema::object().property(field_name, PropertySchema::enum_values(options)),
        )
    }
}

/// A URL-mode request to elicit information via an out-of-band interaction.
///
/// The user is asked to navigate to `url` (e.g. for authorization or payment).
/// The client returns an [`ElicitResult`] action immediately (consenting to open
/// the URL); the server later sends an [`ElicitationCompleteNotification`] when
/// the out-of-band interaction finishes.
///
/// Per the spec, URL mode is security-sensitive: the `elicitation_id` MUST be
/// unguessable and bound to a verified user identity, and MUST NOT carry
/// credentials in the URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlElicitRequest {
    /// The elicitation mode (always [`ElicitMode::Url`]).
    pub mode: ElicitMode,
    /// Message explaining why the interaction is needed.
    pub message: String,
    /// Opaque, unguessable id, unique within the server, correlating this
    /// elicitation with its completion notification.
    #[serde(rename = "elicitationId")]
    pub elicitation_id: String,
    /// The URL the user should navigate to.
    pub url: String,
}

impl UrlElicitRequest {
    /// Create a URL-mode elicitation request.
    #[must_use]
    pub fn new(
        message: impl Into<String>,
        elicitation_id: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            mode: ElicitMode::Url,
            message: message.into(),
            elicitation_id: elicitation_id.into(),
            url: url.into(),
        }
    }
}

/// The parameters of an `elicitation/create` request — a form or URL request,
/// discriminated by `mode` (absent = form).
///
/// Used on the client to parse either mode from the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ElicitRequestParams {
    /// A URL-mode request (`mode: "url"`).
    Url(UrlElicitRequest),
    /// A form-mode request (`mode` absent or `"form"`).
    Form(ElicitRequest),
}

/// A `notifications/elicitation/complete` notification: the out-of-band
/// interaction for a URL-mode elicitation has finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationCompleteNotification {
    /// The id of the elicitation that completed.
    #[serde(rename = "elicitationId")]
    pub elicitation_id: String,
}

impl ElicitationCompleteNotification {
    /// Create a completion notification for the given elicitation id.
    #[must_use]
    pub fn new(elicitation_id: impl Into<String>) -> Self {
        Self {
            elicitation_id: elicitation_id.into(),
        }
    }
}

/// Schema for elicitation input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitationSchema {
    /// Schema type (always "object" for elicitation).
    #[serde(rename = "type")]
    pub schema_type: String,
    /// Properties of the object.
    pub properties: serde_json::Map<String, serde_json::Value>,
    /// Required property names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ElicitationSchema {
    /// Create a new object schema.
    #[must_use]
    pub fn object() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: serde_json::Map::new(),
            required: None,
        }
    }

    /// Add a property to the schema.
    #[must_use]
    pub fn property(mut self, name: impl Into<String>, schema: PropertySchema) -> Self {
        let name = name.into();
        self.properties
            .insert(name, serde_json::to_value(schema).unwrap_or_default());
        self
    }

    /// Add a required property to the schema.
    #[must_use]
    pub fn required_property(mut self, name: impl Into<String>, schema: PropertySchema) -> Self {
        let name = name.into();
        self.properties.insert(
            name.clone(),
            serde_json::to_value(schema).unwrap_or_default(),
        );
        self.required.get_or_insert_with(Vec::new).push(name);
        self
    }
}

impl Default for ElicitationSchema {
    fn default() -> Self {
        Self::object()
    }
}

/// Schema for a single property in an elicitation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// The type of the property.
    #[serde(rename = "type")]
    pub property_type: String,
    /// Description of the property.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Default value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Minimum value (for numbers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// Maximum value (for numbers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// Minimum length (for strings).
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    /// Maximum length (for strings).
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// Pattern (for strings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Enum values (for constrained strings).
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl PropertySchema {
    /// Create a string property schema.
    #[must_use]
    pub fn string() -> Self {
        Self {
            property_type: "string".to_string(),
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            enum_values: None,
        }
    }

    /// Create a number property schema.
    #[must_use]
    pub fn number() -> Self {
        Self {
            property_type: "number".to_string(),
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            enum_values: None,
        }
    }

    /// Create an integer property schema.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            property_type: "integer".to_string(),
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            enum_values: None,
        }
    }

    /// Create a boolean property schema.
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            property_type: "boolean".to_string(),
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            enum_values: None,
        }
    }

    /// Create an enum property schema.
    #[must_use]
    pub fn enum_values(values: Vec<String>) -> Self {
        Self {
            property_type: "string".to_string(),
            description: None,
            default: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            enum_values: Some(values),
        }
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the default value.
    #[must_use]
    pub fn default_value(mut self, value: serde_json::Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Set the minimum value.
    #[must_use]
    pub const fn min(mut self, min: f64) -> Self {
        self.minimum = Some(min);
        self
    }

    /// Set the maximum value.
    #[must_use]
    pub const fn max(mut self, max: f64) -> Self {
        self.maximum = Some(max);
        self
    }

    /// Set the minimum string length.
    #[must_use]
    pub const fn min_length(mut self, len: u32) -> Self {
        self.min_length = Some(len);
        self
    }

    /// Set the maximum string length.
    #[must_use]
    pub const fn max_length(mut self, len: u32) -> Self {
        self.max_length = Some(len);
        self
    }

    /// Set a regex pattern.
    #[must_use]
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }
}

/// Result of an elicitation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElicitResult {
    /// The action taken by the user.
    pub action: ElicitAction,
    /// The content provided (if accepted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Map<String, serde_json::Value>>,
    /// Optional protocol metadata (`_meta`).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

impl ElicitResult {
    /// Create an accepted result with content.
    #[must_use]
    pub const fn accepted(content: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            action: ElicitAction::Accept,
            content: Some(content),
            meta: None,
        }
    }

    /// Create a declined result.
    #[must_use]
    pub const fn declined() -> Self {
        Self {
            action: ElicitAction::Decline,
            content: None,
            meta: None,
        }
    }

    /// Create a cancelled result.
    #[must_use]
    pub const fn cancelled() -> Self {
        Self {
            action: ElicitAction::Cancel,
            content: None,
            meta: None,
        }
    }

    /// Check if the user accepted.
    #[must_use]
    pub const fn is_accepted(&self) -> bool {
        matches!(self.action, ElicitAction::Accept)
    }

    /// Get a string value from the content.
    #[must_use]
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.content.as_ref()?.get(key)?.as_str()
    }

    /// Get a boolean value from the content.
    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.content.as_ref()?.get(key)?.as_bool()
    }

    /// Get a number value from the content.
    #[must_use]
    pub fn get_number(&self, key: &str) -> Option<f64> {
        self.content.as_ref()?.get(key)?.as_f64()
    }
}

/// The action taken in response to an elicitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitAction {
    /// User provided the requested information.
    Accept,
    /// User declined to provide information.
    Decline,
    /// User cancelled the operation.
    Cancel,
}

impl std::fmt::Display for ElicitAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accept => write!(f, "accept"),
            Self::Decline => write!(f, "decline"),
            Self::Cancel => write!(f, "cancel"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_request_serializes_with_mode_and_ids() {
        let req = UrlElicitRequest::new("authorize", "elic-123", "https://auth.example/x");
        let j = serde_json::to_value(&req).unwrap();
        assert_eq!(j["mode"], "url");
        assert_eq!(j["elicitationId"], "elic-123");
        assert_eq!(j["url"], "https://auth.example/x");
    }

    #[test]
    fn form_request_omits_mode_by_default() {
        let req = ElicitRequest::text("name?", "name");
        let j = serde_json::to_value(&req).unwrap();
        assert!(
            j.get("mode").is_none(),
            "form mode should be absent by default"
        );
        assert!(j.get("requestedSchema").is_some());
    }

    #[test]
    fn params_union_parses_both_modes() {
        // URL mode.
        let url = serde_json::json!({
            "mode": "url", "message": "go", "elicitationId": "e1", "url": "https://x"
        });
        assert!(matches!(
            serde_json::from_value::<ElicitRequestParams>(url).unwrap(),
            ElicitRequestParams::Url(_)
        ));
        // Form mode (mode absent).
        let form = serde_json::json!({
            "message": "name?",
            "requestedSchema": { "type": "object", "properties": {} }
        });
        assert!(matches!(
            serde_json::from_value::<ElicitRequestParams>(form).unwrap(),
            ElicitRequestParams::Form(_)
        ));
    }

    #[test]
    fn completion_notification_uses_elicitation_id() {
        let j = serde_json::to_value(ElicitationCompleteNotification::new("e1")).unwrap();
        assert_eq!(j["elicitationId"], "e1");
    }

    #[test]
    fn test_text_elicitation() {
        let request = ElicitRequest::text("What is your name?", "name");
        assert_eq!(request.message, "What is your name?");
        assert!(request.requested_schema.properties.contains_key("name"));
    }

    #[test]
    fn test_confirm_elicitation() {
        let request = ElicitRequest::confirm("Are you sure?");
        assert!(
            request
                .requested_schema
                .properties
                .contains_key("confirmed")
        );
    }

    #[test]
    fn test_choice_elicitation() {
        let request = ElicitRequest::choice(
            "Pick a color",
            "color",
            vec!["red".to_string(), "green".to_string(), "blue".to_string()],
        );
        assert!(request.requested_schema.properties.contains_key("color"));
    }

    #[test]
    fn test_property_schema() {
        let schema = PropertySchema::string()
            .description("A name")
            .min_length(1)
            .max_length(100);

        assert_eq!(schema.property_type, "string");
        assert_eq!(schema.min_length, Some(1));
        assert_eq!(schema.max_length, Some(100));
    }

    #[test]
    fn test_number_schema() {
        let schema = PropertySchema::number()
            .min(0.0)
            .max(100.0)
            .description("A percentage");

        assert_eq!(schema.property_type, "number");
        assert_eq!(schema.minimum, Some(0.0));
        assert_eq!(schema.maximum, Some(100.0));
    }

    #[test]
    fn test_elicit_result() {
        let mut content = serde_json::Map::new();
        content.insert(
            "name".to_string(),
            serde_json::Value::String("Alice".to_string()),
        );
        content.insert("age".to_string(), serde_json::Value::Number(30.into()));

        let result = ElicitResult::accepted(content);
        assert!(result.is_accepted());
        assert_eq!(result.get_string("name"), Some("Alice"));
        assert_eq!(result.get_number("age"), Some(30.0));
    }

    #[test]
    fn test_declined_result() {
        let result = ElicitResult::declined();
        assert!(!result.is_accepted());
        assert!(result.content.is_none());
    }

    #[test]
    fn test_complex_schema() {
        let schema = ElicitationSchema::object()
            .required_property(
                "email",
                PropertySchema::string().pattern(r"^[\w\.-]+@[\w\.-]+\.\w+$"),
            )
            .property("age", PropertySchema::integer().min(0.0).max(150.0))
            .property(
                "newsletter",
                PropertySchema::boolean().default_value(serde_json::Value::Bool(false)),
            );

        assert_eq!(schema.required, Some(vec!["email".to_string()]));
        assert!(schema.properties.contains_key("email"));
        assert!(schema.properties.contains_key("age"));
        assert!(schema.properties.contains_key("newsletter"));
    }
}
