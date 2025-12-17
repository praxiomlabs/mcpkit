//! Standard JSON-RPC and MCP error codes.
//!
//! This module defines error code constants used in JSON-RPC 2.0 responses
//! and MCP-specific error responses.

/// Invalid JSON was received.
pub const PARSE_ERROR: i32 = -32700;

/// The JSON sent is not a valid Request object.
pub const INVALID_REQUEST: i32 = -32600;

/// The method does not exist.
pub const METHOD_NOT_FOUND: i32 = -32601;

/// Invalid method parameters.
pub const INVALID_PARAMS: i32 = -32602;

/// Internal JSON-RPC error.
pub const INTERNAL_ERROR: i32 = -32603;

/// Server error range start.
pub const SERVER_ERROR_START: i32 = -32000;

/// Server error range end.
pub const SERVER_ERROR_END: i32 = -32099;

// MCP-specific codes

/// User rejected the operation.
pub const USER_REJECTED: i32 = -1;

/// Resource was not found.
pub const RESOURCE_NOT_FOUND: i32 = -32002;
