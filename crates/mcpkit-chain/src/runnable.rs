//! Core Runnable trait and value types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::error::ChainResult;

/// A dynamic value that can be passed through chains.
///
/// `ChainValue` provides a type-erased container for passing data between
/// chain steps. It supports common types and can be converted to/from
/// strongly typed values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChainValue {
    /// Null/empty value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Floating point value.
    Float(f64),
    /// String value.
    String(String),
    /// Array of values.
    Array(Vec<ChainValue>),
    /// Object/map of values.
    Object(HashMap<String, ChainValue>),
}

impl Default for ChainValue {
    fn default() -> Self {
        Self::Null
    }
}

impl ChainValue {
    /// Create a null value.
    #[must_use]
    pub fn null() -> Self {
        Self::Null
    }

    /// Check if this is a null value.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Get as a string reference.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as a boolean.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as an integer.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Get as a float.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Get as an array reference.
    #[must_use]
    pub fn as_array(&self) -> Option<&Vec<ChainValue>> {
        match self {
            Self::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Get as an object reference.
    #[must_use]
    pub fn as_object(&self) -> Option<&HashMap<String, ChainValue>> {
        match self {
            Self::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Get a field from an object.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&ChainValue> {
        match self {
            Self::Object(obj) => obj.get(key),
            _ => None,
        }
    }

    /// Convert to a string representation.
    #[must_use]
    pub fn to_string_value(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::String(s) => s.clone(),
            Self::Array(_) | Self::Object(_) => {
                serde_json::to_string(self).unwrap_or_default()
            }
        }
    }

    /// Get type name for error messages.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }
}

impl fmt::Display for ChainValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(f, "null"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::String(s) => write!(f, "{s}"),
            Self::Array(arr) => write!(f, "{arr:?}"),
            Self::Object(obj) => write!(f, "{obj:?}"),
        }
    }
}

// Conversions from common types
impl From<String> for ChainValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for ChainValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<bool> for ChainValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i32> for ChainValue {
    fn from(i: i32) -> Self {
        Self::Int(i64::from(i))
    }
}

impl From<i64> for ChainValue {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for ChainValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<Vec<ChainValue>> for ChainValue {
    fn from(arr: Vec<ChainValue>) -> Self {
        Self::Array(arr)
    }
}

impl From<HashMap<String, ChainValue>> for ChainValue {
    fn from(obj: HashMap<String, ChainValue>) -> Self {
        Self::Object(obj)
    }
}

impl From<serde_json::Value> for ChainValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Self::Float(f)
                } else {
                    Self::Null
                }
            }
            serde_json::Value::String(s) => Self::String(s),
            serde_json::Value::Array(arr) => {
                Self::Array(arr.into_iter().map(ChainValue::from).collect())
            }
            serde_json::Value::Object(obj) => {
                Self::Object(obj.into_iter().map(|(k, v)| (k, ChainValue::from(v))).collect())
            }
        }
    }
}

impl From<ChainValue> for serde_json::Value {
    fn from(v: ChainValue) -> Self {
        match v {
            ChainValue::Null => serde_json::Value::Null,
            ChainValue::Bool(b) => serde_json::Value::Bool(b),
            ChainValue::Int(i) => serde_json::Value::Number(i.into()),
            ChainValue::Float(f) => {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
            ChainValue::String(s) => serde_json::Value::String(s),
            ChainValue::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(serde_json::Value::from).collect())
            }
            ChainValue::Object(obj) => {
                serde_json::Value::Object(
                    obj.into_iter()
                        .map(|(k, v)| (k, serde_json::Value::from(v)))
                        .collect(),
                )
            }
        }
    }
}

/// The core trait for composable chain operations.
///
/// A `Runnable` is an async operation that takes an input and produces an output.
/// Runnables can be composed using combinators like `then`, `parallel`, and `branch`.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_chain::{Runnable, ChainValue, RunnableFn};
///
/// // Create a runnable from a function
/// let uppercase = RunnableFn::new(|input: ChainValue| async move {
///     Ok(ChainValue::String(input.to_string_value().to_uppercase()))
/// });
///
/// // Execute
/// let result = uppercase.invoke(ChainValue::from("hello")).await?;
/// assert_eq!(result.as_str(), Some("HELLO"));
/// ```
#[async_trait]
pub trait Runnable: Send + Sync {
    /// Execute this runnable with the given input.
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue>;

    /// Get the name of this runnable for debugging.
    fn name(&self) -> &str {
        "Runnable"
    }

    /// Chain this runnable with another, executing sequentially.
    fn then<R: Runnable + 'static>(self, next: R) -> RunnableSequence
    where
        Self: Sized + 'static,
    {
        RunnableSequence::new(vec![Arc::new(self), Arc::new(next)])
    }

    /// Execute multiple runnables in parallel, combining outputs.
    fn parallel<R: Runnable + 'static>(self, other: R) -> RunnableParallel
    where
        Self: Sized + 'static,
    {
        RunnableParallel::new(vec![
            ("left".to_string(), Arc::new(self)),
            ("right".to_string(), Arc::new(other)),
        ])
    }
}

/// A sequence of runnables executed in order.
///
/// Each runnable's output becomes the next runnable's input.
pub struct RunnableSequence {
    steps: Vec<Arc<dyn Runnable>>,
    name: String,
}

impl RunnableSequence {
    /// Create a new sequence from a list of runnables.
    pub fn new(steps: Vec<Arc<dyn Runnable>>) -> Self {
        Self {
            steps,
            name: "Sequence".to_string(),
        }
    }

    /// Set a custom name for this sequence.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a step to the sequence.
    pub fn push<R: Runnable + 'static>(&mut self, runnable: R) {
        self.steps.push(Arc::new(runnable));
    }

    /// Get the number of steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[async_trait]
impl Runnable for RunnableSequence {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        let mut current = input;
        for step in &self.steps {
            current = step.invoke(current).await?;
        }
        Ok(current)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Execute multiple runnables in parallel.
///
/// All runnables receive the same input. Outputs are combined into an object
/// with keys corresponding to each runnable.
pub struct RunnableParallel {
    branches: Vec<(String, Arc<dyn Runnable>)>,
    name: String,
}

impl RunnableParallel {
    /// Create a new parallel runnable.
    pub fn new(branches: Vec<(String, Arc<dyn Runnable>)>) -> Self {
        Self {
            branches,
            name: "Parallel".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Add a named branch.
    pub fn add<R: Runnable + 'static>(&mut self, name: impl Into<String>, runnable: R) {
        self.branches.push((name.into(), Arc::new(runnable)));
    }

    /// Get the number of branches.
    #[must_use]
    pub fn len(&self) -> usize {
        self.branches.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
}

#[async_trait]
impl Runnable for RunnableParallel {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        use futures::future::join_all;

        let futures: Vec<_> = self
            .branches
            .iter()
            .map(|(name, runnable)| {
                let name = name.clone();
                let input = input.clone();
                let runnable = Arc::clone(runnable);
                async move {
                    let result = runnable.invoke(input).await;
                    (name, result)
                }
            })
            .collect();

        let results = join_all(futures).await;
        let mut output = HashMap::new();

        for (name, result) in results {
            output.insert(name, result?);
        }

        Ok(ChainValue::Object(output))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_value_conversions() {
        let v = ChainValue::from("hello");
        assert_eq!(v.as_str(), Some("hello"));

        let v = ChainValue::from(42);
        assert_eq!(v.as_int(), Some(42));

        let v = ChainValue::from(3.14);
        assert!((v.as_float().unwrap() - 3.14).abs() < 0.001);

        let v = ChainValue::from(true);
        assert_eq!(v.as_bool(), Some(true));
    }

    #[test]
    fn test_chain_value_json_roundtrip() {
        let original = ChainValue::Object(
            [("key".to_string(), ChainValue::String("value".to_string()))]
                .into_iter()
                .collect(),
        );

        let json: serde_json::Value = original.clone().into();
        let back: ChainValue = json.into();

        assert_eq!(back.get("key").and_then(|v| v.as_str()), Some("value"));
    }

    #[test]
    fn test_chain_value_type_names() {
        assert_eq!(ChainValue::Null.type_name(), "null");
        assert_eq!(ChainValue::Bool(true).type_name(), "bool");
        assert_eq!(ChainValue::Int(1).type_name(), "int");
        assert_eq!(ChainValue::Float(1.0).type_name(), "float");
        assert_eq!(ChainValue::String("".to_string()).type_name(), "string");
        assert_eq!(ChainValue::Array(vec![]).type_name(), "array");
        assert_eq!(ChainValue::Object(HashMap::new()).type_name(), "object");
    }
}
