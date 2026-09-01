#!/usr/bin/env bash
# Persist one immutable GHCR digest in the release body without adding an asset.
set -euo pipefail

if [ "$#" -lt 5 ] || [ "$#" -gt 6 ]; then
  echo "usage: $0 <read|stage> <owner/repo> <release-id> <tag> <sha> [digest]" >&2
  exit 64
fi

mode="$1"
repo="$2"
release_id="$3"
tag="$4"
sha="$5"
candidate="${6:-}"
case "$mode" in
  read) [ "$#" -eq 5 ] ;;
  stage) [ "$#" -eq 6 ] ;;
  *)
    echo "release digest: invalid mode: $mode" >&2
    exit 64
    ;;
esac
if [ -n "$candidate" ] && [[ ! "$candidate" =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "release digest: invalid candidate digest" >&2
  exit 64
fi

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
marker_prefix='<!-- nika-ghcr-digest: '

read_marker() {
  local body_file="$1"
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" >/dev/null
  gh api "repos/${repo}/releases/${release_id}" --jq '.body // ""' >"$body_file"
  grep -E '^<!-- nika-ghcr-digest: sha256:[0-9a-f]{64} -->$' "$body_file" \
    >"${body_file}.markers" || true
  count="$(wc -l <"${body_file}.markers" | tr -d ' ')"
  [ "$count" -le 1 ] || {
    echo "release digest: REFUSED duplicate digest markers" >&2
    return 73
  }
  if [ "$count" -eq 0 ]; then
    return 44
  fi
  sed -n 's/^<!-- nika-ghcr-digest: \(sha256:[0-9a-f]\{64\}\) -->$/\1/p' \
    "${body_file}.markers"
}

state=0
occupied="$(read_marker "$scratch/body")" || state=$?
if [ "$state" -eq 0 ]; then
  if [ -n "$candidate" ] && [ "$occupied" != "$candidate" ]; then
    echo "release digest: REFUSED marker drift; have ${occupied}, candidate ${candidate}" >&2
    exit 73
  fi
  printf '%s\n' "$occupied"
  exit 0
fi
[ "$state" -eq 44 ] || exit "$state"
[ "$mode" = stage ] || {
  echo "release digest: marker is absent" >&2
  exit 44
}

cp "$scratch/body" "$scratch/updated"
if [ -s "$scratch/updated" ]; then
  printf '\n' >>"$scratch/updated"
fi
printf '%s%s -->\n' "$marker_prefix" "$candidate" >>"$scratch/updated"
patch_failed=false
if ! gh api --method PATCH "repos/${repo}/releases/${release_id}" \
  -f "body=$(cat "$scratch/updated")" >/dev/null; then
  patch_failed=true
fi

state=0
committed="$(read_marker "$scratch/committed")" || state=$?
[ "$state" -eq 0 ] || {
  [ "$patch_failed" = false ] \
    && echo "release digest: marker write returned success but did not commit" >&2 \
    || echo "release digest: marker write failed and did not commit" >&2
  exit 69
}
[ "$committed" = "$candidate" ] || {
  echo "release digest: REFUSED concurrently committed divergent marker" >&2
  exit 73
}
printf '%s\n' "$committed"
