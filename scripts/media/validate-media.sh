#!/usr/bin/env bash
# validate-media.sh — the media honesty + budget gate.
#
# 1. Every workflow shown in a media asset passes (or fails) `nika check`
#    exactly as the asset claims.
# 2. Every required export exists, including the four exports of every
#    clip in motion/intent-to-proof/clips/.
# 3. README GIFs stay under the 8 MB budget; posters under 1 MB.
# 4. No export predates what it is drawn from: an HTML scene's GIF by commit
#    time, a clip's media by the source recorded in media/clip-sources.json.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIX="scripts/media/fixtures"
fail=0

say() { printf ' %s\n' "$*"; }

# ── claim checks ────────────────────────────────────────────────────────
if nika check "$FIX/broken-pr-review.nika" >/dev/null 2>&1; then
  say "✖ broken-pr-review fixture PASSES check — the static-check-fix asset lies"
  fail=1
else
  say "✔ broken fixture fails check (as shown)"
fi

if nika check "$FIX/permits-escape.nika" >/dev/null 2>&1; then
  say "✖ permits-escape fixture PASSES check — the permits-audit asset lies"
  fail=1
else
  say "✔ permits-escape fixture fails check (as shown)"
fi

if nika check "$FIX/release-notes-draft.nika" >/dev/null 2>&1; then
  say "✖ release-notes-draft fixture PASSES check — the agent-plugin asset lies"
  fail=1
else
  say "✔ release-notes-draft fixture fails check (as shown)"
fi

for wf in "$FIX/fixed-pr-review.nika" "$FIX/meeting-actions.nika" \
  "$FIX/permits-fits.nika" "$FIX/recover-fallback.nika" \
  "$FIX/invoice-payments.nika" "$FIX/release-notes.nika" "$FIX/ship-notes.nika" \
  "$FIX/cost-unbounded.nika" "$FIX/cost-ceiling.nika" "$FIX/gated-ship.nika" \
  "crates/nika-pack/pack/examples/pr-review-fanout.nika"; do
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
  media/videos/intent-dag-proof.mp4
  media/videos/intent-dag-proof.webm
  media/posters/intent-dag-proof.png
  media/storyboards/intent-dag-proof.png
  media/raw/transcripts.json
  scripts/media/motion/intent-dag-proof.storyboard.md
  media/videos/intent-to-proof.mp4
  media/gifs/intent-to-proof.optimized.gif
  media/nika-hero.gif
  media/clip-sources.json
  media/social/github-social-preview-1280x640.png
  media/social/og-card-1600x900.png
  media/social/check-before-run-1600x900.png
  media/social/chat-vs-keeping-1600x900.png
  media/posters/intent-to-proof.png
  media/storyboards/intent-to-proof.png
  scripts/media/motion/intent-to-proof/README.md
)
# Every clip (motion/intent-to-proof/clips/<name>.mjs) ships four exports:
# the README GIF, the MP4 and WebM, and the poster. The list follows the
# clips, so a new clip cannot forget one.
for clip_file in scripts/media/motion/intent-to-proof/clips/*.mjs; do
  clip="$(basename "$clip_file" .mjs)"
  [ "$clip" = kit ] && continue
  required+=("media/gifs/$clip.optimized.gif" "media/videos/$clip.mp4"
    "media/videos/$clip.webm" "media/posters/$clip.png")
done
for f in "${required[@]}"; do
  if [ -f "$f" ]; then say "✔ $f"; else
    say "✖ missing $f"
    fail=1
  fi
done

# Other repositories embed media/nika-hero.gif by URL (the name is the
# API); it is the nika-hero clip's GIF, copied by the renderer, never a
# second painting that can drift.
if cmp -s media/nika-hero.gif media/gifs/nika-hero.optimized.gif; then
  say "✔ hotlinked media/nika-hero.gif is the nika-hero clip"
else
  say "✖ media/nika-hero.gif differs from media/gifs/nika-hero.optimized.gif"
  fail=1
fi

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
# The architecture film is cut to one 120 BPM clock: exactly 15 bars.
if command -v ffprobe >/dev/null 2>&1 \
  && [ "$(ffprobe -v error -show_entries format=duration -of csv=p=0 media/videos/intent-to-proof.mp4)" = "30.000000" ]; then
  say "✔ architecture film MP4 is exactly 30 seconds"
else
  say "✖ architecture film duration must be 30 seconds (ffprobe required)"
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
import hashlib
import json
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
    """Is `slug` a door bare `nika try` names? (rows read `<slug>.nika`)"""
    return f"{slug}.nika" in REGISTER


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
front_door = ["nika compile", "nika check", "nika run", "nika trace verify"]
positions = [readme.find(command) for command in front_door]
if any(position < 0 for position in positions) or positions != sorted(positions):
    print(" x README.md: front door must teach compile → check → run → trace verify")
    bad = 1
if re.search(r"\bnika try\b", readme):
    print(" x README.md: showroom `nika try` leaked into the ownership path")
    bad = 1

# The gallery clip draws and counts what the captured `nika try` listing
# says (never typed free-hand), so the capture must still be what this
# binary prints.
listing = subprocess.run(["nika", "try", "--color", "never"], capture_output=True, text=True)
captured = pathlib.Path("media/raw/try-gallery.txt").read_text(encoding="utf-8")
if listing.returncode != 0 or listing.stdout != captured:
    print(" x media/raw/try-gallery.txt differs from `nika try` — recapture, then re-render workflow-gallery")
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
# A clip (motion/intent-to-proof/clips/<name>.mjs) renders the media of the
# same name, and the renderer records the sha256 of the clip file it drew
# them from in media/clip-sources.json. Judged by content, not commit time:
# a re-render that changes no pixel commits no media, but it still records
# the source it was drawn from. Code the clips share (the kit, the engine)
# changes renders too; whoever changes it re-renders the clips it touches.
record = pathlib.Path("media/clip-sources.json")
drawn = json.loads(record.read_text(encoding="utf-8")) if record.exists() else {}
for p in sorted(pathlib.Path("scripts/media/motion/intent-to-proof/clips").glob("*.mjs")):
    if p.stem == "kit":
        continue
    if drawn.get(p.stem) != "sha256:" + hashlib.sha256(p.read_bytes()).hexdigest():
        print(f" x {p.stem}: its media were not rendered from {p.name} as it reads now — re-render owed")
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
for gif in media/gifs/*.gif media/nika-hero.gif; do
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
