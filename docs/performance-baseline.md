# Performance Baseline

Performance benchmarks captured using Criterion on 2025-12-18.

## Environment

- **OS**: Linux (WSL2)
- **Rust**: 1.85+
- **Profile**: Release (`--release`)

## Request Processing

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Single cycle | 1.2 µs | - |
| Batch 10 | 13.7 µs | 730K elem/s |
| Batch 100 | 133 µs | 751K elem/s |
| Batch 1000 | 1.35 ms | 737K elem/s |

## Tool Operations

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Create 10 results | 450 ns | 22M elem/s |
| Create 100 results | 8.1 µs | 12.3M elem/s |
| Create 1000 results | 90 µs | 11M elem/s |
| Registry build (10 tools) | 8.7 µs | - |
| Registry build (100 tools) | 82 µs | - |
| Lookup existing | 11.4 ns | - |
| Lookup missing | 9.2 ns | - |

## Tool Invocation

| Operation | Latency |
|-----------|---------|
| List tools | 9.0 ns |
| Get existing tool | 12.0 ns |
| Get missing tool | 9.0 ns |
| Echo tool call | 75 ns |
| Calculate tool call | 184 ns |
| Tool not found | 31 ns |
| Full echo E2E | 427 ns |
| Full calculate E2E | 698 ns |

## JSON Serialization

| Operation | Size | Latency | Throughput |
|-----------|------|---------|------------|
| Request serialize (minimal) | - | 77 ns | 12.9M elem/s |
| Request serialize (complex) | - | 438 ns | 2.3M elem/s |
| Request deserialize (71B) | 71B | 191 ns | 354 MiB/s |
| Request deserialize (355B) | 355B | 1.14 µs | 296 MiB/s |
| Response serialize (success) | - | 92 ns | 10.9M elem/s |
| Response serialize (large) | - | 6.4 µs | 156K elem/s |
| Response deserialize (102B) | 102B | 366 ns | - |
| Response deserialize (11KB) | 11KB | 30 µs | - |
| Roundtrip request | - | 1.65 µs | - |
| Roundtrip response | - | 29.7 µs | - |

## Message Size Performance

| Size | Serialize | Deserialize | Roundtrip |
|------|-----------|-------------|-----------|
| Tiny (39B) | 49 ns (800 MiB/s) | 125 ns (309 MiB/s) | 272 ns |
| Small (70B) | 69 ns (968 MiB/s) | 172 ns (387 MiB/s) | 328 ns |
| Medium (221B) | 211 ns (1000 MiB/s) | 657 ns (321 MiB/s) | 1.06 µs |
| Large (29KB) | 56 µs (524 MiB/s) | 190 µs (154 MiB/s) | 212 µs |

## Transport Performance

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Memory single send/recv | 280 ns | - |
| Memory batch 10 | 2.2 µs | 4.5M elem/s |
| Memory batch 100 | 23 µs | 4.4M elem/s |
| Memory batch 1000 | 214 µs | 4.7M elem/s |
| MPSC channel (1) | 197 ns | 5.1M elem/s |
| MPSC channel (100) | 9.8 µs | 10.2M elem/s |
| MPSC channel (1000) | 106 µs | 9.4M elem/s |
| MPSC unbounded (100) | 7.2 µs | 13.9M elem/s |
| MPSC unbounded (1000) | 74 µs | 13.5M elem/s |

## Concurrent Access

| Senders | Latency | Throughput |
|---------|---------|------------|
| 1 | 51 µs | 1.9M elem/s |
| 2 | 79 µs | 2.5M elem/s |
| 4 | 102 µs | 3.9M elem/s |
| 8 | 175 µs | 4.6M elem/s |

## Memory Operations

| Operation | Latency |
|-----------|---------|
| Small JSON clone | 50 ns |
| Medium JSON clone | 14 µs |
| Large JSON clone | 347 µs |
| Small JSON roundtrip | 96 ns |
| Medium JSON roundtrip | 24 µs |
| Large JSON roundtrip | 618 µs |

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench --package mcpkit-benches

# Run specific benchmark suite
cargo bench --package mcpkit-benches -- serialization
cargo bench --package mcpkit-benches -- tool_invocation
cargo bench --package mcpkit-benches -- transport
cargo bench --package mcpkit-benches -- memory

# Generate HTML report
cargo bench --package mcpkit-benches -- --verbose
# Results in: target/criterion/report/index.html
```

## Notes

- All benchmarks run in release mode with optimizations
- Results may vary based on system load and hardware
- Use `gnuplot` for better visualizations: `apt install gnuplot`
- Criterion automatically detects performance regressions between runs
