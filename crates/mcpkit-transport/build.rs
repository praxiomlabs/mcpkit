//! Build script for mcpkit-transport.
//!
//! This script compiles protobuf definitions when the `regenerate-proto` feature is enabled.
//! The generated code is checked into the repository at `src/grpc/mcp_proto.rs`, so
//! normal builds do not require protoc or protobuf-src.
//!
//! ## Why Pre-generated Code?
//!
//! The `protobuf-src` crate builds protobuf from source using CMake, which fails on
//! Windows due to abseil-cpp linker issues with UCRT math functions (ceilf, ldexp, etc.).
//! By pre-generating the code, we enable cross-platform builds without requiring protoc.
//!
//! ## Regenerating Proto Code
//!
//! When you modify `proto/mcp.proto`, regenerate the Rust code:
//!
//! ```bash
//! just generate-proto
//! ```
//!
//! Or manually:
//!
//! ```bash
//! cargo build -p mcpkit-transport --features regenerate-proto
//! # Then copy the generated file:
//! cp target/debug/build/mcpkit-transport-*/out/mcp.rs \
//!    crates/mcpkit-transport/src/grpc/mcp_proto.rs
//! ```
//!
//! Don't forget to add the header comment back to the file after copying.

fn main() {
    // Only compile protos when explicitly regenerating
    #[cfg(feature = "regenerate-proto")]
    {
        compile_protos();
    }
}

#[cfg(feature = "regenerate-proto")]
fn compile_protos() {
    use std::path::PathBuf;

    let proto_file = "proto/mcp.proto";
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Tell cargo to rerun this script if the proto file changes
    println!("cargo:rerun-if-changed={proto_file}");

    // Configure prost-build with protoc from protobuf-src
    let mut prost_config = prost_build::Config::new();
    prost_config.protoc_executable(protobuf_src::protoc());

    // Add clippy allows for generated code
    prost_config.type_attribute(".", "#[allow(clippy::derive_partial_eq_without_eq)]");

    // Configure tonic-build
    tonic_build::configure()
        // Generate server code
        .build_server(true)
        // Generate client code
        .build_client(true)
        // Enable documentation comments from proto files
        .emit_rerun_if_changed(true)
        // Output directory for generated files
        .out_dir(&out_dir)
        // Compile the proto file with custom prost config
        .compile_protos_with_config(prost_config, &[proto_file], &["proto"])
        .unwrap_or_else(|e| panic!("Failed to compile proto files: {e}"));

    println!("cargo:info=Generated gRPC code to {}", out_dir.display());
    println!(
        "cargo:warning=Proto code regenerated. Copy {}/mcp.rs to src/grpc/mcp_proto.rs",
        out_dir.display()
    );
}
