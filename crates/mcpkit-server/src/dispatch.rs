//! Object-safe dispatch layer.
//!
//! The public handler traits ([`ToolHandler`] et al.) return `impl Future`
//! (RPITIT) and so are not object-safe, which previously forced request routing
//! into a combinatorial per-handler-combination typestate macro. The `Dyn*`
//! traits here box the handler futures so a single router impl can dispatch any
//! registered handler, and the `*Slot` traits expose "the registered handler,
//! or `None`" uniformly across the typestate slots (`Registered<H>` /
//! `NotRegistered`).
//!
//! Adding a dispatched capability is then one `Dyn`/`Slot` pair plus one match
//! arm — no combinatorial growth.

use crate::builder::{NotRegistered, Registered};
use crate::context::Context;
use crate::handler::{PromptHandler, ResourceHandler, TaskHandler, ToolHandler};
use mcpkit_core::error::McpError;
use mcpkit_core::types::{
    GetPromptResult, Prompt, Resource, ResourceContents, ResourceTemplate, Task, TaskId, Tool,
    ToolOutput,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// A boxed, `Send` future borrowing for `'a`.
type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ============================================================================
// Object-safe handler traits (boxed futures), blanket-impl'd over the public
// RPITIT traits.
// ============================================================================

/// Object-safe form of [`ToolHandler`] (only the dispatched methods).
pub trait DynToolHandler: Send + Sync {
    /// See [`ToolHandler::list_tools`].
    fn list_tools<'a>(&'a self, ctx: &'a Context<'_>) -> BoxFut<'a, Result<Vec<Tool>, McpError>>;
    /// See [`ToolHandler::call_tool`].
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        args: Value,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<ToolOutput, McpError>>;
}

impl<T: ToolHandler> DynToolHandler for T {
    fn list_tools<'a>(&'a self, ctx: &'a Context<'_>) -> BoxFut<'a, Result<Vec<Tool>, McpError>> {
        Box::pin(ToolHandler::list_tools(self, ctx))
    }
    fn call_tool<'a>(
        &'a self,
        name: &'a str,
        args: Value,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<ToolOutput, McpError>> {
        Box::pin(ToolHandler::call_tool(self, name, args, ctx))
    }
}

/// Object-safe form of [`ResourceHandler`] (only the dispatched methods).
pub trait DynResourceHandler: Send + Sync {
    /// See [`ResourceHandler::list_resources`].
    fn list_resources<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<Resource>, McpError>>;
    /// See [`ResourceHandler::list_resource_templates`].
    fn list_resource_templates<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<ResourceTemplate>, McpError>>;
    /// See [`ResourceHandler::read_resource`].
    fn read_resource<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<ResourceContents>, McpError>>;
    /// See [`ResourceHandler::subscribe`].
    fn subscribe<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>>;
    /// See [`ResourceHandler::unsubscribe`].
    fn unsubscribe<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>>;
}

impl<R: ResourceHandler> DynResourceHandler for R {
    fn list_resources<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<Resource>, McpError>> {
        Box::pin(ResourceHandler::list_resources(self, ctx))
    }
    fn list_resource_templates<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<ResourceTemplate>, McpError>> {
        Box::pin(ResourceHandler::list_resource_templates(self, ctx))
    }
    fn read_resource<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<ResourceContents>, McpError>> {
        Box::pin(ResourceHandler::read_resource(self, uri, ctx))
    }
    fn subscribe<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>> {
        Box::pin(ResourceHandler::subscribe(self, uri, ctx))
    }
    fn unsubscribe<'a>(
        &'a self,
        uri: &'a str,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>> {
        Box::pin(ResourceHandler::unsubscribe(self, uri, ctx))
    }
}

/// Object-safe form of [`PromptHandler`] (only the dispatched methods).
pub trait DynPromptHandler: Send + Sync {
    /// See [`PromptHandler::list_prompts`].
    fn list_prompts<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<Prompt>, McpError>>;
    /// See [`PromptHandler::get_prompt`].
    fn get_prompt<'a>(
        &'a self,
        name: &'a str,
        args: Option<serde_json::Map<String, Value>>,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<GetPromptResult, McpError>>;
}

impl<P: PromptHandler> DynPromptHandler for P {
    fn list_prompts<'a>(
        &'a self,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Vec<Prompt>, McpError>> {
        Box::pin(PromptHandler::list_prompts(self, ctx))
    }
    fn get_prompt<'a>(
        &'a self,
        name: &'a str,
        args: Option<serde_json::Map<String, Value>>,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<GetPromptResult, McpError>> {
        Box::pin(PromptHandler::get_prompt(self, name, args, ctx))
    }
}

/// Object-safe form of [`TaskHandler`].
pub trait DynTaskHandler: Send + Sync {
    /// See [`TaskHandler::list_tasks`].
    fn list_tasks<'a>(&'a self, ctx: &'a Context<'_>) -> BoxFut<'a, Result<Vec<Task>, McpError>>;
    /// See [`TaskHandler::get_task`].
    fn get_task<'a>(
        &'a self,
        id: &'a TaskId,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Option<Task>, McpError>>;
    /// See [`TaskHandler::cancel_task`].
    fn cancel_task<'a>(
        &'a self,
        id: &'a TaskId,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>>;
}

impl<K: TaskHandler> DynTaskHandler for K {
    fn list_tasks<'a>(&'a self, ctx: &'a Context<'_>) -> BoxFut<'a, Result<Vec<Task>, McpError>> {
        Box::pin(TaskHandler::list_tasks(self, ctx))
    }
    fn get_task<'a>(
        &'a self,
        id: &'a TaskId,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<Option<Task>, McpError>> {
        Box::pin(TaskHandler::get_task(self, id, ctx))
    }
    fn cancel_task<'a>(
        &'a self,
        id: &'a TaskId,
        ctx: &'a Context<'_>,
    ) -> BoxFut<'a, Result<bool, McpError>> {
        Box::pin(TaskHandler::cancel_task(self, id, ctx))
    }
}

// ============================================================================
// Typestate slots: expose `Option<&dyn Dyn*Handler>` for both `Registered<H>`
// and `NotRegistered`, so one router impl can be generic over the slots.
// ============================================================================

/// A tool-handler slot.
pub trait ToolSlot: Send + Sync {
    /// The registered tool handler, or `None`.
    fn as_tool_handler(&self) -> Option<&dyn DynToolHandler>;
}
impl ToolSlot for NotRegistered {
    fn as_tool_handler(&self) -> Option<&dyn DynToolHandler> {
        None
    }
}
impl<T: ToolHandler> ToolSlot for Registered<T> {
    fn as_tool_handler(&self) -> Option<&dyn DynToolHandler> {
        Some(&self.0)
    }
}

/// A resource-handler slot.
pub trait ResourceSlot: Send + Sync {
    /// The registered resource handler, or `None`.
    fn as_resource_handler(&self) -> Option<&dyn DynResourceHandler>;
}
impl ResourceSlot for NotRegistered {
    fn as_resource_handler(&self) -> Option<&dyn DynResourceHandler> {
        None
    }
}
impl<R: ResourceHandler> ResourceSlot for Registered<R> {
    fn as_resource_handler(&self) -> Option<&dyn DynResourceHandler> {
        Some(&self.0)
    }
}

/// A prompt-handler slot.
pub trait PromptSlot: Send + Sync {
    /// The registered prompt handler, or `None`.
    fn as_prompt_handler(&self) -> Option<&dyn DynPromptHandler>;
}
impl PromptSlot for NotRegistered {
    fn as_prompt_handler(&self) -> Option<&dyn DynPromptHandler> {
        None
    }
}
impl<P: PromptHandler> PromptSlot for Registered<P> {
    fn as_prompt_handler(&self) -> Option<&dyn DynPromptHandler> {
        Some(&self.0)
    }
}

/// A task-handler slot.
pub trait TaskSlot: Send + Sync {
    /// The registered task handler, or `None`.
    fn as_task_handler(&self) -> Option<&dyn DynTaskHandler>;
}
impl TaskSlot for NotRegistered {
    fn as_task_handler(&self) -> Option<&dyn DynTaskHandler> {
        None
    }
}
impl<K: TaskHandler> TaskSlot for Registered<K> {
    fn as_task_handler(&self) -> Option<&dyn DynTaskHandler> {
        Some(&self.0)
    }
}
