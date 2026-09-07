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
  media/gifs/intent-to-impact.optimized.gif
  media/videos/intent-to-impact.mp4
  media/posters/intent-to-impact.png
  media/storyboards/intent-to-impact.png
  scripts/media/motion/intent-to-impact/README.md
  media/brand/nika-logomark.svg
  media/gifs/intent-dag-proof.optimized.gif
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
  media/videos/intent-dag-proof.mp4
  media/videos/intent-dag-proof.webm
  media/posters/intent-dag-proof.png
  media/posters/static-check-fix.png
  media/posters/chat-to-workflow.png
  media/posters/dag-execution.png
  media/posters/editor-diagnostics.png
  media/posters/permits-audit.png
  media/posters/on-error-recover.png
  media/storyboards/intent-dag-proof.png
  media/raw/transcripts.json
  scripts/media/motion/intent-dag-proof.storyboard.md
)
for f in "${required[@]}"; do
  if [ -f "$f" ]; then say "✔ $f"; else
    say "✖ missing $f"
    fail=1
  fi
done

# Product-film claims are tested as an illustration, not as a live workflow.
if node --test scripts/media/motion/intent-to-impact/commerce-model.test.js; then
  say "✔ product film topology, approval ordering and one-minute edit"
else
  fail=1
fi
if command -v ffprobe >/dev/null 2>&1 \
  && [ "$(ffprobe -v error -show_entries format=duration -of csv=p=0 media/videos/intent-to-impact.mp4)" = "60.000000" ]; then
  say "✔ product film MP4 is exactly 60 seconds"
else
  say "✖ product film duration must be 60 seconds (ffprobe required)"
  fail=1
fi

# ── drawn-YAML honesty (the scenes may not speak a dead grammar) ────────
# The motion scenes hand-draw YAML in span markup; this is where media
# drifts from the language (the July class: list-form `- id:` tasks · a
# scalar `workflow:` · a filecard with no envelope · `url:`/`path:` naked
# on invoke instead of under `args:`). Strip the tags and judge the text —
# the WHOLE text, every markup class: the editor scene draws its code in
# `.buf`, not `.yaml`, and a class-scoped scan let it lie for weeks.
if python3 - <<'PY'; then
import pathlib
import re
import subprocess
import sys


def _register():
    """The showroom register the RELEASED binary prints (bare `nika try`).

    Listed once. Offline, no run, no side effect — the only honest way to
    ask "does this slug resolve?" without executing someone's workflow.
    """
    r = subprocess.run(["nika", "try"], capture_output=True, text=True)
    if r.returncode != 0:
        print(" x `nika try` refused to list the register — cannot judge slugs")
        sys.exit(1)
    return r.stdout


REGISTER = _register()


def shows(slug):
    """Is `slug` a door bare `nika try` names? (rows read `<slug>.nika.yaml`)"""
    return f"{slug}.nika.yaml" in REGISTER


bad = 0
for p in sorted(pathlib.Path("scripts/media/motion").glob("*.html")):
    t = p.read_text(encoding="utf-8")
    body = re.sub(r"<script.*?</script>", "", t, flags=re.S)
    body = re.sub(r"<style.*?</style>", "", body, flags=re.S)
    body = re.sub(r"<[^>]+>", "", body)
    if re.search(r"-\s+id\s*:", body):
        print(f" x {p.name}: dead list-form '- id:' drawn somewhere in the scene")
        bad = 1
    for m in re.finditer(r'<div class="yaml">(.*?)</div>', t, re.S):
        text = re.sub(r"<[^>]+>", "", m.group(1))
        if re.search(r'invoke\s*:\s*\{(?![^}]*\bargs\s*:)[^}]*\b(url|path|pattern)\s*:', text):
            print(f" x {p.name}: invoke arg outside 'args:' in drawn YAML")
            bad = 1
    # Dead envelope anywhere a scene paints — not only titled filecards.
    # poster-keeping used `.hdr` not `.title` and taught `nika: v1` past
    # a class-scoped scan (the same hole editor-diagnostics taught in July).
    if re.search(r"^nika\s*:\s*v1\b", body, re.M) or re.search(r"^workflow\s*:", body, re.M):
        print(f" x {p.name}: draws the dead envelope (nika: v1 / workflow:) — 0.109 refuses it")
        bad = 1
    # A titled filecard that DRAWS yaml must draw the envelope; a titled
    # card showing terminal output (og-card) has no yaml body to judge.
    if '<div class="yaml">' in t and re.search(r'class="title">[^<]*\.nika\.yaml', t):
        # The nine-key envelope (0.109): the identity rides ON `nika:` as a
        # kebab-case id and `tasks:` is the type discriminant. This rule
        # used to DEMAND `nika: v1` + `workflow:` — the exact spelling the
        # engine now refuses (PARSE-005) — so a scene teaching the dead
        # envelope was the only way to pass it.
        if not re.search(r"^\s*nika\s*:\s*[a-z][a-z0-9-]*\s*$", body, re.M) \
                or not re.search(r"^\s*tasks\s*:", body, re.M):
            print(f" x {p.name}: filecard misses the nine-key envelope (nika: <kebab-id> + tasks:)")
            bad = 1
    # Every showroom slug a scene teaches must resolve on the RELEASED
    # binary — membership in the register bare `nika try` prints. Offline,
    # no run, no side effect.
    for slug in sorted(set(re.findall(r"nika try\s+([a-z0-9/_-]+)", body))):
        if not shows(slug):
            print(f" x {p.name}: teaches slug {slug!r} the released binary refuses")
            bad = 1

# The README front door is the real ownership loop. `try` is deliberately not
# taught here: it is a showroom, while the README must teach the four verbs a
# user keeps after the first minute. Pin presence AND order so another rewrite
# cannot put proof before the run or silently bring the showroom back.
readme = pathlib.Path("README.md").read_text(encoding="utf-8")
front_door = ["nika new", "nika check", "nika run", "nika trace verify"]
positions = [readme.find(command) for command in front_door]
if any(position < 0 for position in positions) or positions != sorted(positions):
    print(" x README.md: front door must teach new → check → run → trace verify")
    bad = 1
if re.search(r"\bnika try\b", readme):
    print(" x README.md: showroom `nika try` leaked into the ownership path")
    bad = 1

# The gallery's count claims derive from the released pack, never typed
# free-hand: "N ... business jobs" must equal the showcase family count.
gal = pathlib.Path("scripts/media/motion/workflow-gallery.html").read_text(encoding="utf-8")
claims = {int(n) for n in re.findall(r"(\d+)\s+(?:embedded\s+)?business jobs", gal)}
listing = subprocess.run(["nika", "examples", "list"], capture_output=True, text=True)
real = len(re.findall(r"showcase/", listing.stdout)) or len(
    [ln for ln in listing.stdout.splitlines() if re.match(r"\s*│\s+t\d-", ln)]
)
if real and claims and claims != {real}:
    print(f" x workflow-gallery.html: claims {sorted(claims)} jobs · released pack has {real}")
    bad = 1

# Freshness: a scene edit without a re-render is how a fixed source keeps
# shipping a lying gif (editor-diagnostics: source healed, gif stale since
# July 2). Judge by git commit times: the export must not predate its scene.
def last_commit(path):
    r = subprocess.run(["git", "log", "-1", "--format=%ct", "--", path],
                       capture_output=True, text=True)
    out = r.stdout.strip()
    return int(out) if out else None

def dirty(path):
    # An uncommitted export IS the fresh painting — mid-repair must pass.
    r = subprocess.run(["git", "status", "--porcelain", "--", path],
                       capture_output=True, text=True)
    return bool(r.stdout.strip())

for p in sorted(pathlib.Path("scripts/media/motion").glob("*.html")):
    scene_t = last_commit(str(p))
    gif = pathlib.Path("media/gifs") / (p.stem + ".optimized.gif")
    if scene_t is None or not gif.exists() or dirty(str(gif)):
        continue
    gif_t = last_commit(str(gif))
    if gif_t is not None and gif_t < scene_t:
        print(f" x {gif.name}: older than its scene {p.name} — re-render owed")
        bad = 1
for tape in sorted(pathlib.Path("scripts/media/tapes").glob("*.tape")):
    tape_t = last_commit(str(tape))
    gif = pathlib.Path("media/gifs") / (tape.stem + ".optimized.gif")
    if tape_t is None or not gif.exists() or dirty(str(gif)):
        continue
    gif_t = last_commit(str(gif))
    if gif_t is not None and gif_t < tape_t:
        print(f" x {gif.name}: older than its tape {tape.name} — re-render owed")
        bad = 1
sys.exit(bad)
PY
  say "✔ drawn claims honest (grammar · slugs resolve · counts derived · renders fresh)"
else
  say "✖ drawn-claims honesty failed"
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
