#!/usr/bin/env bash
# Harness for scripts/version-sync.sh: proves each of its three checks can fail,
# and that the exemption mechanism still exempts.
#
# Every check in that script was added after a real miss — a doc left at the old
# version, a partially bumped file that the per-file presence check happily
# passed, and a RELEASING.md header that sat a full minor behind for an entire
# release cycle. None of them had anything confirming they still fire.
#
# The exemption case matters as much as the failures: line-level exemptions
# replaced whole-file excludes precisely so that one justified pin stops
# blanketing a file. If the exemption silently widened back out, the stale sweep
# would go quiet again — and quiet looks exactly like passing.
#
# Works on a copy; the working tree is never touched.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="9.9.9"          # what the fixture is "at"
OLD="1.2"                # a plausibly stale pin

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cd "$TMP" || exit 1

failures=0
fail() { echo "FAIL: $*"; failures=$((failures + 1)); }

DOCS="getting-started client-guide extensions runtimes wasm migration-from-rmcp"

# Build a minimal tree that passes, so any later failure is attributable to the
# single mutation rather than to pre-existing drift.
fixture() {
  rm -rf "$TMP/fx"; mkdir -p "$TMP/fx/docs"; cd "$TMP/fx" || exit 1
  printf '# mcpkit\n\n```toml\nmcpkit = "9.9"\n```\n' > README.md
  for d in $DOCS; do
    printf '# %s\n\n```toml\nmcpkit = "9.9"\n```\n' "$d" > "docs/$d.md"
  done
  printf '# Releasing\n\n**Version:** %s\n' "$VERSION" > RELEASING.md
}

run() { MCPKIT_VERSION="$VERSION" bash "$ROOT/scripts/version-sync.sh" > "$TMP/out.log" 2>&1; }

# expect <pass|fail> <label>
expect() {
  local want="$1" label="$2"
  run; local got=$?
  if [ "$want" = pass ] && [ "$got" -ne 0 ]; then
    fail "$label: expected the check to pass, got exit $got"; sed 's/^/      /' "$TMP/out.log"
  elif [ "$want" = fail ] && [ "$got" -eq 0 ]; then
    fail "$label: expected the check to FAIL, but it passed — this check cannot fail"
  else
    echo "  ok  $label"
  fi
}

# --- Baseline: a clean fixture must pass -----------------------------------
fixture
expect pass "clean fixture passes"

# --- Check 1: a doc that never mentions the current version ----------------
fixture
printf '# runtimes\n\nno snippet here\n' > docs/runtimes.md
expect fail "check 1: doc missing any current-version reference"

# --- Check 2: a partially bumped file ---------------------------------------
# The per-file presence check passes here — the file *does* mention 9.9 — so
# only the stale sweep can catch it. This is the case that motivated it.
fixture
printf '# wasm\n\n```toml\nmcpkit = "9.9"\n```\n\n```toml\nmcpkit = "%s"\n```\n' "$OLD" > docs/wasm.md
expect fail "check 2: file with both a current and a stale pin"

# --- Check 3: RELEASING.md header left behind -------------------------------
fixture
printf '# Releasing\n\n**Version:** 0.6.0\n' > RELEASING.md
expect fail "check 3: RELEASING.md header a release behind"

# --- Exemptions: still exempt, and still narrow -----------------------------
fixture
{ printf '# migration\n\n<!-- version-sync:ok — historical migration path -->\n'
  printf '```toml\nmcpkit = "%s"\n```\n' "$OLD"; } > docs/migration.md
expect pass "exemption: marked block is exempt"

fixture
{ printf '# migration\n\nmcpkit = "%s"  <!-- version-sync:ok -->\n' "$OLD"; } > docs/migration.md
expect pass "exemption: marked inline pin is exempt"

# The exemption must NOT blanket the rest of the file. A whole-file exclude is
# exactly what this replaced, so if it ever widens back out, this goes quiet.
fixture
{ printf '# migration\n\n<!-- version-sync:ok — historical -->\n'
  printf '```toml\nmcpkit = "%s"\n```\n\n' "$OLD"
  printf '```toml\nmcpkit = "%s"\n```\n' "$OLD"; } > docs/migration.md
expect fail "exemption is per block, not per file"

if [ "$failures" -ne 0 ]; then
  echo
  echo "$failures check(s) in version-sync.sh do not behave as intended."
  exit 1
fi

echo
echo "version-sync.sh fired on every planted defect and honoured every exemption."
