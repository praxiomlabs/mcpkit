//! Build script for mcpkit-transport.
//!
//! This script compiles protobuf definitions when the `grpc` feature is enabled.
//! It uses protobuf-src to compile protoc from source if it's not available in the system.

fn main() {
    #[cfg(feature = "grpc")]
    {
        compile_protos();
    }
}

#[cfg(feature = "grpc")]
fn compile_protos() {
    use std::path::PathBuf;

    let proto_file = "proto/mcp.proto";
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Tell cargo to rerun this script if the proto file changes
    println!("cargo:rerun-if-changed={proto_file}");

    // Configure prost-build with protoc from protobuf-src
    let mut prost_config = prost_build::Config::new();
    prost_config.protoc_executable(protobuf_src::protoc());

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
}
