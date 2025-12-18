#!/usr/bin/env bash
# Wrapper script for Claude Desktop WSL2 integration
# Ensures sandbox exists and starts the filesystem server

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SANDBOX_DIR="${1:-/tmp/mcpkit-sandbox}"

# Ensure sandbox directory exists
mkdir -p "$SANDBOX_DIR"

# Run the server
exec "$PROJECT_ROOT/target/release/filesystem-server" "$SANDBOX_DIR"
