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
# Banned in crates/nika-kernel/src/**:
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

KERNEL_SRC="crates/nika-kernel/src"

if [ ! -d "$KERNEL_SRC" ]; then
  echo "WARN: $KERNEL_SRC not found — vector skipped"
  exit 0
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
  matches=$(grep -rFn "$pat" "$KERNEL_SRC" 2>/dev/null \
    | grep -vE ':[0-9]+:[[:space:]]*//' \
    || true)
  if [ -n "$matches" ]; then
    violations=$((violations + 1))
    violation_log+="\n  Forbidden spawn primitive '$pat':\n"
    violation_log+="$(echo "$matches" | head -5 | sed 's/^/    /')\n"
  fi
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d spawn-primitive violation(s) in nika-kernel src/:\n" "$violations"
  printf "%b\n" "$violation_log"
  echo ""
  echo "Hint: nika-kernel is L0.5 runtime-agnostic. Spawn decisions live at"
  echo "L3 (nika-runtime / nika-daemon). If the kernel needs to describe a"
  echo "concurrent operation, expose a trait method returning Future or"
  echo "Stream — let the implementor pick the runtime."
  echo "See: ADR-014 (sealed kernel traits), ADR-016 (cancellation)."
  exit 2
fi

echo "OK: no spawn primitives in nika-kernel/src/"
exit 0
