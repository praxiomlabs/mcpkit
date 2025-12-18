# MCP SDK Fuzzing

This directory contains fuzz tests for the MCP SDK using `cargo-fuzz` and `libFuzzer`.

## Prerequisites

Install `cargo-fuzz`:

```bash
cargo install cargo-fuzz
```

Fuzzing requires nightly Rust:

```bash
rustup install nightly
```

## Available Fuzz Targets

| Target | Description |
|--------|-------------|
| `fuzz_jsonrpc_message` | Fuzzes parsing of arbitrary bytes as JSON-RPC `Message` |
| `fuzz_jsonrpc_request` | Fuzzes parsing of JSON-RPC `Request` messages |
| `fuzz_jsonrpc_response` | Fuzzes parsing of JSON-RPC `Response` messages |
| `fuzz_progress_token` | Fuzzes parsing of `ProgressToken` values |
| `fuzz_jsonrpc_structured` | Structure-aware fuzzing with `arbitrary` crate |
| `fuzz_protocol_version` | Fuzzes `ProtocolVersion` parsing and negotiation |

## Running Fuzzers

Run a specific fuzzer:

```bash
cd fuzz
cargo +nightly fuzz run fuzz_jsonrpc_message
```

Run with a specific number of iterations:

```bash
cargo +nightly fuzz run fuzz_jsonrpc_message -- -runs=10000
```

Run with multiple jobs in parallel:

```bash
cargo +nightly fuzz run fuzz_jsonrpc_message -- -jobs=4 -workers=4
```

## Seed Corpus

Seed corpus files are located in `corpus/<target_name>/`. These provide initial inputs that help the fuzzer find interesting code paths faster.

To add a new seed corpus file:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"test"}' > corpus/fuzz_jsonrpc_message/my_seed
```

## Crash Artifacts

When a crash is found, `cargo-fuzz` will save the crashing input to `artifacts/<target_name>/`.

To reproduce a crash:

```bash
cargo +nightly fuzz run fuzz_jsonrpc_message artifacts/fuzz_jsonrpc_message/crash-xxxxx
```

## Coverage

Generate coverage reports:

```bash
cargo +nightly fuzz coverage fuzz_jsonrpc_message
```

## Minimizing Corpus

After running for a while, minimize the corpus to remove redundant inputs:

```bash
cargo +nightly fuzz cmin fuzz_jsonrpc_message
```

## CI Integration

### Automated Fuzzing

The SDK includes automated fuzzing via GitHub Actions (`.github/workflows/fuzz.yml`):

- **Nightly runs**: Fuzzing runs automatically every night at 2 AM UTC
- **Duration**: Each target runs for 5 minutes (300 seconds) by default
- **Corpus caching**: The fuzzing corpus is cached between runs to accumulate interesting inputs
- **Crash notifications**: If a crash is found, a GitHub issue is automatically created

### Manual Triggering

You can trigger fuzzing manually via the GitHub Actions UI:

1. Go to Actions > Fuzzing
2. Click "Run workflow"
3. Optionally specify:
   - **duration**: Seconds to run each target (default: 300)
   - **target**: Specific target to run (leave empty for all)

### Local CI Simulation

To simulate CI fuzzing locally:

```bash
# Run all targets for 60 seconds each
for target in fuzz_jsonrpc_message fuzz_jsonrpc_request fuzz_jsonrpc_response fuzz_progress_token fuzz_jsonrpc_structured fuzz_protocol_version; do
    cargo +nightly fuzz run $target -- -max_total_time=60
done
```

## Security

If you find a security issue through fuzzing, please report it according to our security policy in `SECURITY.md`.
