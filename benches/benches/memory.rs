//! Memory allocation benchmarks for long-running servers.
//!
//! Run with: `cargo bench --package rust-mcp-benches --bench memory`
//!
//! This benchmark measures memory allocation patterns and helps identify
//! potential memory leaks in long-running server scenarios.

// Allow missing docs for criterion_group! macro generated functions
#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mcpkit_core::{
    protocol::{Request, Response, RequestId},
    types::{CallToolResult, Tool, ToolAnnotations},
};
use serde_json::json;
use std::collections::HashMap;

/// Simulate request/response lifecycle
fn request_response_cycle(id: u64) -> Response {
    // Create request
    let request = Request::with_params(
        "tools/call",
        RequestId::Number(id),
        json!({
            "name": "process_data",
            "arguments": {
                "data": format!("sample data for request {}", id),
                "options": {
                    "validate": true,
                    "transform": false
                }
            }
        }),
    );

    // Serialize request (as would happen in transport)
    let serialized = serde_json::to_string(&request).unwrap();

    // Deserialize (as would happen on receiving end)
    let _: Request = serde_json::from_str(&serialized).unwrap();

    // Create response
    Response::success(
        RequestId::Number(id),
        json!({
            "content": [{
                "type": "text",
                "text": format!("Processed request {} successfully", id)
            }],
            "isError": false
        }),
    )
}

/// Simulate tool handler that allocates and releases memory
fn simulate_tool_handler(iterations: u64) -> Vec<CallToolResult> {
    let mut results = Vec::with_capacity(iterations as usize);

    for i in 0..iterations {
        let result = CallToolResult::text(format!(
            "Result for iteration {} with some additional content to make it more realistic in size",
            i
        ));
        results.push(result);
    }

    results
}

/// Simulate maintaining tool registry
fn simulate_tool_registry(num_tools: usize) -> HashMap<String, Tool> {
    let mut registry = HashMap::with_capacity(num_tools);

    for i in 0..num_tools {
        let tool = Tool::new(format!("tool_{}", i))
            .description(format!("A test tool number {}", i))
            .input_schema(json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"},
                    "count": {"type": "number"}
                },
                "required": ["input"]
            }))
            .annotations(ToolAnnotations::read_only());

        registry.insert(format!("tool_{}", i), tool);
    }

    registry
}

/// Benchmark memory allocation patterns during request processing
fn bench_request_processing_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_request_processing");

    // Single request cycle
    group.bench_function("single_cycle", |b| {
        let mut id = 0u64;
        b.iter(|| {
            id += 1;
            black_box(request_response_cycle(id))
        });
    });

    // Batch request processing
    for batch_size in [10u64, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size));

        group.bench_with_input(
            BenchmarkId::new("batch_cycle", batch_size),
            &batch_size,
            |b, &size| {
                let mut base_id = 0u64;
                b.iter(|| {
                    let responses: Vec<_> = (0..size)
                        .map(|i| {
                            base_id += 1;
                            request_response_cycle(base_id + i)
                        })
                        .collect();
                    black_box(responses)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark tool result creation and collection
fn bench_tool_results_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_tool_results");

    for count in [10u64, 100, 1000, 10000] {
        group.throughput(Throughput::Elements(count));

        group.bench_with_input(
            BenchmarkId::new("create_results", count),
            &count,
            |b, &count| {
                b.iter(|| black_box(simulate_tool_handler(count)));
            },
        );
    }

    group.finish();
}

/// Benchmark tool registry operations
fn bench_tool_registry_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_tool_registry");

    // Building the registry
    for num_tools in [10usize, 50, 100, 500] {
        group.bench_with_input(
            BenchmarkId::new("build_registry", num_tools),
            &num_tools,
            |b, &count| {
                b.iter(|| black_box(simulate_tool_registry(count)));
            },
        );
    }

    // Looking up tools in registry
    let registry = simulate_tool_registry(100);
    group.bench_function("lookup_existing", |b| {
        b.iter(|| black_box(registry.get("tool_50")));
    });

    group.bench_function("lookup_missing", |b| {
        b.iter(|| black_box(registry.get("nonexistent_tool")));
    });

    group.finish();
}

/// Benchmark JSON serialization memory impact
fn bench_json_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_json");

    // Create increasingly large JSON structures
    let small_json = json!({"key": "value"});
    let medium_json = json!({
        "data": (0..100).map(|i| json!({"id": i, "value": format!("item_{}", i)})).collect::<Vec<_>>()
    });
    let large_json = json!({
        "data": (0..1000).map(|i| json!({
            "id": i,
            "value": format!("item_{}", i),
            "nested": {"a": 1, "b": 2, "c": 3}
        })).collect::<Vec<_>>()
    });

    group.bench_function("small_clone", |b| {
        b.iter(|| black_box(small_json.clone()));
    });

    group.bench_function("medium_clone", |b| {
        b.iter(|| black_box(medium_json.clone()));
    });

    group.bench_function("large_clone", |b| {
        b.iter(|| black_box(large_json.clone()));
    });

    // Serialization and deserialization round-trip
    group.bench_function("small_roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&small_json)).unwrap();
            let _: serde_json::Value = serde_json::from_str(&s).unwrap();
        });
    });

    group.bench_function("medium_roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&medium_json)).unwrap();
            let _: serde_json::Value = serde_json::from_str(&s).unwrap();
        });
    });

    group.bench_function("large_roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&large_json)).unwrap();
            let _: serde_json::Value = serde_json::from_str(&s).unwrap();
        });
    });

    group.finish();
}

/// Benchmark string allocation patterns
fn bench_string_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_strings");

    // Different string creation patterns
    group.bench_function("format_short", |b| {
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            black_box(format!("msg_{}", i))
        });
    });

    group.bench_function("format_long", |b| {
        let mut i = 0u64;
        b.iter(|| {
            i += 1;
            black_box(format!(
                "This is a longer message with more content: {} and some padding to make it realistic",
                i
            ))
        });
    });

    // String concatenation
    group.bench_function("concat_push", |b| {
        b.iter(|| {
            let mut s = String::with_capacity(100);
            for i in 0..10 {
                s.push_str(&format!("part_{}", i));
            }
            black_box(s)
        });
    });

    group.bench_function("concat_collect", |b| {
        b.iter(|| {
            let s: String = (0..10).map(|i| format!("part_{}", i)).collect();
            black_box(s)
        });
    });

    group.finish();
}

/// Benchmark vector allocation and reuse patterns
fn bench_vec_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_vectors");

    // Growing vectors
    for size in [100usize, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("grow_no_capacity", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut v = Vec::new();
                    for i in 0..size {
                        v.push(i);
                    }
                    black_box(v)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("grow_with_capacity", size),
            &size,
            |b, &size| {
                b.iter(|| {
                    let mut v = Vec::with_capacity(size);
                    for i in 0..size {
                        v.push(i);
                    }
                    black_box(v)
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("collect", size), &size, |b, &size| {
            b.iter(|| {
                let v: Vec<usize> = (0..size).collect();
                black_box(v)
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_request_processing_memory,
    bench_tool_results_memory,
    bench_tool_registry_memory,
    bench_json_memory,
    bench_string_allocation,
    bench_vec_allocation,
);

criterion_main!(benches);
