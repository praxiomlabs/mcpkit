//! Benchmarks for tool invocation latency.
//!
//! Run with: `cargo bench --package rust-mcp-benches --bench tool_invocation`

// Allow missing docs for criterion_group! macro generated functions
#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mcpkit_core::types::{CallToolResult, Tool, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Simulated tool handler
struct ToolHandler {
    tools: HashMap<String, Tool>,
}

impl ToolHandler {
    fn new() -> Self {
        let mut tools = HashMap::new();

        // Add some tools
        tools.insert(
            "echo".to_string(),
            Tool {
                name: "echo".to_string(),
                description: Some("Echo back the input".to_string()),
                input_schema: json!({"type": "object", "properties": {"message": {"type": "string"}}}),
                annotations: None,
            },
        );

        tools.insert(
            "calculate".to_string(),
            Tool {
                name: "calculate".to_string(),
                description: Some("Perform arithmetic".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "a": {"type": "number"},
                        "b": {"type": "number"},
                        "op": {"type": "string", "enum": ["add", "sub", "mul", "div"]}
                    },
                    "required": ["a", "b", "op"]
                }),
                annotations: None,
            },
        );

        Self { tools }
    }

    fn list_tools(&self) -> Vec<&Tool> {
        self.tools.values().collect()
    }

    fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput, String> {
        match name {
            "echo" => {
                let message = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no message)");
                Ok(ToolOutput::text(message.to_string()))
            }
            "calculate" => {
                let a = args.get("a").and_then(|v| v.as_f64()).ok_or("missing a")?;
                let b = args.get("b").and_then(|v| v.as_f64()).ok_or("missing b")?;
                let op = args.get("op").and_then(|v| v.as_str()).ok_or("missing op")?;

                let result = match op {
                    "add" => a + b,
                    "sub" => a - b,
                    "mul" => a * b,
                    "div" => {
                        if b == 0.0 {
                            return Ok(ToolOutput::error("Division by zero"));
                        }
                        a / b
                    }
                    _ => return Ok(ToolOutput::error("Unknown operation")),
                };

                Ok(ToolOutput::text(result.to_string()))
            }
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CalculateInput {
    a: f64,
    b: f64,
    op: String,
}

/// Parse tool arguments from JSON
fn parse_args<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(args.clone())
}

fn bench_tool_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_lookup");

    let handler = ToolHandler::new();

    group.bench_function("list_tools", |b| {
        b.iter(|| {
            black_box(handler.list_tools());
        });
    });

    group.bench_function("get_existing", |b| {
        b.iter(|| {
            black_box(handler.tools.get("echo"));
        });
    });

    group.bench_function("get_missing", |b| {
        b.iter(|| {
            black_box(handler.tools.get("nonexistent"));
        });
    });

    group.finish();
}

fn bench_arg_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("arg_parsing");

    let simple_args = json!({"message": "hello"});
    let complex_args = json!({
        "query": "SELECT * FROM users",
        "options": {
            "limit": 100,
            "fields": ["id", "name", "email"]
        }
    });
    let typed_args = json!({"a": 42.0, "b": 3.14, "op": "mul"});

    group.bench_with_input(
        BenchmarkId::new("simple", "value"),
        &simple_args,
        |b, args| {
            b.iter(|| {
                black_box(args.get("message"));
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("complex", "nested"),
        &complex_args,
        |b, args| {
            b.iter(|| {
                let query = args.get("query");
                let limit = args.get("options").and_then(|o| o.get("limit"));
                black_box((query, limit));
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("typed", "deserialize"),
        &typed_args,
        |b, args| {
            b.iter(|| {
                let result: Result<CalculateInput, _> = parse_args(black_box(args));
                let _ = black_box(result);
            });
        },
    );

    group.finish();
}

fn bench_tool_call(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_call");

    let handler = ToolHandler::new();

    group.bench_function("echo", |b| {
        let args = json!({"message": "hello world"});
        b.iter(|| {
            let _ = black_box(handler.call_tool("echo", black_box(args.clone())));
        });
    });

    group.bench_function("calculate", |b| {
        let args = json!({"a": 42.0, "b": 3.14, "op": "mul"});
        b.iter(|| {
            let _ = black_box(handler.call_tool("calculate", black_box(args.clone())));
        });
    });

    group.bench_function("not_found", |b| {
        let args = json!({});
        b.iter(|| {
            let _ = black_box(handler.call_tool("nonexistent", black_box(args.clone())));
        });
    });

    group.finish();
}

fn bench_result_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("result_creation");

    group.bench_function("text_short", |b| {
        b.iter(|| {
            black_box(ToolOutput::text("Hello, World!"));
        });
    });

    group.bench_function("text_long", |b| {
        let long_text = "x".repeat(10000);
        b.iter(|| {
            black_box(ToolOutput::text(black_box(&long_text).clone()));
        });
    });

    group.bench_function("json", |b| {
        let data = json!({"key": "value", "count": 42});
        b.iter(|| {
            black_box(ToolOutput::json(black_box(&data)).unwrap());
        });
    });

    group.bench_function("error", |b| {
        b.iter(|| {
            black_box(ToolOutput::error("Something went wrong"));
        });
    });

    group.bench_function("error_with_suggestion", |b| {
        b.iter(|| {
            black_box(ToolOutput::error_with_suggestion(
                "Invalid input",
                "Try using a valid email address",
            ));
        });
    });

    group.finish();
}

fn bench_full_invocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_invocation");

    let handler = ToolHandler::new();

    // Simulate the full path: receive JSON -> parse -> lookup -> execute -> serialize result
    group.bench_function("echo_e2e", |b| {
        let request_json = r#"{"name":"echo","arguments":{"message":"hello"}}"#;

        b.iter(|| {
            let request: Value = serde_json::from_str(black_box(request_json)).unwrap();
            let name = request.get("name").and_then(|v| v.as_str()).unwrap();
            let args = request.get("arguments").cloned().unwrap_or(json!({}));
            let result = handler.call_tool(name, args).unwrap();
            // Convert ToolOutput to CallToolResult for serialization
            let call_result: CallToolResult = result.into();
            let _ = black_box(serde_json::to_string(&call_result));
        });
    });

    group.bench_function("calculate_e2e", |b| {
        let request_json = r#"{"name":"calculate","arguments":{"a":42,"b":3.14,"op":"mul"}}"#;

        b.iter(|| {
            let request: Value = serde_json::from_str(black_box(request_json)).unwrap();
            let name = request.get("name").and_then(|v| v.as_str()).unwrap();
            let args = request.get("arguments").cloned().unwrap_or(json!({}));
            let result = handler.call_tool(name, args).unwrap();
            // Convert ToolOutput to CallToolResult for serialization
            let call_result: CallToolResult = result.into();
            let _ = black_box(serde_json::to_string(&call_result));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_tool_lookup,
    bench_arg_parsing,
    bench_tool_call,
    bench_result_creation,
    bench_full_invocation,
);

criterion_main!(benches);
