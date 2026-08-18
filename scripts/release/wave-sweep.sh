#!/usr/bin/env bash
# wave-sweep.sh — the whole version sweep for a release wave, one command.
#
#   bash scripts/release/wave-sweep.sh 0.102.0            # apply (a release wave)
#   bash scripts/release/wave-sweep.sh 0.102.0 --dry      # show, touch nothing
#   bash scripts/release/wave-sweep.sh 0.109.0-dev --dev  # main opens a NEXT train:
#                                                         # every carrier, NO changelog fold
#
# A version names ONE behavior family. When main diverges from the last
# published tag (the 0.109 opening: the language changed under a Cargo
# that still read 0.108.0 · 2026-08-18) the workspace moves to
# `<next>.0-dev` — a real semver prerelease, so `nika --version`, the
# trace's engine_version and every path-dep pin identify the next train,
# never the shipped one. `--dev` runs the same carriers minus step 5: a
# dev version has no release heading, the fold happens on the rc/final
# sweep (`0.109.0-rc.1` · `0.109.0`), which this script accepts too.
#
# PATTERN-anchored on purpose: the v0.101.0 sweep anchored on the exact
# previous number and silently missed everything a concurrent session had
# bumped in between (the kit trio sat at 0.100.1 · three follow-up fixes).
# Each carrier below is matched by ROLE — whatever number it holds today
# becomes the wave number.
#
# Carriers: root+crate manifests · both locks (cargo update -w) · the
# Dockerfile teaching comment · live status rows (ROADMAP + .claude/CLAUDE.md)
# · the kit trio + native kit CHANGELOG heading insert · the engine
# CHANGELOG Unreleased fold. Frozen surfaces (crate-spec admission facts ·
# doctor/banner test fixtures · .pre-commit-hooks marker · docs/) are never
# touched.
set -euo pipefail
cd "$(dirname "$0")/../.."

VER="${1:?usage: wave-sweep.sh <new-version> [--dry|--dev]}"
MODE="${2:-}"
DRY=""
DEV=""
case "$MODE" in
  "") ;;
  --dry) DRY=--dry ;;
  --dev) DEV=--dev ;;
  *)
    echo "wave-sweep: unknown mode: $MODE (--dry | --dev)" >&2
    exit 2
    ;;
esac
# A semver with an optional prerelease (`-dev` · `-rc.1`) — the exact
# spelling Cargo, `nika --version` and the release tag share.
[[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || {
  echo "wave-sweep: not a semver: $VER" >&2
  exit 2
}
if [ -n "$DEV" ] && [[ ! "$VER" =~ -dev$ ]]; then
  echo "wave-sweep: --dev takes a <x.y.z>-dev version, got $VER" >&2
  exit 2
fi
if [ -z "$DEV" ] && [[ "$VER" =~ -dev$ ]]; then
  echo "wave-sweep: a -dev version is not a release · pass --dev (no changelog fold)" >&2
  exit 2
fi
# The carrier pattern matches whatever the tree holds today, prerelease
# or not — the sweep from 0.109.0-dev to 0.109.0-rc.1 is the same command.
V='[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.]+)?'
TODAY="$(date +%Y-%m-%d)"

run() { if [ "$DRY" = "--dry" ]; then echo "DRY · $*"; else "$@"; fi; }

# 1 · workspace + crate manifests (any literal version = the workspace's)
run perl -pi -e "s/^(version\s*=\s*)\"$V\"/\${1}\"$VER\"/" Cargo.toml crates/*/Cargo.toml
run perl -pi -e "s/(version = )\"$V\"/\${1}\"$VER\"/g" crates/*/Cargo.toml

# 2 · locks — never hand-edited
if [ "$DRY" != "--dry" ]; then
  cargo update --workspace -q
  cargo update --workspace --manifest-path fuzz/Cargo.toml -q
else
  echo "DRY · cargo update --workspace (both locks)"
fi

# 3 · teaching comment + live status rows. The Dockerfile comment teaches
#     a RELEASE-tarball download — a stable-consumer surface: it names the
#     newest PUBLISHED version and moves only on a release sweep, never on
#     the dev opening (a `-dev` has no asset to download).
if [ -z "$DEV" ]; then
  run perl -pi -e "s/v=$V/v=$VER/" Dockerfile
fi
run perl -pi -e "s/v$V/v$VER/ if /^\| workspace/" ROADMAP.md .claude/CLAUDE.md
run perl -pi -e "s/\`chore\/release-$V\`/\`chore\/release-$VER\`/ if /^\| branch/" ROADMAP.md .claude/CLAUDE.md

# 4 · the kit manifests (WHATEVER number they hold — the concurrent-session
#     law): the portable Agent Plugins manifest at the kit root + every
#     client-native `.<client>-plugin/plugin.json`. Globbed, not listed:
#     the portable manifest arrived 2026-08-07 and the hand list here
#     missed it until hygiene vector 47 caught the sweep (2026-08-18).
run perl -pi -e "s/(\"version\": )\"$V\"/\${1}\"$VER\"/" \
  .agents/plugins/nika/plugin.json \
  .agents/plugins/nika/.*-plugin/plugin.json

# 5 · changelog folds — engine Unreleased + kit heading, both ABOVE the
#     NEWEST existing heading (never anchored to a specific old version).
#     Skipped on --dev: the next train has no heading until its rc.
if [ -n "$DEV" ]; then
  echo "dev · no changelog fold (the heading lands on the rc/final sweep)"
elif [ "$DRY" != "--dry" ]; then
  python3 - "$VER" "$TODAY" <<'PY'
import io, re, sys
ver, today = sys.argv[1], sys.argv[2]

p = 'CHANGELOG.md'
s = io.open(p, encoding='utf-8').read()
prev = re.search(r'^## \[(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)\]', s, re.M)
assert prev, 'no previous release heading'
old = '## [Unreleased]\n'
new = (f'## [Unreleased]\n\n## [{ver}](https://github.com/supernovae-st/nika/'
       f'compare/v{prev.group(1)}..v{ver}) - {today}\n')
assert s.count(old) == 1
io.open(p, 'w', encoding='utf-8').write(s.replace(old, new, 1))

k = '.agents/plugins/nika/CHANGELOG.md'
s = io.open(k, encoding='utf-8').read()
m = re.search(r'^## \d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)? — \d{4}-\d{2}-\d{2}$', s, re.M)
assert m, 'no kit heading'
entry = (f'## {ver} — {today}\n\nLockstep on the engine wave.\n\n')
io.open(k, 'w', encoding='utf-8').write(s[:m.start()] + entry + s[m.start():])
PY
else
  echo "DRY · fold CHANGELOG.md + kit CHANGELOG heading ($VER · $TODAY)"
fi

# 6 · uniformity guard — the workspace must read ONE version or the sweep fails
if [ "$DRY" != "--dry" ]; then
  UNIFORM=$(cargo metadata --no-deps --format-version 1 2>/dev/null \
    | python3 -c "import json,sys; vs={p['version'] for p in json.load(sys.stdin)['packages']}; print(' '.join(sorted(vs)))")
  [ "$UNIFORM" = "$VER" ] || {
    echo "wave-sweep: workspace not uniform: $UNIFORM" >&2
    exit 1
  }
  if [ -n "$DEV" ]; then
    echo "wave-sweep: workspace uniformly $VER · locks regenerated · no fold (dev)"
    echo "next: commit → main identifies the next train (no tag · the rc sweep folds the heading)"
  else
    echo "wave-sweep: workspace uniformly $VER · locks regenerated · folds done"
    echo "next: commit → tag v$VER → the train (funnel + trust battery gate the tarballs)"
  fi
fi
