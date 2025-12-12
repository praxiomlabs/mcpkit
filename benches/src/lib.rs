//! Benchmarks for the Rust MCP SDK.
//!
//! This crate contains Criterion benchmarks for measuring performance of:
//!
//! - **Serialization**: JSON-RPC message serialization and deserialization throughput
//! - **Tool Invocation**: Tool lookup, argument parsing, and execution latency
//! - **Transport**: Channel throughput and message passing performance
//! - **Memory**: Memory allocation patterns for long-running server scenarios
//!
//! ## Running Benchmarks
//!
//! Run all benchmarks:
//! ```bash
//! cargo bench --package rust-mcp-benches
//! ```
//!
//! Run specific benchmark:
//! ```bash
//! cargo bench --package rust-mcp-benches --bench serialization
//! cargo bench --package rust-mcp-benches --bench tool_invocation
//! cargo bench --package rust-mcp-benches --bench transport
//! cargo bench --package rust-mcp-benches --bench memory
//! ```
//!
//! Run with fewer samples for quick validation:
//! ```bash
//! cargo bench --package rust-mcp-benches -- --sample-size 10
//! ```
//!
//! ## Benchmark Results
//!
//! Results are written to `target/criterion/` with HTML reports.
//! Open `target/criterion/report/index.html` for a summary.
//!
//! ## Benchmark Groups
//!
//! ### Serialization (`benches/serialization.rs`)
//! - `request_serialization`: Request serialization to string/bytes
//! - `request_deserialization`: Request parsing from JSON
//! - `response_serialization`: Response serialization
//! - `response_deserialization`: Response parsing
//! - `tool_serialization`: Tool and result serialization
//! - `roundtrip`: Full serialize/deserialize cycles
//!
//! ### Tool Invocation (`benches/tool_invocation.rs`)
//! - `tool_lookup`: Tool registry operations
//! - `arg_parsing`: JSON argument extraction
//! - `tool_call`: Direct tool execution
//! - `result_creation`: `ToolOutput` construction
//! - `full_invocation`: End-to-end tool calls
//!
//! ### Transport (`benches/transport.rs`)
//! - `channel_throughput`: mpsc channel performance
//! - `memory_transport`: In-memory transport operations
//! - `message_sizes`: Impact of message size
//! - `concurrent_access`: Multi-sender performance
//!
//! ### Memory (`benches/memory.rs`)
//! - `memory_request_processing`: Request/response lifecycle
//! - `memory_tool_results`: Result creation patterns
//! - `memory_tool_registry`: Registry building and lookup
//! - `memory_json`: JSON structure operations
//! - `memory_strings`: String allocation patterns
//! - `memory_vectors`: Vector allocation strategies

// This is a benchmark-only crate, no library code needed.
