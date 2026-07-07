//! The JSON object map alias used by spec fields typed `{ [key: string]: unknown }`.

use serde_json::{Map, Value};

/// A JSON object map.
///
/// The MCP 2025-11-25 schema types several fields as JSON **objects**
/// (`{ [key: string]: unknown }`) rather than arbitrary JSON values —
/// e.g. `tools/call` `arguments`, `structuredContent`, and `tool_use`
/// `input`. Modeling them as `Object` enforces object-ness at the type
/// level instead of at runtime.
pub type Object = Map<String, Value>;
