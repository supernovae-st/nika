#!/usr/bin/env bash
# render-tape.sh — render a VHS tape against the real binary in a staged workdir.
#
# Usage: bash scripts/media/render-tape.sh [tape-name]   (default: full-loop)
#
# Stages a throwaway git repo (the commits the demo workflow reads), copies the
# BROKEN fixture in under the canonical name, runs vhs, optimizes with gifsicle
# when available, and lands the result at media/gifs/<name>.optimized.gif.
# The honesty contract stays with validate-media.sh: run it after this.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NAME="${1:-full-loop}"
TAPE="$ROOT/scripts/media/tapes/$NAME.tape"
[ -f "$TAPE" ] || {
  echo "no tape at $TAPE" >&2
  exit 1
}
command -v vhs >/dev/null || {
  echo "vhs not installed (brew install vhs)" >&2
  exit 1
}
command -v nika >/dev/null || {
  echo "nika not on PATH" >&2
  exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The staged history the workflow's `git log` / `git shortlog` read.
git -C "$WORK" init -q -b main
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "feat: streaming token meter in run render"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "fix: retry ladder respects Retry-After caps"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "feat: typed outputs for infer tasks"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "fix: trace replay off-by-one on wave close"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "feat: least-privilege permits inference"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "chore: pin provider catalog 16 entries"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "feat: mermaid projector for graph command"
git -C "$WORK" -c user.email=demo@nika.sh -c user.name=maintainer commit -q --allow-empty -m "fix: atomic writes on check repairs"

cp "$ROOT/scripts/media/fixtures/broken-release-notes.nika.yaml" "$WORK/release-notes.nika.yaml"
# The hero story reads the meeting-actions pair (offline mock run · the
# transcript path mirrors the fixture's const so the demo needs zero flags).
cp "$ROOT/scripts/media/fixtures/meeting-actions.nika.yaml" "$WORK/meeting-actions.nika.yaml"
mkdir -p "$WORK/scripts/media/fixtures"
cp "$ROOT/scripts/media/fixtures/sample-transcript.txt" "$WORK/scripts/media/fixtures/sample-transcript.txt"
cp "$TAPE" "$WORK/$NAME.tape"

(cd "$WORK" && vhs "$NAME.tape")

OUT="$ROOT/media/gifs/$NAME.optimized.gif"
if command -v gifsicle >/dev/null; then
  gifsicle -O3 --lossy=40 "$WORK/$NAME.gif" -o "$OUT"
else
  cp "$WORK/$NAME.gif" "$OUT"
fi
echo "→ $OUT ($(du -h "$OUT" | cut -f1))"

# The hero also installs at its hotlinked home (README + homebrew-tap +
# the city READMEs embed media/nika-hero.gif by URL — the name is the API).
if [ "$NAME" = "nika-hero" ]; then
  cp "$OUT" "$ROOT/media/nika-hero.gif"
  echo "→ $ROOT/media/nika-hero.gif (hotlinked home)"
fi
