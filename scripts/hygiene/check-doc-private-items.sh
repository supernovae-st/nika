#!/usr/bin/env bash
# Vector 28: cargo doc --document-private-items must succeed with 0 warnings.
#
# Gate 8 of the 12-gate admission protocol requires cargo doc clean. This
# vector extends Gate 8 to also document PRIVATE items — which catches
# broken rustdoc links, missing intra-doc refs, and malformed doc comments
# that the pub-only `cargo doc` would silently pass.
#
# SOTA projects (axum, tokio, bevy) enforce this because private items
# become pub when we crate-split, and their docs become the contract.
#
# Runs against every admitted lib crate (binary-only crates have no doc
# surface and are skipped).
#
# Exit codes:
#   0 -- GREEN (all clean)
#   2 -- RED (at least one warning or error)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Lib crates to check (binaries excluded — no doc surface). Parsed from
# workspace layers metadata so adding/removing a crate auto-propagates.
mapfile -t LIB_CRATES < <(grep -E '^layers\.[a-z0-9-]+' Cargo.toml \
  | sed -E 's/^layers\.([a-z0-9-]+) = .*/\1/')

if [ "${#LIB_CRATES[@]}" -eq 0 ]; then
  echo "WARN: no crates in [workspace.metadata.diamond.layers] — skipped"
  exit 0
fi

# Collect a `-p` flag for every lib crate (binary-only crates have no doc
# surface). A SINGLE `cargo doc` invocation resolves the (large) dep graph
# once; a per-crate loop re-resolves it N times and is ~10x slower on a heavy
# dep tree (the candle ML stack) — ~19 min vs ~2 min — which false-RED-timeouts
# the pre-push gate. Same coverage, same -D-warnings strictness, one pass.
pkg_args=()
checked=0
for crate in "${LIB_CRATES[@]}"; do
  if [ -f "crates/$crate/src/lib.rs" ]; then
    pkg_args+=(-p "$crate")
    checked=$((checked + 1))
  fi
done

# A DEDICATED target dir, and the reason is the gate's shape. `cargo test`,
# `cargo clippy --all-targets` and this `cargo doc --document-private-items`
# each carry different flags, so one shared target dir makes each invalidate
# the other two. The pre-push gate runs all three back to back, so this vector
# always paid a FULL cold rebuild and blew its timeout twice on 2026-07-27:
# first at the 300s default, then again at 900s. Run alone against a warm
# shared dir the very same check finishes in 220s, clean across 55 crates.
# Its own dir keeps its artifacts across gate runs, where test and clippy
# cannot reach them. Costs one cold build the first time and a few GB on disk.
export CARGO_TARGET_DIR="$REPO_ROOT/target/doc-check"

# RUSTDOCFLAGS=-D warnings promotes every rustdoc warning to an error.
output=$(RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items "${pkg_args[@]}" 2>&1 || true)
if echo "$output" | grep -qE '^(error|warning):'; then
  printf "RED: private-doc drift (cargo doc --document-private-items emitted warnings/errors):\n"
  echo "$output" | grep -E '^(error|warning):' | head -20 | sed 's/^/    /'
  echo ""
  echo "Hint: run 'RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps"
  echo "--document-private-items -p <crate>' locally to reproduce. Broken"
  echo "intra-doc links, missing backticks, and undocumented private"
  echo "items will all surface here. Gate 8 extended — see ADR-015."
  exit 2
fi

echo "OK: private-item rustdoc clean across ${checked} lib crate(s)"
exit 0
