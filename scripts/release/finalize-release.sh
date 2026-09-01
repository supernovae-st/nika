#!/usr/bin/env bash
# Commit the draft-to-public transition once and accept commit-then-error replay.
set -euo pipefail

if [ "$#" -ne 5 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <sha> <ghcr-digest>" >&2
  exit 64
fi

repo="$1"
release_id="$2"
tag="$3"
sha="$4"
digest="$5"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

read_state() {
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha"
}

state="$(read_state)"
draft="$(printf '%s\n' "$state" | sed -n 's/^draft=//p')"
prerelease="$(printf '%s\n' "$state" | sed -n 's/^prerelease=//p')"
persisted="$(bash "$here/release-digest-marker.sh" read \
  "$repo" "$release_id" "$tag" "$sha")"
[ "$persisted" = "$digest" ] || {
  echo "release finalizer: REFUSED persisted digest drift" >&2
  exit 73
}

if [ "$draft" = false ]; then
  printf 'transitioned=false\n'
  exit 0
fi
if [ "$prerelease" = false ] && [ -z "${TAP_DEPLOY_KEY:-}" ]; then
  echo "release finalizer: TAP_DEPLOY_KEY is mandatory before a stable draft transition" >&2
  exit 77
fi
make_latest=false
if [ "$prerelease" = false ]; then
  # GitHub's documented legacy policy chooses Latest by SemVer/creation date;
  # an explicit true would let delayed recovery of an older stable regress it.
  make_latest=legacy
fi

publish_failed=false
if ! gh api --method PATCH "repos/${repo}/releases/${release_id}" \
  -F draft=false -f discussion_category_name=Announcements \
  -f "make_latest=${make_latest}" >/dev/null; then
  publish_failed=true
fi

state="$(read_state)"
committed_draft="$(printf '%s\n' "$state" | sed -n 's/^draft=//p')"
committed_digest="$(bash "$here/release-digest-marker.sh" read \
  "$repo" "$release_id" "$tag" "$sha")"
[ "$committed_digest" = "$digest" ] || {
  echo "release finalizer: REFUSED digest changed during publication" >&2
  exit 73
}
if [ "$committed_draft" = false ]; then
  printf 'transitioned=true\n'
  exit 0
fi
if [ "$publish_failed" = true ]; then
  echo "release finalizer: publish failed and release remains draft" >&2
else
  echo "release finalizer: publish returned success but release remains draft" >&2
fi
exit 69
