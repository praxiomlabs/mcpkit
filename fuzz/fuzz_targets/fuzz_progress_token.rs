//! Fuzz target for ProgressToken parsing.
//!
//! This fuzzer tests parsing of progress tokens which can be either
//! numeric (u64) or string values.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpkit_core::protocol::ProgressToken;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a ProgressToken
        if let Ok(token) = serde_json::from_str::<ProgressToken>(s) {
            // Test display implementation
            let _ = token.to_string();

            // Verify round-trip serialization
            if let Ok(serialized) = serde_json::to_string(&token) {
                let _ = serde_json::from_str::<ProgressToken>(&serialized);
            }
        }
    }

    // Also try parsing directly from bytes
    let _ = serde_json::from_slice::<ProgressToken>(data);
});
