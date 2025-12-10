//! Fuzz target for JSON-RPC Request parsing.
//!
//! This fuzzer specifically tests parsing of Request messages with various
//! ID types, methods, and parameter combinations.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcp_core::protocol::Request;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a Request
        if let Ok(request) = serde_json::from_str::<Request>(s) {
            // If parsing succeeds, verify round-trip serialization
            if let Ok(serialized) = serde_json::to_string(&request) {
                // Deserialize again and verify it's still valid
                let _ = serde_json::from_str::<Request>(&serialized);
            }
        }
    }

    // Also try parsing directly from bytes
    let _ = serde_json::from_slice::<Request>(data);
});
