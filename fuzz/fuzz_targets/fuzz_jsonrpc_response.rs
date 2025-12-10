//! Fuzz target for JSON-RPC Response parsing.
//!
//! This fuzzer tests parsing of Response messages with various combinations
//! of success results and error responses.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcp_core::protocol::Response;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a Response
        if let Ok(response) = serde_json::from_str::<Response>(s) {
            // Test the response methods
            let _ = response.is_success();
            let _ = response.is_error();

            // Verify round-trip serialization
            if let Ok(serialized) = serde_json::to_string(&response) {
                let _ = serde_json::from_str::<Response>(&serialized);
            }
        }
    }

    // Also try parsing directly from bytes
    let _ = serde_json::from_slice::<Response>(data);
});
