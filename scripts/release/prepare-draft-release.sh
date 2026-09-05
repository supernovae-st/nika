#!/usr/bin/env bash
# Reuse an exact existing release (draft or published, including a
# validation-only public replay), and create a new draft only when the
# release list carries no release for the tag.
#
# GitHub's by-tag endpoint (`releases/tags/<tag>`) returns PUBLISHED releases
# only: a draft that carries the tag answers 404 there. v0.118.0 died on
# exactly that (2026-09-04): this script created its draft, looked it up by
# tag, read 404, and the train stopped with four green builds and no assets.
# The release LIST carries drafts for a token with push access, so the lookup
# reads the list and matches the tag exactly. An empty list answer is the
# only absence; any failure of the list call is a barrier, never an absence.
# Creation retains the POST response's immutable id: list visibility can lag
# the committed draft (v0.118.1), and is not a read-after-write guarantee.
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

# The id of the one release (draft or published) carrying the tag, or nothing.
# The list is read whole (`--paginate`, the house shape of every list read
# here) and answered by one jq expression per page: no pager, no `head`,
# nothing that could turn a SIGPIPE into a verdict.
lookup_release_id() {
  gh api "repos/${repo}/releases" --paginate \
    --jq "[.[] | select(.tag_name == \"${tag}\")] | map(.id) | first // empty" \
    2>"$scratch/error"
}

if ! release_id="$(lookup_release_id)"; then
  echo "release barrier: release lookup failed (the release list did not answer)" >&2
  cat "$scratch/error" >&2
  exit 69
fi
if [ -n "$release_id" ]; then
  state="$(bash "$here/read-release-state.sh" \
    "$repo" "$release_id" "$tag" "$expected_sha")"
  draft="$(printf '%s\n' "$state" | sed -n 's/^draft=//p')"
  prerelease="$(printf '%s\n' "$state" | sed -n 's/^prerelease=//p')"
  printf 'id=%s\ncreated=false\ndraft=%s\nprerelease=%s\n' \
    "$release_id" "$draft" "$prerelease"
  exit 0
fi

version="${tag#v}"
prerelease=false
case "$version" in
  *-*) prerelease=true ;;
esac
# The tag was resolved before the mutation and read-release-state proves it
# again afterwards. Creation routing is pinned, but never used as the existing
# release's identity. Preserve gh's notes-prepending behavior through the API.
if ! release_id="$(gh api "repos/${repo}/releases" --method POST \
  --raw-field "tag_name=$tag" --raw-field "name=$tag" \
  --raw-field "target_commitish=$expected_sha" \
  --field draft=true --field "prerelease=$prerelease" \
  --field generate_release_notes=true --field "body=@$notes" \
  --jq '.id' 2>"$scratch/error")"; then
  echo "release barrier: draft creation failed; inspect committed state before retrying" >&2
  cat "$scratch/error" >&2
  exit 69
fi
state="$(bash "$here/read-release-state.sh" \
  "$repo" "$release_id" "$tag" "$expected_sha")"
[ "$(printf '%s\n' "$state" | sed -n 's/^draft=//p')" = true ] || {
  echo "release barrier: newly created release is not a draft" >&2
  exit 73
}
printf 'id=%s\ncreated=true\ndraft=true\nprerelease=%s\n' "$release_id" "$prerelease"
