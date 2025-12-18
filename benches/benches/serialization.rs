//! Benchmarks for JSON-RPC message serialization and deserialization.
//!
//! Run with: `cargo bench --package mcpkit-benches --bench serialization`

// Allow missing docs for criterion_group! macro generated functions
#![allow(missing_docs)]

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use mcpkit_core::{
    protocol::{Request, RequestId, Response},
    types::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::{Value, json};

/// Create a minimal request for benchmarking
fn minimal_request() -> Request {
    Request::with_params("tools/call", RequestId::Number(1), json!({"name": "test"}))
}

/// Create a complex request with nested params
fn complex_request() -> Request {
    Request::with_params(
        "tools/call",
        RequestId::Number(12345),
        json!({
            "name": "search_database",
            "arguments": {
                "query": "SELECT * FROM users WHERE active = true AND created_at > '2024-01-01'",
                "database": "production",
                "options": {
                    "limit": 100,
                    "offset": 0,
                    "include_deleted": false,
                    "fields": ["id", "name", "email", "created_at", "updated_at"]
                }
            },
            "_meta": {
                "progressToken": "abc123"
            }
        }),
    )
}

/// Create a successful response
fn success_response() -> Response {
    Response::success(
        RequestId::Number(1),
        json!({
            "content": [
                {"type": "text", "text": "Hello, World!"}
            ],
            "isError": false
        }),
    )
}

/// Create a large response with multiple content items
fn large_response() -> Response {
    let content: Vec<Value> = (0..100)
        .map(|i| {
            json!({
                "type": "text",
                "text": format!("Result item {} with some additional content to make it more realistic in terms of size", i)
            })
        })
        .collect();

    Response::success(
        RequestId::Number(99999),
        json!({
            "content": content,
            "isError": false,
            "_meta": {
                "requestId": "req_abc123xyz",
                "processingTime": 1234
            }
        }),
    )
}

/// Create a tool definition
fn tool_definition() -> Tool {
    Tool {
        name: "search_database".to_string(),
        description: Some("Search the database with a SQL query".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The SQL query to execute"
                },
                "database": {
                    "type": "string",
                    "enum": ["production", "staging", "development"]
                },
                "options": {
                    "type": "object",
                    "properties": {
                        "limit": {"type": "integer", "minimum": 1, "maximum": 1000},
                        "offset": {"type": "integer", "minimum": 0}
                    }
                }
            },
            "required": ["query"]
        }),
        annotations: Some(ToolAnnotations {
            title: Some("Database Search".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: None,
        }),
    }
}

/// Create a `CallToolResult`
fn call_tool_result() -> CallToolResult {
    CallToolResult::text("Query executed successfully. Found 42 results.")
}

fn bench_request_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_serialization");

    let minimal = minimal_request();
    let complex = complex_request();

    group.throughput(Throughput::Elements(1));

    group.bench_with_input(
        BenchmarkId::new("minimal", "to_string"),
        &minimal,
        |b, req| {
            b.iter(|| serde_json::to_string(black_box(req)).unwrap());
        },
    );

    group.bench_with_input(
        BenchmarkId::new("complex", "to_string"),
        &complex,
        |b, req| {
            b.iter(|| serde_json::to_string(black_box(req)).unwrap());
        },
    );

    group.bench_with_input(BenchmarkId::new("minimal", "to_vec"), &minimal, |b, req| {
        b.iter(|| serde_json::to_vec(black_box(req)).unwrap());
    });

    group.bench_with_input(BenchmarkId::new("complex", "to_vec"), &complex, |b, req| {
        b.iter(|| serde_json::to_vec(black_box(req)).unwrap());
    });

    group.finish();
}

fn bench_request_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_deserialization");

    let minimal_json = serde_json::to_string(&minimal_request()).unwrap();
    let complex_json = serde_json::to_string(&complex_request()).unwrap();

    group.throughput(Throughput::Bytes(minimal_json.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("minimal", minimal_json.len()),
        &minimal_json,
        |b, json| {
            b.iter(|| serde_json::from_str::<Request>(black_box(json)).unwrap());
        },
    );

    group.throughput(Throughput::Bytes(complex_json.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("complex", complex_json.len()),
        &complex_json,
        |b, json| {
            b.iter(|| serde_json::from_str::<Request>(black_box(json)).unwrap());
        },
    );

    group.finish();
}

fn bench_response_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("response_serialization");

    let success = success_response();
    let large = large_response();

    group.throughput(Throughput::Elements(1));

    group.bench_with_input(
        BenchmarkId::new("success", "to_string"),
        &success,
        |b, resp| {
            b.iter(|| serde_json::to_string(black_box(resp)).unwrap());
        },
    );

    group.bench_with_input(BenchmarkId::new("large", "to_string"), &large, |b, resp| {
        b.iter(|| serde_json::to_string(black_box(resp)).unwrap());
    });

    group.finish();
}

fn bench_response_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("response_deserialization");

    let success_json = serde_json::to_string(&success_response()).unwrap();
    let large_json = serde_json::to_string(&large_response()).unwrap();

    group.bench_with_input(
        BenchmarkId::new("success", success_json.len()),
        &success_json,
        |b, json| {
            b.iter(|| serde_json::from_str::<Response>(black_box(json)).unwrap());
        },
    );

    group.bench_with_input(
        BenchmarkId::new("large", large_json.len()),
        &large_json,
        |b, json| {
            b.iter(|| serde_json::from_str::<Response>(black_box(json)).unwrap());
        },
    );

    group.finish();
}

fn bench_tool_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_serialization");

    let tool = tool_definition();
    let result = call_tool_result();

    group.bench_with_input(BenchmarkId::new("tool", "to_string"), &tool, |b, t| {
        b.iter(|| serde_json::to_string(black_box(t)).unwrap());
    });

    group.bench_with_input(BenchmarkId::new("result", "to_string"), &result, |b, r| {
        b.iter(|| serde_json::to_string(black_box(r)).unwrap());
    });

    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("roundtrip");

    let request = complex_request();
    let response = large_response();

    group.bench_function("request", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&request)).unwrap();
            serde_json::from_str::<Request>(&json).unwrap()
        });
    });

    group.bench_function("response", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&response)).unwrap();
            serde_json::from_str::<Response>(&json).unwrap()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_request_serialization,
    bench_request_deserialization,
    bench_response_serialization,
    bench_response_deserialization,
    bench_tool_serialization,
    bench_roundtrip,
);

criterion_main!(benches);
