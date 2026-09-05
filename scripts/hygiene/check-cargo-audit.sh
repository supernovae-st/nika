#!/usr/bin/env bash
# Vector 15: RustSec vulnerabilities and unsoundness in every lock family.
set -uo pipefail
if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "cargo-audit not installed (run: cargo install cargo-audit)"
  exit 2
fi

engine_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$engine_root" || exit 2
locks=(Cargo.lock fuzz/Cargo.lock)
for lock in crates/*/Cargo.lock; do
  [ ! -f "$lock" ] || locks+=("$lock")
done
scratch="$(mktemp -d)" || exit 2
trap 'rm -r "$scratch"' EXIT
failed=0
for lock in "${locks[@]}"; do
  if [ ! -f "$lock" ]; then
    printf 'RustSec: required lockfile missing: %s\n' "$lock"
    failed=1
    continue
  fi
  # Unsoundness is informational in RustSec, but not acceptable by default.
  # The same explicit advisory policy applies to root, fuzz and excluded crates.
  # Scan once: a second invocation could answer a different database/network.
  if ! cargo audit --file "$lock" --deny unsound --quiet >"$scratch/output" 2>&1; then
    printf 'RustSec: %s refused (finding or tool failure)\n' "$lock"
    cat "$scratch/output"
    failed=1
  fi
done
[ "$failed" -eq 0 ] || exit 2
printf 'OK (%s lockfiles; no blocking vulnerabilities or unsoundness under configured policy)\n' \
  "${#locks[@]}"
