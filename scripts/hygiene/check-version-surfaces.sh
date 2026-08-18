#!/usr/bin/env bash
# Vector 47: every surface that spells the version says the same one.
#
# The release ceremony bumps `Cargo.toml`, and four other files carry the
# number by hand: the three plugin manifests an editor SHOWS the user,
# and the Dockerfile's install example. Nothing checked them.
#
# They drifted (2026-08-02). The v0.107.0 release commit updated all of
# them; 0.107.1 and 0.107.2 did not, so the kit told three editors it was
# 0.107.0 while the binary beside it said otherwise. Found only because a
# reverted `fuzz/Cargo.lock` sent me back through that commit's file
# list — which is not a way of finding things, it is luck.
#
# The workspace manifest is the authority. Everything else must agree.
#
# Exit: 0 GREEN · 2 RED (a surface disagrees) · 1 YELLOW (cannot read the
# authority — not a clean bill).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
cd "$ROOT" || exit 1

want="$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[^"]*')"
if [ -z "$want" ]; then
  echo "YELLOW: no workspace version in Cargo.toml — cannot compare"
  exit 1
fi

# The Dockerfile example downloads a RELEASE tarball, so it names a version
# that has one. On a `-dev` workspace (main between tags · RELEASING.md §0)
# no such asset exists for the workspace version, and the honest teaching
# is the newest published tag; on a release sweep the two coincide (the
# sweep writes the workspace version and the tag follows).
docker_want="$want"
if [[ "$want" == *-dev ]]; then
  newest_tag="$(git tag --sort=-v:refname 2>/dev/null | grep -m1 -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | cut -c2-)"
  [ -n "$newest_tag" ] && docker_want="$newest_tag"
fi

fails=0
checked=0

# <path>|<extraction>|<what it is>
surfaces=(
  ".agents/plugins/nika/plugin.json|json|the portable Agent Plugins manifest"
  ".agents/plugins/nika/.claude-plugin/plugin.json|json|the Claude Code plugin manifest"
  ".agents/plugins/nika/.codex-plugin/plugin.json|json|the Codex plugin manifest"
  ".agents/plugins/nika/.cursor-plugin/plugin.json|json|the Cursor plugin manifest"
  "Dockerfile|docker|the install example in the image header"
)

for row in "${surfaces[@]}"; do
  path="${row%%|*}"
  rest="${row#*|}"
  kind="${rest%%|*}"
  what="${rest#*|}"
  if [ ! -f "$path" ]; then
    printf 'RED: %s is gone (%s)\n' "$path" "$what" >&2
    fails=$((fails + 1))
    continue
  fi
  case "$kind" in
    json) got="$(grep -oE '"version"[[:space:]]*:[[:space:]]*"[^"]+"' "$path" | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[^"]*')" ;;
    docker) got="$(grep -oE 'v=[0-9]+\.[0-9]+\.[0-9]+[^;]*' "$path" | head -1 | cut -c3-)" ;;
    *) got="" ;;
  esac
  checked=$((checked + 1))
  expect="$want"
  [ "$kind" = docker ] && expect="$docker_want"
  if [ -z "$got" ]; then
    printf 'RED: no version found in %s (%s) — the extraction broke, which is not a pass\n' "$path" "$what" >&2
    fails=$((fails + 1))
  elif [ "$got" != "$expect" ]; then
    printf 'RED: %s says %s, the workspace says %s (%s)\n' "$path" "$got" "$expect" "$what" >&2
    fails=$((fails + 1))
  fi
done

if [ "$fails" -gt 0 ]; then
  printf '\n%d version surface(s) disagree with Cargo.toml (%s).\n' "$fails" "$want" >&2
  printf 'The release ceremony bumps these by hand — see RELEASING.md step 2.\n' >&2
  exit 2
fi

if [ "$docker_want" != "$want" ]; then
  echo "OK: $checked version surface(s) agree — manifests $want · Dockerfile teaches the newest release $docker_want (dev train)"
else
  echo "OK: $checked version surface(s) agree with the workspace ($want)"
fi
exit 0
