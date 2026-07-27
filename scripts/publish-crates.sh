#!/usr/bin/env bash
# Publish all mcpkit crates to crates.io in dependency order.
#
# Invoked by .github/workflows/release.yml as `bash -e scripts/publish-crates.sh`
# and by scripts/publish-crates-test.sh the same way, so the tested shell
# semantics are the released shell semantics by construction.
#
# Order matters: each crate must have its dependencies published first. Derived
# from the workspace graph, normal dependencies only — a dev-dependency does not
# gate a publish:
#
#   mcpkit-core      (none)
#   mcpkit-transport  <- core
#   mcpkit-server     <- core, transport
#   mcpkit-macros     <- core (dev only)
#   mcpkit-client     <- core, transport
#   mcpkit-testing    <- core, transport, server
#   axum/actix/rocket/warp <- core, transport, server
#   mcpkit            <- core, transport, server, macros, client   (last)
#
# Several orders satisfy that; the one below is one of them. Re-derive with
# `cargo metadata --no-deps` rather than trusting this comment.
#
# The note previously here claimed mcpkit-macros dev-depends on mcpkit-server,
# so server had to precede it. That stopped being true in 3a351d5 (2025-12-23),
# seven months before this file was written — it was transcribed stale. It was
# also the only stated rationale for the ordering, so a later audit read it as
# a live constraint and wrongly reported the Justfile's different-but-equally-
# valid order as a defect.
#
# A real publish failure aborts the release. The only tolerated error is
# "already uploaded/exists", so re-running after a partial publish skips the
# crates already on crates.io instead of masking every failure (the old
# `continue-on-error: true` hid genuine errors on all but the last crate).
# Note: native `cargo publish --workspace` was evaluated and rejected — its
# upfront verify_unpublished check hard-fails on the first already-published
# crate, so a partial-publish re-run aborts instead of skipping (see
# docs/adr/0005-v0.6.0-release-incident.md and #53's advisory follow-up).

# NB: callers run this with `bash -e`. Capture the publish output via an `if`
# condition so a non-zero cargo exit does NOT trip errexit before we can print
# the error and check for "already uploaded".
set -uo pipefail

# Seconds to wait for crates.io index propagation between dependent publishes.
# Overridable so the test harness does not sleep for real.
WAIT_SECS="${PUBLISH_WAIT_SECS:-30}"

publish() {
  echo "::group::publish $1"
  local out
  if out=$(cargo publish -p "$1" 2>&1); then
    echo "$out"
    echo "::endgroup::"
    return 0
  fi
  echo "$out"
  echo "::endgroup::"
  if echo "$out" | grep -qiE "already (exists|uploaded)"; then
    echo "::notice::$1 already published at this version — skipping"
    return 0
  fi
  echo "::error::failed to publish $1"
  exit 1
}

# Let the crates.io index propagate before publishing dependents.
wait_index() { echo "waiting ${WAIT_SECS}s for crates.io index..."; sleep "$WAIT_SECS"; }

publish mcpkit-core
wait_index
publish mcpkit-transport
wait_index
publish mcpkit-server
wait_index
publish mcpkit-macros
wait_index
publish mcpkit-client
wait_index
publish mcpkit-testing
wait_index
publish mcpkit-axum
wait_index
publish mcpkit-actix
wait_index
publish mcpkit-rocket
wait_index
publish mcpkit-warp
wait_index
publish mcpkit
