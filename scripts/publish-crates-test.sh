#!/usr/bin/env bash
# Harness for scripts/publish-crates.sh: exercises the success,
# already-uploaded, and real-error paths against a stub `cargo`, invoking the
# script exactly as release.yml does (`bash -e`), so what passes here is what
# runs in a release. No network, no real cargo, no real sleeps.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PUBLISH_SCRIPT="$SCRIPT_DIR/publish-crates.sh"

EXPECTED_ORDER="mcpkit-core mcpkit-transport mcpkit-server mcpkit-macros \
mcpkit-client mcpkit-testing mcpkit-axum mcpkit-actix mcpkit-rocket \
mcpkit-warp mcpkit"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
STUB_DIR="$TMP/bin"
CALL_LOG="$TMP/calls.log"
mkdir -p "$STUB_DIR"

# Stub cargo: logs each `publish -p <crate>` call, then consults MODE.
# MODE format, one directive per line in $TMP/mode: "<crate> <behavior>"
# where behavior is ok | already | fail. Unlisted crates default to ok.
cat > "$STUB_DIR/cargo" <<'STUB'
#!/usr/bin/env bash
crate="$3"   # cargo publish -p <crate>
echo "$crate" >> "$CALL_LOG"
behavior=$(awk -v c="$crate" '$1 == c { print $2 }' "$MODE_FILE" 2>/dev/null)
case "${behavior:-ok}" in
  ok)      echo "Uploading $crate"; exit 0 ;;
  already) echo "error: crate $crate@0.0.0 already uploaded" >&2; exit 101 ;;
  fail)    echo "error: the remote server responded with an error: broken" >&2; exit 101 ;;
esac
STUB
chmod +x "$STUB_DIR/cargo"

run_script() {
  : > "$CALL_LOG"
  # Invoke exactly as the release workflow does: `bash -e <script>`.
  CALL_LOG="$CALL_LOG" MODE_FILE="$TMP/mode" PUBLISH_WAIT_SECS=0 \
    PATH="$STUB_DIR:$PATH" bash -e "$PUBLISH_SCRIPT" > "$TMP/out.log" 2>&1
}

fail() { echo "FAIL: $1"; echo "--- output:"; cat "$TMP/out.log"; exit 1; }

# --- Path 1: clean publish — succeeds and hits every crate in order --------
: > "$TMP/mode"
run_script || fail "clean publish path exited non-zero"
[ "$(tr '\n' ' ' < "$CALL_LOG" | sed 's/ $//')" = "$(echo $EXPECTED_ORDER)" ] \
  || fail "publish order mismatch: $(tr '\n' ' ' < "$CALL_LOG")"
echo "PASS: clean publish (11 crates, dependency order)"

# --- Path 2: partial re-run — already-uploaded crates are skipped ----------
cat > "$TMP/mode" <<'MODE'
mcpkit-core already
mcpkit-transport already
mcpkit-server already
MODE
run_script || fail "already-uploaded path must be tolerated (re-run support)"
grep -q "mcpkit-core already published at this version" "$TMP/out.log" \
  || fail "missing skip notice for already-uploaded crate"
[ "$(wc -l < "$CALL_LOG")" -eq 11 ] || fail "re-run must still attempt all crates"
echo "PASS: partial-publish re-run (already-uploaded tolerated)"

# --- Path 3: real error — aborts and does not publish dependents -----------
cat > "$TMP/mode" <<'MODE'
mcpkit-server fail
MODE
if run_script; then fail "a real publish error must abort the release"; fi
grep -q "failed to publish mcpkit-server" "$TMP/out.log" \
  || fail "real error was not surfaced"
[ "$(wc -l < "$CALL_LOG")" -eq 3 ] \
  || fail "publishing must stop at the failing crate (got $(wc -l < "$CALL_LOG") calls)"
grep -q "mcpkit-macros" "$CALL_LOG" && fail "dependent crate was attempted after a real failure"
echo "PASS: real error aborts before dependents"

echo "All publish-script paths verified."
