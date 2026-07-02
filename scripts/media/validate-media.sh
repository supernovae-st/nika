#!/usr/bin/env bash
# validate-media.sh — the media honesty + budget gate.
#
# 1. Every workflow shown in a media asset passes (or fails) `nika check`
#    exactly as the asset claims.
# 2. Every required export exists.
# 3. README GIFs stay under the 8 MB budget; posters under 1 MB.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIX="scripts/media/fixtures"
fail=0

say() { printf ' %s\n' "$*"; }

# ── claim checks ────────────────────────────────────────────────────────
if nika check "$FIX/broken-pr-review.nika.yaml" >/dev/null 2>&1; then
  say "✖ broken-pr-review fixture PASSES check — the static-check-fix asset lies"
  fail=1
else
  say "✔ broken fixture fails check (as shown)"
fi

for wf in "$FIX/fixed-pr-review.nika.yaml" "$FIX/meeting-actions.nika.yaml" \
  "crates/nika-pack/pack/examples/showcase/t3-pr-review-fanout.nika.yaml"; do
  if nika check "$wf" >/dev/null 2>&1; then
    say "✔ $(basename "$wf") clean (as shown)"
  else
    say "✖ $(basename "$wf") FAILS check but is shown as clean"
    fail=1
  fi
done

# ── existence ───────────────────────────────────────────────────────────
required=(
  media/gifs/static-check-fix.optimized.gif
  media/gifs/chat-to-workflow.optimized.gif
  media/gifs/dag-execution.optimized.gif
  media/videos/static-check-fix.mp4
  media/videos/chat-to-workflow.mp4
  media/videos/dag-execution.mp4
  media/videos/static-check-fix.webm
  media/videos/chat-to-workflow.webm
  media/videos/dag-execution.webm
  media/posters/static-check-fix.png
  media/posters/chat-to-workflow.png
  media/posters/dag-execution.png
  media/raw/transcripts.json
)
for f in "${required[@]}"; do
  if [ -f "$f" ]; then say "✔ $f"; else
    say "✖ missing $f"
    fail=1
  fi
done

# ── budgets ─────────────────────────────────────────────────────────────
max_gif=$((8 * 1024 * 1024))
for gif in media/gifs/*.gif; do
  size=$(wc -c <"$gif")
  if [ "$size" -gt "$max_gif" ]; then
    say "✖ GIF over 8MB: $gif ($((size / 1024 / 1024))MB)"
    fail=1
  fi
done
max_poster=$((1024 * 1024))
for png in media/posters/*.png; do
  size=$(wc -c <"$png")
  if [ "$size" -gt "$max_poster" ]; then
    say "✖ poster over 1MB: $png"
    fail=1
  fi
done

if [ "$fail" -eq 0 ]; then
  say "✔ media validation clean"
else
  say "✖ media validation failed"
fi
exit "$fail"
