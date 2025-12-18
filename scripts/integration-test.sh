#!/usr/bin/env bash
# Integration test script for mcpkit
# Tests MCP protocol compliance using multiple methods
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SANDBOX_DIR="/tmp/mcpkit-integration-test-$$"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

cleanup() {
    rm -rf "$SANDBOX_DIR" 2>/dev/null || true
}
trap cleanup EXIT

echo "=============================================="
echo "mcpkit Integration Test Suite"
echo "=============================================="
echo ""

# Build release binaries
echo -e "${YELLOW}Building release binaries...${NC}"
cargo build --release -p filesystem-server --manifest-path "$PROJECT_ROOT/Cargo.toml" 2>&1 | tail -1

# Create sandbox directory
mkdir -p "$SANDBOX_DIR"
echo "Sandbox directory: $SANDBOX_DIR"
echo ""

SERVER_BIN="$PROJECT_ROOT/target/release/filesystem-server"

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

run_test() {
    local name="$1"
    shift
    echo -n "Testing $name... "
    if "$@" > /dev/null 2>&1; then
        echo -e "${GREEN}PASSED${NC}"
        ((TESTS_PASSED++)) || true
    else
        echo -e "${RED}FAILED${NC}"
        ((TESTS_FAILED++)) || true
    fi
}

echo "=== JSON-RPC Protocol Tests ==="

# Test initialize handshake
echo -n "Testing initialize... "
INIT_RESPONSE=$(echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"test"},"capabilities":{}}}' | timeout 5 "$SERVER_BIN" "$SANDBOX_DIR" 2>/dev/null || true)
if echo "$INIT_RESPONSE" | grep -q 'protocolVersion'; then
    echo -e "${GREEN}PASSED${NC}"
    ((TESTS_PASSED++)) || true
else
    echo -e "${RED}FAILED${NC}"
    ((TESTS_FAILED++)) || true
fi

# Test tools/list via MCP Inspector (if available)
echo ""
echo "=== MCP Inspector Tests ==="
if command -v npx &> /dev/null; then
    run_test "tools/list" timeout 30 npx @modelcontextprotocol/inspector --cli --method tools/list --transport stdio "$SERVER_BIN" "$SANDBOX_DIR"
    run_test "get_root tool" timeout 30 npx @modelcontextprotocol/inspector --cli --method tools/call --tool-name get_root --transport stdio "$SERVER_BIN" "$SANDBOX_DIR"
else
    echo -e "${YELLOW}npx not available - skipping MCP Inspector tests${NC}"
fi

echo ""
echo "=== Code Quality Checks ==="
run_test "cargo fmt" cargo fmt --all --check --manifest-path "$PROJECT_ROOT/Cargo.toml"
run_test "cargo clippy" cargo clippy --workspace --all-features --manifest-path "$PROJECT_ROOT/Cargo.toml" -- -D warnings

echo ""
echo "=== Unit Tests ==="
run_test "cargo test" cargo test --workspace --manifest-path "$PROJECT_ROOT/Cargo.toml"

echo ""
echo "=============================================="
echo "Test Results"
echo "=============================================="
echo -e "Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [ "$TESTS_FAILED" -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
