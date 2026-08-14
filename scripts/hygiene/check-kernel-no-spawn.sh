#!/usr/bin/env bash
# Vector 26: Forbid spawn primitives in nika-kernel (L0.5 runtime-agnostic).
#
# nika-kernel defines trait contracts only (ADR-006, ADR-014, ADR-034).
# Spawning a task binds the kernel to a specific runtime (tokio, async-std,
# smol) — that decision must live at L3 (runtime/daemon), NOT at the trait
# layer. Any `tokio::spawn` in nika-kernel leaks runtime choice upward and
# breaks the "L0.5 is runtime-agnostic" invariant.
#
# Allowed:
#   - `tokio::sync::Mutex` / channels in public signatures (type-only)
#   - `futures_core::Stream` type declarations
#
# SCOPE IS DERIVED, never typed. This read `crates/nika-kernel/src`, which
# the 2026-06-10 split left holding 263 of the 13302 lines of L0.5 source —
# 2% of the invariant it claims to enforce, and green on the other 98%
# without saying so. The layer metadata in Cargo.toml already names the
# L0.5 set; asking it means the next split moves this vector by itself.
#
# Banned in every L0.5 crate's src/**:
#   - tokio::spawn, tokio::task::spawn, tokio::task::spawn_blocking
#   - std::thread::spawn
#   - tokio::spawn_local, spawn_pinned
#
# Exit codes:
#   0 -- GREEN (no violations)
#   2 -- RED (at least one violation)
#
# See: ADR-014 sealed kernel traits, ADR-016 cancellation, crate-layer-registry.md.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# The L0.5 set, straight from the layer metadata that declares it.
l05_crates() {
  awk '
    /^\[workspace\.metadata\.diamond\]/ { in_section = 1; next }
    /^\[/ && in_section { in_section = 0 }
    in_section && /^layers\./ {
      gsub(/^layers\./, "")
      gsub(/ /, "")
      gsub(/"/, "")
      print
    }
  ' Cargo.toml | grep '=L0\.5$' | cut -d= -f1
}

TARGETS=()
while IFS= read -r crate; do
  [ -z "$crate" ] && continue
  [ -d "crates/$crate/src" ] && TARGETS+=("crates/$crate/src")
done < <(l05_crates)

# Fail CLOSED on an empty harvest. `WARN … vector skipped` + exit 0 is how
# this vector would have greeted a rename of the whole layer: silently, in
# green. A vector that cannot find its subject has not cleared it.
if [ "${#TARGETS[@]}" -eq 0 ]; then
  echo "RED: no L0.5 crate sources found — refusing to report a verdict" >&2
  echo "  (expected layers.<crate> = \"L0.5\" rows in Cargo.toml with a crates/<crate>/src)" >&2
  exit 2
fi

# Forbidden spawn primitives. Each is a runtime-binding construct.
FORBIDDEN_PATTERNS=(
  'tokio::spawn'
  'tokio::task::spawn'
  'tokio::task::spawn_blocking'
  'tokio::task::spawn_local'
  'tokio::task::spawn_pinned'
  'tokio_util::task::spawn'
  'std::thread::spawn'
  'thread::spawn'
  'rayon::spawn'
  'smol::spawn'
  'async_std::task::spawn'
)

violations=0
violation_log=""

for pat in "${FORBIDDEN_PATTERNS[@]}"; do
  # Fixed-string search (-F), then strip line comments.
  matches=$(grep -rFn "$pat" "${TARGETS[@]}" 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*//' \
    || true)
  if [ -n "$matches" ]; then
    violations=$((violations + 1))
    violation_log+="\n  Forbidden spawn primitive '$pat':\n"
    violation_log+="$(echo "$matches" | head -5 | sed 's/^/    /')\n"
  fi
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d spawn-primitive violation(s) in L0.5 src/:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: nika-kernel is L0.5 runtime-agnostic. Spawn decisions live at"
  echo "L3 (nika-runtime / nika-daemon). If the kernel needs to describe a"
  echo "concurrent operation, expose a trait method returning Future or"
  echo "Stream — let the implementor pick the runtime."
  echo "See: ADR-014 (sealed kernel traits), ADR-016 (cancellation)."
  exit 2
fi

scanned="$(printf '%s\n' "${TARGETS[@]}" | sed 's|^crates/||; s|/src$||' | tr '\n' ' ')"
echo "OK: no spawn primitives across ${#TARGETS[@]} L0.5 crate(s) · ${scanned% }"
exit 0
