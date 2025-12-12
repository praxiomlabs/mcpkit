//! Benchmarks for transport operations.
//!
//! Run with: `cargo bench --package rust-mcp-benches --bench transport`

// Allow missing docs for criterion_group! macro generated functions
#![allow(missing_docs)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Simulated message for transport benchmarks
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: u64,
    method: String,
    params: Value,
}

impl Message {
    fn new(id: u64, method: &str) -> Self {
        Self {
            id,
            method: method.to_string(),
            params: json!({}),
        }
    }

    fn with_params(id: u64, method: &str, params: Value) -> Self {
        Self {
            id,
            method: method.to_string(),
            params,
        }
    }
}

/// Simulated in-memory transport for benchmarking
struct MemoryTransport {
    buffer: Arc<Mutex<VecDeque<String>>>,
}

impl MemoryTransport {
    fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn send(&self, message: &Message) {
        let json = serde_json::to_string(message).unwrap();
        self.buffer.lock().await.push_back(json);
    }

    async fn recv(&self) -> Option<Message> {
        let json = self.buffer.lock().await.pop_front()?;
        serde_json::from_str(&json).ok()
    }
}

/// Benchmark channel-based message passing
fn bench_channel_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput");
    let rt = tokio::runtime::Runtime::new().unwrap();

    for batch_size in [1, 10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size));

        group.bench_with_input(
            BenchmarkId::new("mpsc", batch_size),
            &batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async move {
                    let (tx, mut rx) = mpsc::channel::<Message>(1024);

                    // Send messages
                    for i in 0..size {
                        tx.send(Message::new(i, "test")).await.unwrap();
                    }
                    drop(tx);

                    // Receive messages
                    let mut count = 0u64;
                    while rx.recv().await.is_some() {
                        count += 1;
                    }
                    black_box(count)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mpsc_unbounded", batch_size),
            &batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async move {
                    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

                    // Send messages
                    for i in 0..size {
                        tx.send(Message::new(i, "test")).unwrap();
                    }
                    drop(tx);

                    // Receive messages
                    let mut count = 0u64;
                    while rx.recv().await.is_some() {
                        count += 1;
                    }
                    black_box(count)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory transport operations
fn bench_memory_transport(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_transport");
    let rt = tokio::runtime::Runtime::new().unwrap();

    group.bench_function("send_recv_single", |b| {
        b.to_async(&rt).iter(|| async {
            let transport = MemoryTransport::new();
            let msg = Message::new(1, "test");

            transport.send(&msg).await;
            black_box(transport.recv().await)
        });
    });

    for batch_size in [10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size));

        group.bench_with_input(
            BenchmarkId::new("send_recv_batch", batch_size),
            &batch_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async move {
                    let transport = MemoryTransport::new();

                    // Send all messages
                    for i in 0..size {
                        transport.send(&Message::new(i, "test")).await;
                    }

                    // Receive all messages
                    let mut count = 0u64;
                    while transport.recv().await.is_some() {
                        count += 1;
                    }
                    black_box(count)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark message size impact
fn bench_message_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_sizes");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Different payload sizes
    let sizes = [
        ("tiny", json!({"x": 1})),
        ("small", json!({"message": "hello world", "count": 42})),
        (
            "medium",
            json!({
                "query": "SELECT * FROM users WHERE active = true",
                "options": {
                    "limit": 100,
                    "offset": 0,
                    "fields": ["id", "name", "email", "created_at"]
                },
                "metadata": {
                    "requestId": "abc123",
                    "timestamp": 1234567890
                }
            }),
        ),
        (
            "large",
            json!({
                "data": vec!["item"; 1000].iter().enumerate()
                    .map(|(i, _)| json!({"id": i, "value": format!("value_{}", i)}))
                    .collect::<Vec<_>>()
            }),
        ),
    ];

    for (name, params) in &sizes {
        let msg = Message::with_params(1, "test", params.clone());
        let json_size = serde_json::to_string(&msg).unwrap().len();

        group.throughput(Throughput::Bytes(json_size as u64));

        group.bench_with_input(BenchmarkId::new("serialize", name), &msg, |b, msg| {
            b.iter(|| black_box(serde_json::to_string(msg).unwrap()));
        });

        let json = serde_json::to_string(&msg).unwrap();
        group.bench_with_input(BenchmarkId::new("deserialize", name), &json, |b, json| {
            b.iter(|| {
                black_box(serde_json::from_str::<Message>(json).unwrap());
            });
        });

        group.bench_with_input(BenchmarkId::new("roundtrip", name), &msg, |b, msg| {
            b.to_async(&rt).iter(|| async {
                let transport = MemoryTransport::new();
                transport.send(msg).await;
                black_box(transport.recv().await)
            });
        });
    }

    group.finish();
}

/// Benchmark concurrent access patterns
fn bench_concurrent_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_access");
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Benchmark with multiple concurrent senders
    for num_senders in [1, 2, 4, 8] {
        let messages_per_sender = 100u64;

        group.throughput(Throughput::Elements(num_senders * messages_per_sender));

        group.bench_with_input(
            BenchmarkId::new("multi_sender", num_senders),
            &num_senders,
            |b, &senders| {
                b.to_async(&rt).iter(|| async move {
                    let (tx, mut rx) = mpsc::channel::<Message>(1024);

                    // Spawn senders
                    let mut handles = Vec::new();
                    for sender_id in 0..senders {
                        let tx = tx.clone();
                        handles.push(tokio::spawn(async move {
                            for i in 0..messages_per_sender {
                                let id = sender_id * messages_per_sender + i;
                                tx.send(Message::new(id, "test")).await.unwrap();
                            }
                        }));
                    }
                    drop(tx);

                    // Wait for senders
                    for handle in handles {
                        handle.await.unwrap();
                    }

                    // Receive all
                    let mut count = 0u64;
                    while rx.recv().await.is_some() {
                        count += 1;
                    }
                    black_box(count)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_channel_throughput,
    bench_memory_transport,
    bench_message_sizes,
    bench_concurrent_access,
);

criterion_main!(benches);
