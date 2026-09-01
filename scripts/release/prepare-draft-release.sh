#!/usr/bin/env bash
# Reuse an exact existing release (including validation-only public replay),
# and create a new draft only on an explicit 404.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <tag> <owner/repo> <notes-file> <expected-sha>" >&2
  exit 64
fi

tag="$1"
repo="$2"
notes="$3"
expected_sha="$4"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$here/check-release-tag.sh" "$tag"
test -f "$notes"
bash "$here/resolve-release-tag.sh" "$tag" "$repo" "$expected_sha" >/dev/null

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
if gh api "repos/${repo}/releases/tags/${tag}" --jq '.id' \
  >"$scratch/release" 2>"$scratch/error"; then
  release_id="$(tr -d '\r\n' <"$scratch/release")"
  state="$(bash "$here/read-release-state.sh" \
    "$repo" "$release_id" "$tag" "$expected_sha")"
  draft="$(printf '%s\n' "$state" | sed -n 's/^draft=//p')"
  prerelease="$(printf '%s\n' "$state" | sed -n 's/^prerelease=//p')"
  printf 'id=%s\ncreated=false\ndraft=%s\nprerelease=%s\n' \
    "$release_id" "$draft" "$prerelease"
  exit 0
fi

if ! grep -Fq '(HTTP 404)' "$scratch/error"; then
  echo "release barrier: release lookup failed (not an explicit HTTP 404)" >&2
  cat "$scratch/error" >&2
  exit 69
fi

version="${tag#v}"
prerelease=false
case "$version" in
  *-*) prerelease=true ;;
esac
create_args=(
  release create "$tag" --repo "$repo" --title "$tag"
  --notes-file "$notes" --generate-notes --verify-tag --draft
  --target "$expected_sha"
)
if [ "$prerelease" = true ]; then
  create_args+=(--prerelease)
fi
gh "${create_args[@]}" >/dev/null
release_id="$(gh api "repos/${repo}/releases/tags/${tag}" --jq '.id')"
state="$(bash "$here/read-release-state.sh" \
  "$repo" "$release_id" "$tag" "$expected_sha")"
[ "$(printf '%s\n' "$state" | sed -n 's/^draft=//p')" = true ] || {
  echo "release barrier: newly created release is not a draft" >&2
  exit 73
}
printf 'id=%s\ncreated=true\ndraft=true\nprerelease=%s\n' "$release_id" "$prerelease"
