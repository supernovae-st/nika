#!/usr/bin/env bash
# wave-sweep.sh — the whole version sweep for a release wave, one command.
#
#   bash scripts/release/wave-sweep.sh 0.102.0            # apply
#   bash scripts/release/wave-sweep.sh 0.102.0 --dry      # show, touch nothing
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

VER="${1:?usage: wave-sweep.sh <new-version> [--dry]}"
DRY="${2:-}"
[[ "$VER" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "wave-sweep: not a semver: $VER" >&2; exit 2; }
V='[0-9]+\.[0-9]+\.[0-9]+'
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

# 3 · teaching comment + live status rows
run perl -pi -e "s/v=$V/v=$VER/" Dockerfile
run perl -pi -e "s/v$V/v$VER/ if /^\| workspace/" ROADMAP.md .claude/CLAUDE.md
run perl -pi -e "s/\`chore\/release-$V\`/\`chore\/release-$VER\`/ if /^\| branch/" ROADMAP.md .claude/CLAUDE.md

# 4 · the kit trio (WHATEVER number they hold — the concurrent-session law)
run perl -pi -e "s/(\"version\": )\"$V\"/\${1}\"$VER\"/" \
  .agents/plugins/nika/.claude-plugin/plugin.json \
  .agents/plugins/nika/.codex-plugin/plugin.json \
  .agents/plugins/nika/.cursor-plugin/plugin.json

# 5 · changelog folds — engine Unreleased + kit heading, both ABOVE the
#     NEWEST existing heading (never anchored to a specific old version)
if [ "$DRY" != "--dry" ]; then
  python3 - "$VER" "$TODAY" <<'PY'
import io, re, sys
ver, today = sys.argv[1], sys.argv[2]

p = 'CHANGELOG.md'
s = io.open(p, encoding='utf-8').read()
prev = re.search(r'^## \[(\d+\.\d+\.\d+)\]', s, re.M)
assert prev, 'no previous release heading'
old = '## [Unreleased]\n'
new = (f'## [Unreleased]\n\n## [{ver}](https://github.com/supernovae-st/nika/'
       f'compare/v{prev.group(1)}..v{ver}) - {today}\n')
assert s.count(old) == 1
io.open(p, 'w', encoding='utf-8').write(s.replace(old, new, 1))

k = '.agents/plugins/nika/CHANGELOG.md'
s = io.open(k, encoding='utf-8').read()
m = re.search(r'^## \d+\.\d+\.\d+ — \d{4}-\d{2}-\d{2}$', s, re.M)
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
  [ "$UNIFORM" = "$VER" ] || { echo "wave-sweep: workspace not uniform: $UNIFORM" >&2; exit 1; }
  echo "wave-sweep: workspace uniformly $VER · locks regenerated · folds done"
  echo "next: commit → tag v$VER → the train (funnel + trust battery gate the tarballs)"
fi
