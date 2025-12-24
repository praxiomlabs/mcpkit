//! Debug and tracing utilities for MCP protocol development.
//!
//! This module provides tools for debugging, inspecting, and recording
//! MCP protocol sessions. These are primarily intended for development
//! and testing, not production use.
//!
//! # Features
//!
//! - **Message Inspector**: Capture and analyze protocol messages
//! - **Session Recorder**: Record and replay MCP sessions
//! - **Protocol Validator**: Validate message sequences
//!
//! # Example
//!
//! ```rust
//! use mcpkit_core::debug::{MessageInspector, SessionRecorder};
//!
//! // Create a message inspector
//! let inspector = MessageInspector::new();
//!
//! // Create a session recorder
//! let recorder = SessionRecorder::new("debug-session");
//! ```

mod inspector;
mod recorder;
mod validator;

pub use inspector::{MessageInspector, MessageRecord, MessageStats};
pub use recorder::{RecordedSession, SessionEvent, SessionRecorder};
pub use validator::{
    ProtocolValidator, ValidationError, ValidationResult, validate_message_sequence,
};
