#!/usr/bin/env bash
# post-commit hook — auto-fire Olympus xtask in background so the dashboard
# snapshots, graph, and hygiene JSON all reflect the new engine HEAD without
# manual reload. Runs non-blocking; commit itself always succeeds.
#
# Skips silently when the olympus sibling isn't present (e.g., engine cloned
# standalone) or when pnpm is missing. The log at .nika/post-commit-xtask.log
# captures every fire so we can audit what ran and when.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/../.." && pwd)"

OLYMPUS="$ENGINE/../../olympus"
if [ ! -d "$OLYMPUS" ]; then
  exit 0
fi
OLYMPUS="$(cd "$OLYMPUS" && pwd)"

SHA=$(git -C "$ENGINE" rev-parse --short=9 HEAD 2>/dev/null || echo "unknown")
LOGFILE="$ENGINE/.nika/post-commit-xtask.log"
mkdir -p "$(dirname "$LOGFILE")"

printf '[%s] post-commit fired for %s\n' "$(date -u +%FT%TZ)" "$SHA" >>"$LOGFILE"

if ! command -v pnpm >/dev/null 2>&1; then
  printf '  (pnpm not found — Olympus xtask skipped)\n' >>"$LOGFILE"
  exit 0
fi

(
  cd "$OLYMPUS" || exit 0
  nohup pnpm tsx scripts/xtask.ts >>"$LOGFILE" 2>&1 &
)

exit 0
