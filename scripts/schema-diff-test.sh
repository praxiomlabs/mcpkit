#!/usr/bin/env bash
# Harness for scripts/schema-diff.sh: proves the gate can fail.
#
# A conformance gate that reports success without having verified anything is
# worse than no gate — it converts "unchecked" into "checked and fine". This
# repo has shipped that twice: schema-diff rows 1 and 2 extracted method names
# with a wide literal sweep that matched every spec method in doc comments and
# test fixtures, so the comparison was saturated and could not fail; and the
# baseline header stated a guarantee the gate did not provide.
#
# So: plant a defect the gate is supposed to notice, and assert it notices.
# Each mutation below maps to a specific row, and each is a real regression
# someone could introduce — a renamed method constant, a dropped `_meta`, a
# removed enum variant.
#
# Reads only, and works on a copy, so the working tree is never touched.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

command -v jq >/dev/null || { echo "error: jq not found" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/scripts"
cp -r "$ROOT/spec" "$ROOT/crates" "$TMP/"
cp "$ROOT/scripts/schema-diff.sh" "$TMP/scripts/"
cd "$TMP"

failures=0
fail() { echo "FAIL: $*"; failures=$((failures + 1)); }

# --- The clean run must reproduce the checked-in baseline -------------------
# Guards the harness itself: if this drifts, every mutation below is comparing
# against the wrong reference and "detected" means nothing.
./scripts/schema-diff.sh --baseline > clean.txt 2>/dev/null
grep -vE '^\s*(#|$)' "$ROOT/spec/schema-diff-baseline.txt" > expected.txt
if ! diff -q expected.txt clean.txt >/dev/null; then
  echo "FAIL: a clean run does not match spec/schema-diff-baseline.txt."
  echo "      The harness cannot attribute any difference to its mutations."
  diff -u expected.txt clean.txt | head -20
  exit 1
fi

# --- Mutations -------------------------------------------------------------
# probe <label> <file> <sed-expr>
probe() {
  local label="$1" file="$2" expr="$3"
  cp "$ROOT/$file" "$file"
  sed -i "$expr" "$file"
  if diff -q "$ROOT/$file" "$file" >/dev/null; then
    fail "$label: the mutation changed nothing — the sed expression no longer matches."
    return
  fi
  ./scripts/schema-diff.sh --baseline > mutated.txt 2>/dev/null || true
  if diff -q clean.txt mutated.txt >/dev/null; then
    fail "$label: schema-diff.sh did not notice. This row cannot fail."
  else
    echo "  ok  $label -> $(diff clean.txt mutated.txt | grep -m1 '^[<>]' | cut -c1-72)"
  fi
  cp "$ROOT/$file" "$file"
}

METHODS="crates/mcpkit-core/src/methods.rs"

# Row 1, schema -> mcpkit: a spec method we stop declaring.
probe "row 1: dropped request method" "$METHODS" '/= "tools\/call";/d'

# Row 1, mcpkit -> schema: a method we declare that the spec does not have.
# The reverse direction is separate: a gate that only looks one way is blind to
# our own typos and renames.
probe "row 1: method renamed off-spec" "$METHODS" 's|"tools/call"|"tools/kall"|'

# Row 2: notification methods, same two directions.
probe "row 2: dropped notification method" "$METHODS" '\|notifications/initialized|d'

# Row 3: a variant removed from a closed enum.
probe "row 3: dropped enum variant" "crates/mcpkit-core/src/types/content.rs" '/^\s*Assistant,$/d'

# Field presence: `_meta` on a request type. This is the shape of a real
# conformance failure — elicitation params carried no `_meta` for a spec MUST,
# and the gate's alias map was hiding it.
probe "fields: dropped _meta" "crates/mcpkit-core/src/types/elicitation.rs" '/pub meta: Option<Meta>/d'

if [ "$failures" -ne 0 ]; then
  echo
  echo "$failures mutation(s) went unnoticed by scripts/schema-diff.sh."
  exit 1
fi

echo
echo "schema-diff.sh rejected every planted defect."
