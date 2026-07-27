#!/usr/bin/env bash
# Check that documentation version references match the workspace version.
#
# Extracted from .github/workflows/ci.yml so it can be exercised by
# scripts/version-sync-test.sh and run locally (`just version-sync`). An inline
# workflow step cannot be tested, and this one grew three distinct checks —
# each added after a real miss — with no way to confirm any of them still fire.
#
# Set MCPKIT_VERSION to override the version under test (used by the harness so
# it needs neither cargo nor a real workspace).
set -uo pipefail

if [ -n "${MCPKIT_VERSION:-}" ]; then
  VERSION="$MCPKIT_VERSION"
else
  VERSION=$(cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[] | select(.name == "mcpkit") | .version')
fi
MAJOR_MINOR=$(echo "$VERSION" | cut -d. -f1,2)

echo "Checking documentation references mcpkit version $MAJOR_MINOR..."
FAIL=0

# Every snippet-bearing doc must reference the current version at least once
# (umbrella crate or subcrate — all published mcpkit-* crates share the
# workspace version).
for f in README.md docs/getting-started.md docs/client-guide.md \
         docs/extensions.md docs/runtimes.md docs/wasm.md \
         docs/migration-from-rmcp.md; do
  if ! grep -qE "mcpkit(-[a-z]+)? = \"$MAJOR_MINOR\"" "$f"; then
    echo "ERROR: $f does not reference mcpkit(-*) = \"$MAJOR_MINOR\""
    FAIL=1
  fi
done

# No doc may pin any OTHER mcpkit version — this catches partially bumped
# files, which the per-file presence check alone would pass.
#
# Exemptions are per line, not per file. This used to pass
# --exclude=migration-to-1.0.md --exclude=api-stability.md --exclude-dir=adr,
# because each holds legitimately-old versions (migration paths, post-1.0
# contract examples, point-in-time ADRs). But a whole-file exclusion means one
# justified pin exempts every other line in that file forever — so a genuinely
# stale install snippet added to any of them would never be caught.
#
# A pin is exempt only where explicitly marked:
#   <!-- version-sync:ok — reason -->    on the line before a fenced block,
#                                        exempting that block
#   mcpkit = "1"  <!-- version-sync:ok -->   inline, one line
# HTML comments render invisibly, so this costs readers nothing and keeps the
# justification at the point of use.
STALE=$(find README.md docs -name '*.md' -print0 \
  | xargs -0 awk '
      /<!-- *version-sync:ok/ {
        if ($0 ~ /mcpkit[a-z-]* *= *"/) { next }
        pending = 1; next
      }
      /^[ \t]*```/ {
        if (in_block) { in_block = 0; exempt = 0 }
        else { in_block = 1; exempt = pending; pending = 0 }
        next
      }
      /mcpkit(-[a-z]+)? *= *"[0-9]/ {
        if (in_block && exempt) next
        print FILENAME ":" FNR ":" $0
      }
    ' \
  | grep -vE "mcpkit(-[a-z]+)? = \"$MAJOR_MINOR\"" || true)
if [ -n "$STALE" ]; then
  echo "ERROR: stale mcpkit version references (expected $MAJOR_MINOR):"
  echo "$STALE"
  FAIL=1
fi

# RELEASING.md declares the release it documents in its own header, using a
# different syntax from the dependency snippets above and living at the repo
# root — so neither the per-file presence check nor the stale sweep could see
# it. It sat at 0.6.0 for the whole 0.7.0 cycle and was only caught by hand
# during release prep.
if ! grep -qE "^\*\*Version:\*\* ${VERSION}( |$)" RELEASING.md; then
  echo "ERROR: RELEASING.md header does not declare **Version:** $VERSION"
  grep -nE "^\*\*Version:\*\*" RELEASING.md || true
  FAIL=1
fi

if [ "$FAIL" -ne 0 ]; then
  exit 1
fi
echo "All version references are correct!"
