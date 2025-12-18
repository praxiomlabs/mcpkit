//! Fuzz target for ProtocolVersion parsing.
//!
//! This fuzzer tests the parsing of arbitrary strings as MCP protocol versions.
//! It ensures version negotiation never panics on unexpected input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use mcpkit_core::protocol_version::ProtocolVersion;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    // Try to parse as UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        // Attempt to parse as a ProtocolVersion
        // This should never panic, only return Ok or Err
        let _ = ProtocolVersion::from_str(s);

        // Test version negotiation with arbitrary version strings
        // This should gracefully handle unknown versions
        let _ = ProtocolVersion::negotiate(s, ProtocolVersion::ALL);

        // Test compatibility checks with parsed versions
        if let Ok(version) = ProtocolVersion::from_str(s) {
            // All versions should be compatible with themselves
            assert!(version.is_compatible_with(version));

            // Test feature detection methods - none should panic
            let _ = version.supports_oauth();
            let _ = version.supports_elicitation();
            let _ = version.supports_tasks();
            let _ = version.supports_parallel_tools();
            let _ = version.supports_streamable_http();
            let _ = version.supports_batching();
            let _ = version.supports_tool_annotations();
            let _ = version.supports_structured_tool_output();
            let _ = version.supports_resource_links();
            let _ = version.supports_protected_resources();
            let _ = version.supports_agent_loops();
            let _ = version.supports_sampling_tools();
            let _ = version.supports_meta_field();
            let _ = version.supports_title_field();
            let _ = version.supports_completion_context();
            let _ = version.supports_completions_capability();
            let _ = version.supports_audio_content();
            let _ = version.supports_sse_transport();
            let _ = version.requires_version_header();
        }
    }

    // Also try deserializing from JSON
    if let Ok(s) = std::str::from_utf8(data) {
        // Wrap in quotes to make it a valid JSON string
        let json_str = format!("\"{s}\"");
        let _ = serde_json::from_str::<ProtocolVersion>(&json_str);
    }
});
