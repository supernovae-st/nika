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

violations=0
violation_log=""

for crate in "${LIB_CRATES[@]}"; do
  crate_dir="crates/$crate"
  # Skip binary-only crates (no lib.rs).
  if [ ! -f "$crate_dir/src/lib.rs" ]; then
    continue
  fi

  # RUSTDOCFLAGS=-D warnings promotes every warning to an error.
  output=$(RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items -p "$crate" 2>&1 || true)
  if echo "$output" | grep -qE '^(error|warning):'; then
    violations=$((violations + 1))
    violation_log+="\n  $crate: cargo doc (private) emitted warnings/errors\n"
    violation_log+="$(echo "$output" | grep -E '^(error|warning):' | head -5 | sed 's/^/    /')\n"
  fi
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d crate(s) with private-doc drift:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: run 'RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps"
  echo "--document-private-items -p <crate>' locally to reproduce. Broken"
  echo "intra-doc links, missing backticks, and undocumented private"
  echo "items will all surface here. Gate 8 extended — see ADR-015."
  exit 2
fi

echo "OK: private-item rustdoc clean across ${#LIB_CRATES[@]} crate(s)"
exit 0
