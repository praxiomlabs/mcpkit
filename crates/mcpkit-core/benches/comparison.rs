#![allow(missing_docs)]
//! Comparison benchmarks between rust-mcp-sdk and rmcp.
//!
//! Run with: `cargo bench --bench comparison`
//!
//! These benchmarks compare protocol operations between this SDK and the
//! official rmcp SDK to track relative performance.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::{json, Value};

// Our SDK types
use mcpkit_core::protocol::{Message as OurMessage, Request as OurRequest};
use mcpkit_core::types::Tool as OurTool;

// rmcp SDK types (from the official MCP Rust SDK)
use rmcp::model::CallToolRequestParam;

/// Benchmark JSON-RPC request serialization: our SDK vs rmcp
fn bench_request_serialization_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_serialization_comparison");

    // Our SDK request
    let our_request = OurRequest::with_params("tools/list", 1u64, json!({}));

    group.bench_function("mcpkit_sdk", |b| {
        b.iter(|| serde_json::to_string(black_box(&our_request)).unwrap());
    });

    // rmcp uses its own JsonRpc types, benchmark raw JSON serialization as comparable baseline
    let rmcp_json = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    group.bench_function("rmcp_json_baseline", |b| {
        b.iter(|| serde_json::to_string(black_box(&rmcp_json)).unwrap());
    });

    group.finish();
}

/// Benchmark JSON-RPC request deserialization: our SDK vs rmcp
fn bench_request_deserialization_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_deserialization_comparison");

    let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;

    // Our SDK deserialization
    group.bench_function("mcpkit_sdk", |b| {
        b.iter(|| serde_json::from_str::<OurRequest>(black_box(json_str)).unwrap());
    });

    // rmcp JSON baseline
    group.bench_function("rmcp_json_baseline", |b| {
        b.iter(|| serde_json::from_str::<Value>(black_box(json_str)).unwrap());
    });

    group.finish();
}

/// Benchmark Message enum parsing: our SDK vs rmcp
fn bench_message_parsing_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_parsing_comparison");

    let request_json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let response_json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
    let notification_json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

    // Our SDK request parsing
    group.bench_function("mcpkit_sdk_request", |b| {
        b.iter(|| serde_json::from_str::<OurMessage>(black_box(request_json)).unwrap());
    });

    group.bench_function("mcpkit_sdk_response", |b| {
        b.iter(|| serde_json::from_str::<OurMessage>(black_box(response_json)).unwrap());
    });

    group.bench_function("mcpkit_sdk_notification", |b| {
        b.iter(|| serde_json::from_str::<OurMessage>(black_box(notification_json)).unwrap());
    });

    // rmcp JSON baseline
    group.bench_function("rmcp_json_baseline_request", |b| {
        b.iter(|| serde_json::from_str::<Value>(black_box(request_json)).unwrap());
    });

    group.bench_function("rmcp_json_baseline_response", |b| {
        b.iter(|| serde_json::from_str::<Value>(black_box(response_json)).unwrap());
    });

    group.bench_function("rmcp_json_baseline_notification", |b| {
        b.iter(|| serde_json::from_str::<Value>(black_box(notification_json)).unwrap());
    });

    group.finish();
}

/// Benchmark Tool type operations: our SDK vs rmcp
fn bench_tool_operations_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_operations_comparison");

    // Our SDK tool creation
    group.bench_function("mcpkit_sdk_tool_creation", |b| {
        b.iter(|| {
            OurTool::new(black_box("search"))
                .description("Search for items")
                .input_schema(json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }))
        });
    });

    // rmcp tool creation (if available - using JSON baseline)
    let tool_json = json!({
        "name": "search",
        "description": "Search for items",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        }
    });

    group.bench_function("rmcp_json_baseline_tool", |b| {
        b.iter(|| serde_json::to_string(black_box(&tool_json)).unwrap());
    });

    group.finish();
}

/// Benchmark varying payload sizes: our SDK vs rmcp
fn bench_payload_sizes_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("payload_sizes_comparison");

    for size in &[100, 1000, 10000] {
        let payload: String = (0..*size).map(|_| 'x').collect();

        // Our SDK
        let our_request = OurRequest::with_params("tools/call", 1u64, json!({"data": payload}));

        group.bench_with_input(
            BenchmarkId::new("mcpkit_sdk_serialize", size),
            &our_request,
            |b, req| b.iter(|| serde_json::to_string(black_box(req)).unwrap()),
        );

        // rmcp JSON baseline
        let rmcp_json = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"data": payload}
        });

        group.bench_with_input(
            BenchmarkId::new("rmcp_json_baseline_serialize", size),
            &rmcp_json,
            |b, json_val| b.iter(|| serde_json::to_string(black_box(json_val)).unwrap()),
        );
    }

    group.finish();
}

/// Benchmark `CallToolRequestParam`: our SDK representation vs rmcp
fn bench_tool_call_params_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_call_params_comparison");

    // Complex tool call parameters
    let args = json!({
        "query": "rust mcp sdk",
        "filters": {
            "language": "rust",
            "category": "sdk",
            "tags": ["mcp", "protocol", "ai"]
        },
        "pagination": {
            "offset": 0,
            "limit": 100
        }
    });

    // Our SDK approach: inline in request params
    let our_params = json!({
        "name": "search",
        "arguments": args
    });

    group.bench_function("mcpkit_sdk_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&our_params)).unwrap());
    });

    // rmcp uses CallToolRequestParam
    let rmcp_params = CallToolRequestParam {
        name: "search".into(),
        arguments: Some(args.as_object().unwrap().clone()),
    };

    group.bench_function("rmcp_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&rmcp_params)).unwrap());
    });

    // Deserialization
    let param_json = serde_json::to_string(&our_params).unwrap();

    group.bench_function("mcpkit_sdk_deserialize", |b| {
        b.iter(|| serde_json::from_str::<Value>(black_box(&param_json)).unwrap());
    });

    let rmcp_json = serde_json::to_string(&rmcp_params).unwrap();
    group.bench_function("rmcp_deserialize", |b| {
        b.iter(|| serde_json::from_str::<CallToolRequestParam>(black_box(&rmcp_json)).unwrap());
    });

    group.finish();
}

criterion_group!(
    comparison_benches,
    bench_request_serialization_comparison,
    bench_request_deserialization_comparison,
    bench_message_parsing_comparison,
    bench_tool_operations_comparison,
    bench_payload_sizes_comparison,
    bench_tool_call_params_comparison,
);

criterion_main!(comparison_benches);
