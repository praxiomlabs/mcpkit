//! Built-in runnable implementations.

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::{ChainError, ChainResult};
use crate::runnable::{ChainValue, Runnable};

/// A runnable created from an async function.
///
/// # Example
///
/// ```rust,ignore
/// use mcpkit_chain::{RunnableFn, ChainValue};
///
/// let uppercase = RunnableFn::new(|input: ChainValue| async move {
///     Ok(ChainValue::from(input.to_string_value().to_uppercase()))
/// });
/// ```
pub struct RunnableFn<F>
where
    F: Fn(ChainValue) -> Pin<Box<dyn Future<Output = ChainResult<ChainValue>> + Send>>
        + Send
        + Sync,
{
    func: F,
    name: String,
}

impl<F> RunnableFn<F>
where
    F: Fn(ChainValue) -> Pin<Box<dyn Future<Output = ChainResult<ChainValue>> + Send>>
        + Send
        + Sync,
{
    /// Create a new function-based runnable.
    pub fn new(func: F) -> Self {
        Self {
            func,
            name: "Function".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl<F> Runnable for RunnableFn<F>
where
    F: Fn(ChainValue) -> Pin<Box<dyn Future<Output = ChainResult<ChainValue>> + Send>>
        + Send
        + Sync,
{
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        (self.func)(input).await
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Helper macro to create a RunnableFn from an async closure.
#[macro_export]
macro_rules! runnable_fn {
    ($closure:expr) => {
        $crate::RunnableFn::new(move |input| Box::pin($closure(input)))
    };
}

/// A runnable that passes input through unchanged.
///
/// Useful for parallel compositions where you want to preserve the original input.
#[derive(Debug, Clone, Default)]
pub struct RunnablePassthrough {
    name: String,
}

impl RunnablePassthrough {
    /// Create a new passthrough runnable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: "Passthrough".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Runnable for RunnablePassthrough {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        Ok(input)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that always returns a constant value.
#[derive(Debug, Clone)]
pub struct RunnableConst {
    value: ChainValue,
    name: String,
}

impl RunnableConst {
    /// Create a runnable that always returns the given value.
    pub fn new(value: impl Into<ChainValue>) -> Self {
        Self {
            value: value.into(),
            name: "Const".to_string(),
        }
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Runnable for RunnableConst {
    async fn invoke(&self, _input: ChainValue) -> ChainResult<ChainValue> {
        Ok(self.value.clone())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that extracts a field from an object input.
#[derive(Debug, Clone)]
pub struct RunnablePick {
    key: String,
    name: String,
}

impl RunnablePick {
    /// Create a runnable that extracts the given key from input.
    pub fn new(key: impl Into<String>) -> Self {
        let key = key.into();
        Self {
            name: format!("Pick({key})"),
            key,
        }
    }
}

#[async_trait]
impl Runnable for RunnablePick {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        match input.get(&self.key) {
            Some(value) => Ok(value.clone()),
            None => Ok(ChainValue::Null),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that assigns additional fields to the input object.
///
/// Takes an object input and adds the outputs of nested runnables as new fields.
pub struct RunnableAssign {
    assignments: Vec<(String, Arc<dyn Runnable>)>,
    name: String,
}

impl RunnableAssign {
    /// Create a new assign runnable.
    pub fn new() -> Self {
        Self {
            assignments: Vec::new(),
            name: "Assign".to_string(),
        }
    }

    /// Add an assignment.
    pub fn assign<R: Runnable + 'static>(mut self, key: impl Into<String>, runnable: R) -> Self {
        self.assignments.push((key.into(), Arc::new(runnable)));
        self
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for RunnableAssign {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Runnable for RunnableAssign {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        use futures::future::join_all;

        // Start with the input object or create a new one
        let mut output = match input.clone() {
            ChainValue::Object(obj) => obj,
            _ => std::collections::HashMap::new(),
        };

        // Run all assignments in parallel
        let futures: Vec<_> = self
            .assignments
            .iter()
            .map(|(key, runnable)| {
                let key = key.clone();
                let input = input.clone();
                let runnable = Arc::clone(runnable);
                async move { (key, runnable.invoke(input).await) }
            })
            .collect();

        let results = join_all(futures).await;

        for (key, result) in results {
            output.insert(key, result?);
        }

        Ok(ChainValue::Object(output))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Conditional branch based on a predicate.
pub struct RunnableBranch {
    branches: Vec<(
        Arc<dyn Fn(&ChainValue) -> bool + Send + Sync>,
        Arc<dyn Runnable>,
    )>,
    default: Option<Arc<dyn Runnable>>,
    name: String,
}

impl RunnableBranch {
    /// Create a new branch runnable.
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            default: None,
            name: "Branch".to_string(),
        }
    }

    /// Add a conditional branch.
    pub fn when<P, R>(mut self, predicate: P, runnable: R) -> Self
    where
        P: Fn(&ChainValue) -> bool + Send + Sync + 'static,
        R: Runnable + 'static,
    {
        self.branches
            .push((Arc::new(predicate), Arc::new(runnable)));
        self
    }

    /// Set the default branch when no conditions match.
    pub fn otherwise<R: Runnable + 'static>(mut self, runnable: R) -> Self {
        self.default = Some(Arc::new(runnable));
        self
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for RunnableBranch {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Runnable for RunnableBranch {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        for (predicate, runnable) in &self.branches {
            if predicate(&input) {
                return runnable.invoke(input).await;
            }
        }

        match &self.default {
            Some(default) => default.invoke(input).await,
            None => Err(ChainError::NoBranchMatch),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A runnable that retries on failure.
pub struct RunnableRetry {
    inner: Arc<dyn Runnable>,
    max_attempts: u32,
    delay_ms: u64,
    name: String,
}

impl RunnableRetry {
    /// Create a retry wrapper around a runnable.
    pub fn new<R: Runnable + 'static>(runnable: R, max_attempts: u32) -> Self {
        Self {
            inner: Arc::new(runnable),
            max_attempts,
            delay_ms: 1000,
            name: "Retry".to_string(),
        }
    }

    /// Set the delay between retries in milliseconds.
    #[must_use]
    pub fn delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Set a custom name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Runnable for RunnableRetry {
    async fn invoke(&self, input: ChainValue) -> ChainResult<ChainValue> {
        let mut last_error = String::new();

        for attempt in 1..=self.max_attempts {
            match self.inner.invoke(input.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        runnable = self.inner.name(),
                        attempt,
                        max_attempts = self.max_attempts,
                        error = %e,
                        "Retry attempt failed"
                    );

                    if attempt < self.max_attempts {
                        tokio::time::sleep(tokio::time::Duration::from_millis(self.delay_ms)).await;
                    }
                }
            }
        }

        Err(ChainError::RetryExhausted {
            attempts: self.max_attempts,
            last_error,
        })
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// A boxed runnable for type erasure.
pub type BoxedRunnable = Pin<Box<dyn Runnable>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_passthrough() {
        let passthrough = RunnablePassthrough::new();
        let input = ChainValue::from("hello");
        let result = passthrough.invoke(input.clone()).await.unwrap();
        assert_eq!(result.as_str(), input.as_str());
    }

    #[tokio::test]
    async fn test_const() {
        let constant = RunnableConst::new("always this");
        let result = constant.invoke(ChainValue::from("ignored")).await.unwrap();
        assert_eq!(result.as_str(), Some("always this"));
    }

    #[tokio::test]
    async fn test_pick() {
        let pick = RunnablePick::new("name");
        let input = ChainValue::Object(
            [("name".to_string(), ChainValue::from("Alice"))]
                .into_iter()
                .collect(),
        );
        let result = pick.invoke(input).await.unwrap();
        assert_eq!(result.as_str(), Some("Alice"));
    }

    #[tokio::test]
    async fn test_pick_missing() {
        let pick = RunnablePick::new("missing");
        let input = ChainValue::Object(std::collections::HashMap::new());
        let result = pick.invoke(input).await.unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn test_branch() {
        let branch = RunnableBranch::new()
            .when(
                |v| v.as_int().map(|i| i > 0).unwrap_or(false),
                RunnableConst::new("positive"),
            )
            .when(
                |v| v.as_int().map(|i| i < 0).unwrap_or(false),
                RunnableConst::new("negative"),
            )
            .otherwise(RunnableConst::new("zero"));

        let result = branch.invoke(ChainValue::from(5)).await.unwrap();
        assert_eq!(result.as_str(), Some("positive"));

        let result = branch.invoke(ChainValue::from(-3)).await.unwrap();
        assert_eq!(result.as_str(), Some("negative"));

        let result = branch.invoke(ChainValue::from(0)).await.unwrap();
        assert_eq!(result.as_str(), Some("zero"));
    }

    #[tokio::test]
    async fn test_assign() {
        let assign = RunnableAssign::new()
            .assign("original", RunnablePassthrough::new())
            .assign("constant", RunnableConst::new("added"));

        let input = ChainValue::from("input value");
        let result = assign.invoke(input).await.unwrap();

        let obj = result.as_object().unwrap();
        assert_eq!(
            obj.get("original").and_then(|v| v.as_str()),
            Some("input value")
        );
        assert_eq!(obj.get("constant").and_then(|v| v.as_str()), Some("added"));
    }

    #[tokio::test]
    async fn test_function_runnable() {
        let uppercase = RunnableFn::new(|input: ChainValue| {
            Box::pin(async move {
                Ok(ChainValue::from(input.to_string_value().to_uppercase()))
            })
        });

        let result = uppercase.invoke(ChainValue::from("hello")).await.unwrap();
        assert_eq!(result.as_str(), Some("HELLO"));
    }

    #[tokio::test]
    async fn test_function_runnable_macro() {
        let uppercase = runnable_fn!(|input: ChainValue| async move {
            Ok(ChainValue::from(input.to_string_value().to_uppercase()))
        });

        let result = uppercase.invoke(ChainValue::from("world")).await.unwrap();
        assert_eq!(result.as_str(), Some("WORLD"));
    }
}
