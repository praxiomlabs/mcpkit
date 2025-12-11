//! Compile-fail tests for MCP macros.
//!
//! These tests verify that the macros produce helpful error messages
//! when used incorrectly.

#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/*.rs");
}
