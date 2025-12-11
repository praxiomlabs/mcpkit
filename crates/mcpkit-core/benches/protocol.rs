//! Benchmarks for MCP protocol operations.
//!
//! Run with: `cargo bench --bench protocol`
//!
//! These benchmarks measure core protocol operations to track
//! performance and enable comparison with other implementations.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mcpkit_core::error::JsonRpcError;
use mcpkit_core::protocol::{Message, Request, RequestId, Response};
use mcpkit_core::types::{Content, Tool, ToolOutput};
use serde_json::json;

/// Benchmark JSON-RPC request serialization
fn bench_request_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_serialization");

    // Simple request
    let simple_request = Request::with_params("tools/list", 1u64, json!({}));

    group.throughput(Throughput::Elements(1));
    group.bench_function("simple_request", |b| {
        b.iter(|| serde_json::to_string(black_box(&simple_request)).unwrap())
    });

    // Complex request with nested params
    let complex_params = json!({
        "name": "search",
        "arguments": {
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
        }
    });
    let complex_request = Request::with_params("tools/call", 42u64, complex_params);

    group.bench_function("complex_request", |b| {
        b.iter(|| serde_json::to_string(black_box(&complex_request)).unwrap())
    });

    group.finish();
}

/// Benchmark JSON-RPC request deserialization
fn bench_request_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_deserialization");

    let simple_json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let complex_json = r#"{
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "search",
            "arguments": {
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
            }
        }
    }"#;

    group.throughput(Throughput::Bytes(simple_json.len() as u64));
    group.bench_function("simple_request", |b| {
        b.iter(|| serde_json::from_str::<Request>(black_box(simple_json)).unwrap())
    });

    group.throughput(Throughput::Bytes(complex_json.len() as u64));
    group.bench_function("complex_request", |b| {
        b.iter(|| serde_json::from_str::<Request>(black_box(complex_json)).unwrap())
    });

    group.finish();
}

/// Benchmark response serialization
fn bench_response_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("response_serialization");

    // Success response
    let success = Response::success(1u64, json!({"tools": []}));

    group.bench_function("success_response", |b| {
        b.iter(|| serde_json::to_string(black_box(&success)).unwrap())
    });

    // Error response
    let error = Response::error(
        1u64,
        JsonRpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: Some(json!({"method": "unknown"})),
        },
    );

    group.bench_function("error_response", |b| {
        b.iter(|| serde_json::to_string(black_box(&error)).unwrap())
    });

    // Large tool list response
    let tools: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            json!({
                "name": format!("tool_{}", i),
                "description": format!("This is tool number {}", i),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "input": {"type": "string"}
                    }
                }
            })
        })
        .collect();
    let large_response = Response::success(1u64, json!({"tools": tools}));

    group.bench_function("large_tool_list_response", |b| {
        b.iter(|| serde_json::to_string(black_box(&large_response)).unwrap())
    });

    group.finish();
}

/// Benchmark Message enum parsing (discriminating between request/response/notification)
fn bench_message_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_parsing");

    let request_json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let response_json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
    let notification_json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;

    group.bench_function("parse_request", |b| {
        b.iter(|| serde_json::from_str::<Message>(black_box(request_json)).unwrap())
    });

    group.bench_function("parse_response", |b| {
        b.iter(|| serde_json::from_str::<Message>(black_box(response_json)).unwrap())
    });

    group.bench_function("parse_notification", |b| {
        b.iter(|| serde_json::from_str::<Message>(black_box(notification_json)).unwrap())
    });

    group.finish();
}

/// Benchmark Tool type operations
fn bench_tool_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_operations");

    // Tool creation
    group.bench_function("tool_creation", |b| {
        b.iter(|| {
            Tool::new(black_box("search"))
                .description("Search for items")
                .input_schema(json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    },
                    "required": ["query"]
                }))
        })
    });

    // ToolOutput creation
    group.bench_function("tool_output_text", |b| {
        b.iter(|| ToolOutput::text(black_box("Hello, World!")))
    });

    group.bench_function("tool_output_json", |b| {
        let value = json!({
            "result": "success",
            "data": {
                "items": [1, 2, 3, 4, 5]
            }
        });
        b.iter(|| {
            ToolOutput::json(black_box(&value)).unwrap()
        })
    });

    group.finish();
}

/// Benchmark Content type operations
fn bench_content_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_operations");

    // Text content
    group.bench_function("text_content_creation", |b| {
        b.iter(|| Content::text(black_box("This is some text content")))
    });

    // Content serialization
    let text_content = Content::text("This is text content for serialization benchmark");
    group.bench_function("text_content_serialization", |b| {
        b.iter(|| serde_json::to_string(black_box(&text_content)).unwrap())
    });

    group.finish();
}

/// Benchmark varying payload sizes
fn bench_payload_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("payload_sizes");

    for size in [10, 100, 1000, 10000].iter() {
        let payload: String = (0..*size).map(|_| 'x').collect();
        let request = Request::with_params(
            "tools/call",
            1u64,
            json!({"data": payload}),
        );

        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("serialize", size), &request, |b, req| {
            b.iter(|| serde_json::to_string(black_box(req)).unwrap())
        });

        let json_str = serde_json::to_string(&request).unwrap();
        group.bench_with_input(
            BenchmarkId::new("deserialize", size),
            &json_str,
            |b, json| b.iter(|| serde_json::from_str::<Request>(black_box(json)).unwrap()),
        );
    }

    group.finish();
}

/// Benchmark RequestId operations
fn bench_request_id(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_id");

    group.bench_function("number_id_serialize", |b| {
        let id = RequestId::Number(12345);
        b.iter(|| serde_json::to_string(black_box(&id)).unwrap())
    });

    group.bench_function("string_id_serialize", |b| {
        let id = RequestId::String("request-uuid-12345".to_string());
        b.iter(|| serde_json::to_string(black_box(&id)).unwrap())
    });

    group.bench_function("number_id_deserialize", |b| {
        b.iter(|| serde_json::from_str::<RequestId>(black_box("12345")).unwrap())
    });

    group.bench_function("string_id_deserialize", |b| {
        b.iter(|| {
            serde_json::from_str::<RequestId>(black_box("\"request-uuid-12345\"")).unwrap()
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_request_serialization,
    bench_request_deserialization,
    bench_response_serialization,
    bench_message_parsing,
    bench_tool_operations,
    bench_content_operations,
    bench_payload_sizes,
    bench_request_id,
);

criterion_main!(benches);
