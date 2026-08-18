#!/usr/bin/env bash
# check-version-uniform.sh — one workspace, one version, one train.
#
# A version identifies ONE behavior family. The engine's release identity is
# the workspace version in the root Cargo.toml: `nika --version` prints it,
# every trace's `engine_version` records it, `release.yml` refuses a tag
# that does not spell it, and every crate manifest pins its path-siblings
# to it (`version = "<x>"` beside `path = "../<crate>"`).
#
# Nothing checked that they agree. `scripts/release/wave-sweep.sh` carries a
# local uniformity guard, but a hand-edited manifest, a crate added between
# sweeps with yesterday's number, or a `-dev` opening applied to some
# carriers and not others would land on main unseen — the exact silent
# drift a version exists to name (measured 2026-08-18: main read 0.108.0
# through 110 commits and a language change; the sweep to 0.109.0-dev is
# what this gate proves complete, now and at every later sweep).
#
# The rule, with denominators (never a bare OK):
#   1. every workspace member's version == the root workspace.package.version
#   2. every `path = "../…"` dependency that carries a `version = "…"` pin
#      pins THAT same string (an unpinned path dep is fine — Cargo resolves
#      it — but a pin that names another version is a lie)
#   3. both lockfiles agree for every nika-* package
#   4. the version is a semver, optionally with a prerelease (`-dev` · `-rc.1`)
#   5. every kit manifest an editor SHOWS the user (the portable Agent
#      Plugins manifest at the kit root + each `.<client>-plugin/plugin.json`)
#      spells the same version — hygiene vector 47 judges these pre-push
#      and nightly; this puts the same law on every PR, since a squash
#      merge runs nobody's local hooks
#
# Fails CLOSED: an unreadable root version, an empty member list, or a
# missing lockfile is a failure, never a pass with zero checked.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
cd "$ROOT" || exit 1

fail=0
red() {
  printf 'FAIL  %s\n' "$*" >&2
  fail=1
}

# 1 · the authority: workspace.package.version in the root manifest.
root_version="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/.*=[[:space:]]*"|".*/,""); print; exit}' Cargo.toml)"
if [ -z "$root_version" ]; then
  red "cannot read [workspace.package] version from Cargo.toml"
  exit 1
fi
if ! printf '%s' "$root_version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
  red "workspace version '$root_version' is not a semver (x.y.z or x.y.z-pre)"
fi

# 2 · every member manifest inherits it, and every pinned path-dep names it.
members=0
members_ok=0
pins=0
pins_ok=0
for m in crates/*/Cargo.toml; do
  members=$((members + 1))
  # `version.workspace = true` (the dotted inherit) or a literal `version = "…"`.
  own="$(awk '/^\[package\]/{f=1;next} /^\[/{f=0} f && /^version(\.workspace)?[[:space:]]*=/{print; exit}' "$m")"
  case "$own" in
    "") red "$m: [package] has no version line" ;;
    version.workspace*) members_ok=$((members_ok + 1)) ;;
    *)
      v="$(printf '%s' "$own" | sed -E 's/.*=[[:space:]]*"([^"]*)".*/\1/')"
      if [ "$v" = "$root_version" ]; then members_ok=$((members_ok + 1)); else red "$m: version $v (workspace is $root_version)"; fi
      ;;
  esac
  # path-sibling pins · one line each (rustfmt keeps a dep spec on one line
  # in these manifests; a multi-line spec without a version pin is fine).
  while IFS= read -r line; do
    pins=$((pins + 1))
    pv="$(printf '%s' "$line" | sed -E 's/.*version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
    if [ "$pv" = "$root_version" ]; then pins_ok=$((pins_ok + 1)); else red "$m: pin '$pv' on: $line"; fi
  done < <(grep -E 'path[[:space:]]*=[[:space:]]*"\.\./' "$m" | grep -E 'version[[:space:]]*=[[:space:]]*"' || true)
done
if [ "$members" -eq 0 ]; then
  red "no crates/*/Cargo.toml found — nothing checked"
fi

# 3 · both lockfiles record every workspace MEMBER at the workspace version
#     (member = a crates/<name> dir · the fuzz harness's own 0.0.0 crate and
#     any external `nika-*` are not the workspace's to version).
locks_ok=0
locks=0
member_names="$(printf "%s\n" crates/*/ | sed "s|crates/||; s|/$||" | tr "\n" " ")"
for lock in Cargo.lock fuzz/Cargo.lock; do
  locks=$((locks + 1))
  if [ ! -f "$lock" ]; then
    red "$lock missing"
    continue
  fi
  bad="$(awk -v want="$root_version" -v members=" $member_names " '
    /^name = "nika-/ {n=$3; gsub(/"/,"",n); if (index(members, " " n " ") == 0) n=""; next}
    /^version = / && n != "" {v=$3; gsub(/"/,"",v); if (v != want) print n" = "v; n=""}
    /^$/ {n=""}' "$lock")"
  if [ -n "$bad" ]; then red "$lock disagrees: $(printf '%s' "$bad" | tr '\n' ' ')"; else locks_ok=$((locks_ok + 1)); fi
done

# 5 · the kit manifests: globbed (the portable manifest joined the kit
#     2026-08-07 and a hand list missed it), and at least one must exist.
kits=0
kits_ok=0
for k in .agents/plugins/nika/plugin.json .agents/plugins/nika/.*-plugin/plugin.json; do
  [ -f "$k" ] || continue
  kits=$((kits + 1))
  kv="$(grep -oE '"version"[[:space:]]*:[[:space:]]*"[^"]+"' "$k" | head -1 | sed -E 's/.*"([^"]+)"$/\1/')"
  if [ "$kv" = "$root_version" ]; then kits_ok=$((kits_ok + 1)); else red "$k: version '$kv' (workspace is $root_version)"; fi
done
if [ "$kits" -eq 0 ]; then
  red "no kit manifest found under .agents/plugins/nika — nothing checked"
fi

if [ "$fail" -ne 0 ]; then
  echo "version-uniform: FAILED — workspace $root_version · members $members_ok/$members · pins $pins_ok/$pins · locks $locks_ok/$locks · kit manifests $kits_ok/$kits" >&2
  exit 1
fi
echo "version-uniform: OK — workspace $root_version · members $members_ok/$members · pins $pins_ok/$pins · locks $locks_ok/$locks · kit manifests $kits_ok/$kits"
