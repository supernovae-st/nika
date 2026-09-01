#!/usr/bin/env bash
# Read a GitHub release by immutable ID while repeatedly proving its tag.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <expected-sha>" >&2
  exit 64
fi

repo="$1"
release_id="$2"
tag="$3"
expected_sha="$4"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ "$release_id" =~ ^[1-9][0-9]*$ ]] || {
  echo "release state: invalid release id: $release_id" >&2
  exit 64
}
bash "$here/resolve-release-tag.sh" "$tag" "$repo" "$expected_sha" >/dev/null

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
gh api "repos/${repo}/releases/${release_id}" \
  --jq '[.id, .tag_name, .draft, .prerelease] | @tsv' >"$scratch/state"
IFS=$'\t' read -r actual_id actual_tag draft prerelease <"$scratch/state"

[ "$actual_id" = "$release_id" ] || {
  echo "release state: REFUSED id ${actual_id}; expected ${release_id}" >&2
  exit 73
}
[ "$actual_tag" = "$tag" ] || {
  echo "release state: REFUSED tag ${actual_tag}; expected ${tag}" >&2
  exit 73
}
case "$draft:$prerelease" in
  true:true | true:false | false:true | false:false) ;;
  *)
    echo "release state: REFUSED non-boolean draft/prerelease state" >&2
    exit 73
    ;;
esac
expected_prerelease=false
case "${tag#v}" in
  *-*) expected_prerelease=true ;;
esac
[ "$prerelease" = "$expected_prerelease" ] || {
  echo "release state: REFUSED prerelease=${prerelease}; expected ${expected_prerelease}" >&2
  exit 73
}

bash "$here/resolve-release-tag.sh" "$tag" "$repo" "$expected_sha" >/dev/null
printf 'id=%s\ntag=%s\ndraft=%s\nprerelease=%s\nsha=%s\n' \
  "$release_id" "$tag" "$draft" "$prerelease" "$expected_sha"
