//! Fuzz target for JSON-RPC Message parsing.
//!
//! This fuzzer tests the parsing of arbitrary bytes as JSON-RPC messages.
//! It should not panic or crash on any input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpkit_core::protocol::Message;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a JSON-RPC Message
        // This should never panic, only return Ok or Err
        let _ = serde_json::from_str::<Message>(s);
    }

    // Also try parsing directly from bytes
    let _ = serde_json::from_slice::<Message>(data);
});
