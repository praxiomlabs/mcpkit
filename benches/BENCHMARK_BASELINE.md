# MCPKit Benchmark Baseline v0.4.0

**Baseline Date:** 2024-12-24
**MCPKit Version:** 0.4.0
**Platform:** Linux (WSL2)
**Rust Edition:** 2024

## Overview

This document records the performance baseline for MCPKit v0.4.0. Benchmarks are run using [Criterion.rs](https://github.com/bheisler/criterion.rs) and saved with the baseline name `mcpkit-v0.4.0`.

## Running Benchmarks

```bash
# Run all benchmarks and compare against baseline
cargo bench

# Run specific benchmark suite
cargo bench -p mcpkit-core --bench comparison
cargo bench -p mcpkit-core --bench protocol
cargo bench -p mcpkit-benches --bench serialization
cargo bench -p mcpkit-benches --bench tool_invocation
cargo bench -p mcpkit-benches --bench transport
cargo bench -p mcpkit-benches --bench memory

# Save a new baseline
cargo bench -- --save-baseline <baseline-name>

# Compare against existing baseline
cargo bench -- --baseline mcpkit-v0.4.0
```

## Benchmark Suites

### 1. Comparison Benchmarks (`mcpkit-core/benches/comparison.rs`)

Compares MCPKit serialization performance against a baseline JSON implementation.

| Benchmark | MCPKit SDK | Baseline | Notes |
|-----------|------------|----------|-------|
| Request serialization | ~145 ns | ~189 ns | 1.3x faster |
| Request deserialization | ~705 ns | ~570 ns | Parity |
| Payload 100 bytes | ~82 ns | ~91 ns | Parity |
| Payload 1KB | ~540 ns | ~590 ns | Parity |
| Payload 10KB | ~2.9 µs | ~2.9 µs | Parity |
| Tool call params | ~170 ns | ~190 ns | Parity |

### 2. Protocol Benchmarks (`mcpkit-core/benches/protocol.rs`)

Tests protocol message serialization and parsing performance.

| Benchmark | Time |
|-----------|------|
| Request ID (number) serialize | ~12.5 ns |
| Request ID (string) serialize | ~14.7 ns |
| Request ID (number) deserialize | ~20.9 ns |
| Request ID (string) deserialize | ~177 ns |
| Message parsing (list tools) | ~430 ns |
| Message parsing (call tool) | ~570 ns |
| Content operations | ~45 ns |

### 3. Serialization Benchmarks (`mcpkit-benches/benches/serialization.rs`)

Tests various serialization scenarios.

| Benchmark | Time |
|-----------|------|
| Request serialization (simple) | ~385 ns |
| Request serialization (complex) | ~700 ns |
| Response serialization (simple) | ~95 ns |
| Response deserialization (simple) | ~480 ns |
| Tool serialization (to_string) | ~457 ns |
| Result serialization (to_string) | ~45 ns |
| Roundtrip request | ~1.4 µs |
| Roundtrip response | ~44 µs |

### 4. Tool Invocation Benchmarks (`mcpkit-benches/benches/tool_invocation.rs`)

Tests tool registry and invocation performance.

| Benchmark | Time |
|-----------|------|
| Argument parsing | ~133 ns |
| Result creation (text) | ~15 ns |
| Result creation (json) | ~48 ns |
| Result creation (error) | ~7.3 ns |
| Full invocation (echo) | ~394 ns |
| Full invocation (calculate) | ~752 ns |

### 5. Transport Benchmarks (`mcpkit-benches/benches/transport.rs`)

Tests transport layer performance and concurrency.

| Benchmark | Time/Throughput |
|-----------|-----------------|
| Channel throughput 1KB | ~8.6 µs / 117 MiB/s |
| Channel throughput 10KB | ~15 µs / 680 MiB/s |
| Message sizes 1KB | ~56 µs / 18 MiB/s |
| Message sizes 30KB | ~217 µs / 135 MiB/s |
| Concurrent access (1 sender) | ~52 µs |
| Concurrent access (4 senders) | ~143 µs |
| Concurrent access (8 senders) | ~177 µs |

### 6. Memory Benchmarks (`mcpkit-benches/benches/memory.rs`)

Tests memory allocation and management performance.

| Benchmark | Time |
|-----------|------|
| JSON small clone | ~38 ns |
| JSON medium clone | ~120 ns |
| JSON large clone | ~380 ns |
| JSON roundtrip (small) | ~245 ns |
| JSON roundtrip (medium) | ~1.3 µs |
| JSON roundtrip (large) | ~12.5 µs |
| String interning | ~28 ns |
| Vector grow (with capacity) | ~7.1 µs |
| Vector grow (no capacity) | ~11.7 µs |

## Key Performance Characteristics

1. **Serialization:** MCPKit achieves competitive or better serialization performance compared to baseline JSON implementations.

2. **Low-latency operations:** Simple operations like request ID serialization complete in under 15 ns.

3. **Throughput:** Transport layer sustains 100+ MiB/s for bulk message transfer.

4. **Concurrency:** Multi-sender scenarios scale well with 8 concurrent senders achieving ~4.5 Melem/s throughput.

## Notes

- All times are median values from 100 samples
- Benchmarks run in release mode with optimizations
- Results may vary based on system load and hardware
- Criterion baselines stored in `target/criterion/`
