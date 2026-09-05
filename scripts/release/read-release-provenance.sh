#!/usr/bin/env bash
# Fetch one existing statement with the draft owner's token; do not verify it.
# GitHub draft reads require push access. SLSA judges consume the run artifact
# with contents:read, never the owner's contents:write authority.
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <sha> <new-output-file>" >&2
  exit 64
fi
repo="$1"
release_id="$2"
tag="$3"
sha="$4"
output="$5"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ ! -e "$output" ] && [ ! -L "$output" ] || {
  echo 'release provenance: output already exists' >&2
  exit 73
}

read_state() {
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" >/dev/null
}
read_asset_id() {
  local ids
  ids="$(gh api "repos/${repo}/releases/${release_id}/assets" --paginate \
    --jq '.[] | select(.name == "multiple.intoto.jsonl") | .id')" || return 69
  [[ "$ids" =~ ^[1-9][0-9]*$ ]] || {
    echo 'release provenance: expected exactly one existing statement; manual replay cannot regenerate tag-context provenance' >&2
    return 73
  }
  printf '%s\n' "$ids"
}

read_state
asset_id="$(read_asset_id)"
scratch="$(mktemp "${output}.tmp.XXXXXX")"
trap 'rm -f "$scratch"' EXIT
gh api "repos/${repo}/releases/assets/${asset_id}" \
  -H 'Accept: application/octet-stream' >"$scratch"
[ -s "$scratch" ] || {
  echo 'release provenance: existing statement is empty' >&2
  exit 73
}
read_state
[ "$(read_asset_id)" = "$asset_id" ] || {
  echo 'release provenance: statement identity changed during download' >&2
  exit 73
}
# Publish the complete file without replacing any independently existing path.
# Python's link syscall has no directory-target reinterpretation or clobber.
python3 - "$scratch" "$output" <<'PY'
import os
import sys

os.link(sys.argv[1], sys.argv[2])
PY
