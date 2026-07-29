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

if nika check "$FIX/permits-escape.nika.yaml" >/dev/null 2>&1; then
  say "✖ permits-escape fixture PASSES check — the permits-audit asset lies"
  fail=1
else
  say "✔ permits-escape fixture fails check (as shown)"
fi

if nika check "$FIX/broken-release-notes.nika.yaml" >/dev/null 2>&1; then
  say "✖ broken-release-notes fixture PASSES check — the full-loop asset lies"
  fail=1
else
  say "✔ broken-release-notes fixture fails check (as shown)"
fi

for wf in "$FIX/fixed-pr-review.nika.yaml" "$FIX/meeting-actions.nika.yaml" \
  "$FIX/permits-fits.nika.yaml" "$FIX/recover-fallback.nika.yaml" \
  "$FIX/fixed-release-notes.nika.yaml" \
  "crates/nika-pack/pack/examples/pr-review-fanout.nika.yaml"; do
  if nika check "$wf" >/dev/null 2>&1; then
    say "✔ $(basename "$wf") clean (as shown)"
  else
    say "✖ $(basename "$wf") FAILS check but is shown as clean"
    fail=1
  fi
done

# ── existence ───────────────────────────────────────────────────────────
required=(
  media/gifs/full-loop.optimized.gif
  media/gifs/static-check-fix.optimized.gif
  media/gifs/chat-to-workflow.optimized.gif
  media/gifs/dag-execution.optimized.gif
  media/gifs/editor-diagnostics.optimized.gif
  media/gifs/permits-audit.optimized.gif
  media/gifs/on-error-recover.optimized.gif
  media/videos/static-check-fix.mp4
  media/videos/chat-to-workflow.mp4
  media/videos/dag-execution.mp4
  media/videos/permits-audit.mp4
  media/videos/on-error-recover.mp4
  media/videos/static-check-fix.webm
  media/videos/chat-to-workflow.webm
  media/videos/dag-execution.webm
  media/videos/editor-diagnostics.mp4
  media/videos/editor-diagnostics.webm
  media/videos/permits-audit.webm
  media/videos/on-error-recover.webm
  media/posters/static-check-fix.png
  media/posters/chat-to-workflow.png
  media/posters/dag-execution.png
  media/posters/editor-diagnostics.png
  media/posters/permits-audit.png
  media/posters/on-error-recover.png
  media/raw/transcripts.json
)
for f in "${required[@]}"; do
  if [ -f "$f" ]; then say "✔ $f"; else
    say "✖ missing $f"
    fail=1
  fi
done

# ── drawn-YAML honesty (the scenes may not speak a dead grammar) ────────
# The motion scenes hand-draw YAML in span markup; this is where media
# drifts from the language (the July class: list-form `- id:` tasks · a
# scalar `workflow:` · a filecard with no envelope · `url:`/`path:` naked
# on invoke instead of under `args:`). Strip the tags and judge the text.
if python3 - <<'PY'; then
import pathlib
import re
import sys

bad = 0
for p in sorted(pathlib.Path("scripts/media/motion").glob("*.html")):
    t = p.read_text(encoding="utf-8")
    for m in re.finditer(r'<div class="yaml">(.*?)</div>', t, re.S):
        text = re.sub(r"<[^>]+>", "", m.group(1))
        if re.search(r"-\s*id\s*:", text):
            print(f" x {p.name}: dead list-form '- id:' in drawn YAML")
            bad = 1
        if re.search(r'invoke\s*:\s*\{(?![^}]*\bargs\s*:)[^}]*\b(url|path|pattern)\s*:', text):
            print(f" x {p.name}: invoke arg outside 'args:' in drawn YAML")
            bad = 1
    # A titled filecard that DRAWS yaml must draw the envelope; a titled
    # card showing terminal output (og-card) has no yaml body to judge.
    if '<div class="yaml">' in t and re.search(r'class="title">[^<]*\.nika\.yaml', t):
        body = re.sub(r"<[^>]+>", "", t)
        if "nika: v1" not in body or not re.search(r"workflow\s*:", body):
            print(f" x {p.name}: filecard misses the envelope (nika: v1 + workflow:)")
            bad = 1
sys.exit(bad)
PY
  say "✔ drawn YAML speaks the released grammar (no dead forms · envelopes present)"
else
  say "✖ drawn-YAML honesty failed"
  fail=1
fi

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
