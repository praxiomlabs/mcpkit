//! Macro expansion tests.
//!
//! These tests verify that the macros expand to the expected code.
//! Run with `TRYBUILD=overwrite cargo test expand_tests` to update
//! expected outputs when making intentional changes.

#[test]
fn expand_tests() {
    let t = trybuild::TestCases::new();
    t.pass("tests/expand/*.rs");
}
