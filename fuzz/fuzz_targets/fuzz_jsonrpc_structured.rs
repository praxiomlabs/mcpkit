//! Structure-aware fuzz target for JSON-RPC messages.
//!
//! This fuzzer uses the `arbitrary` crate to generate structured inputs
//! that are more likely to exercise interesting code paths.

#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use mcpkit_core::protocol::{Message, Notification, Request, Response};
use serde_json::Value;

/// A structured input for fuzzing JSON-RPC messages.
#[derive(Debug, Clone)]
struct FuzzInput {
    /// The type of message to generate.
    message_type: MessageType,
    /// The JSON-RPC version (should be "2.0" but we test other values).
    jsonrpc: String,
    /// Request ID (numeric or string).
    id: IdType,
    /// Method name.
    method: String,
    /// Whether to include params.
    include_params: bool,
    /// Parameter structure.
    params: ParamType,
    /// Whether to include a result (for responses).
    include_result: bool,
    /// Whether to include an error (for responses).
    include_error: bool,
    /// Error code for error responses.
    error_code: i32,
    /// Error message.
    error_message: String,
}

#[derive(Debug, Clone, Arbitrary)]
enum MessageType {
    Request,
    Response,
    Notification,
}

#[derive(Debug, Clone, Arbitrary)]
enum IdType {
    Number(u64),
    String(String),
    Null,
}

#[derive(Debug, Clone, Arbitrary)]
enum ParamType {
    Object(Vec<(String, SimpleValue)>),
    Array(Vec<SimpleValue>),
    Null,
}

#[derive(Debug, Clone, Arbitrary)]
enum SimpleValue {
    Null,
    Bool(bool),
    Number(i64),
    Float(f64),
    String(String),
}

impl<'a> Arbitrary<'a> for FuzzInput {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Ok(FuzzInput {
            message_type: MessageType::arbitrary(u)?,
            jsonrpc: if u.ratio(9, 10)? {
                "2.0".to_string()
            } else {
                String::arbitrary(u)?
            },
            id: IdType::arbitrary(u)?,
            method: String::arbitrary(u)?,
            include_params: bool::arbitrary(u)?,
            params: ParamType::arbitrary(u)?,
            include_result: bool::arbitrary(u)?,
            include_error: bool::arbitrary(u)?,
            error_code: i32::arbitrary(u)?,
            error_message: String::arbitrary(u)?,
        })
    }
}

impl From<SimpleValue> for Value {
    fn from(v: SimpleValue) -> Self {
        match v {
            SimpleValue::Null => Value::Null,
            SimpleValue::Bool(b) => Value::Bool(b),
            SimpleValue::Number(n) => Value::Number(n.into()),
            SimpleValue::Float(f) => {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            SimpleValue::String(s) => Value::String(s),
        }
    }
}

impl From<ParamType> for Value {
    fn from(p: ParamType) -> Self {
        match p {
            ParamType::Object(pairs) => {
                let map: serde_json::Map<String, Value> = pairs
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect();
                Value::Object(map)
            }
            ParamType::Array(items) => {
                Value::Array(items.into_iter().map(Into::into).collect())
            }
            ParamType::Null => Value::Null,
        }
    }
}

fn generate_json(input: &FuzzInput) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("jsonrpc".to_string(), Value::String(input.jsonrpc.clone()));

    match &input.message_type {
        MessageType::Request => {
            match &input.id {
                IdType::Number(n) => {
                    obj.insert("id".to_string(), Value::Number((*n).into()));
                }
                IdType::String(s) => {
                    obj.insert("id".to_string(), Value::String(s.clone()));
                }
                IdType::Null => {
                    obj.insert("id".to_string(), Value::Null);
                }
            }
            obj.insert("method".to_string(), Value::String(input.method.clone()));
            if input.include_params {
                obj.insert("params".to_string(), input.params.clone().into());
            }
        }
        MessageType::Response => {
            match &input.id {
                IdType::Number(n) => {
                    obj.insert("id".to_string(), Value::Number((*n).into()));
                }
                IdType::String(s) => {
                    obj.insert("id".to_string(), Value::String(s.clone()));
                }
                IdType::Null => {
                    obj.insert("id".to_string(), Value::Null);
                }
            }
            if input.include_result {
                obj.insert("result".to_string(), input.params.clone().into());
            }
            if input.include_error {
                let mut error = serde_json::Map::new();
                error.insert("code".to_string(), Value::Number(input.error_code.into()));
                error.insert(
                    "message".to_string(),
                    Value::String(input.error_message.clone()),
                );
                obj.insert("error".to_string(), Value::Object(error));
            }
        }
        MessageType::Notification => {
            obj.insert("method".to_string(), Value::String(input.method.clone()));
            if input.include_params {
                obj.insert("params".to_string(), input.params.clone().into());
            }
        }
    }

    Value::Object(obj)
}

fuzz_target!(|input: FuzzInput| {
    // Generate JSON from structured input
    let json_value = generate_json(&input);

    // Try to serialize to string
    if let Ok(json_str) = serde_json::to_string(&json_value) {
        // Try to parse as Message
        let _ = serde_json::from_str::<Message>(&json_str);

        // Try to parse as specific types
        let _ = serde_json::from_str::<Request>(&json_str);
        let _ = serde_json::from_str::<Response>(&json_str);
        let _ = serde_json::from_str::<Notification>(&json_str);
    }

    // Also try to serialize to bytes and parse
    if let Ok(json_bytes) = serde_json::to_vec(&json_value) {
        let _ = serde_json::from_slice::<Message>(&json_bytes);
    }
});
