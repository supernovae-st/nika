#!/usr/bin/env bash
# Prove a release is the greatest public stable SemVer before moving a pointer.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <sha>" >&2
  exit 64
fi

repo="$1"
release_id="$2"
tag="$3"
sha="$4"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
state="$(bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha")"
[ "$(printf '%s\n' "$state" | sed -n 's/^draft=//p')" = false ] || {
  echo "floating pointer: release is still draft" >&2
  exit 73
}
[ "$(printf '%s\n' "$state" | sed -n 's/^prerelease=//p')" = false ] || {
  echo "floating pointer: prereleases cannot move stable pointers" >&2
  exit 73
}

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
gh api "repos/${repo}/releases" --paginate \
  --jq '.[] | select(.draft == false and .prerelease == false) | [.id, .tag_name] | @tsv' \
  >"$scratch/releases"
: >"$scratch/stable"
while IFS=$'\t' read -r candidate_id candidate_tag; do
  [ -n "$candidate_id" ] || continue
  if [[ ! "$candidate_tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    echo "floating pointer: REFUSED non-canonical public stable tag ${candidate_tag}" >&2
    exit 73
  fi
  printf '%s\t%s\n' "$candidate_tag" "$candidate_id" >>"$scratch/stable"
done <"$scratch/releases"
[ -s "$scratch/stable" ] || {
  echo "floating pointer: no public stable releases found" >&2
  exit 73
}
newest="$(LC_ALL=C sort -t $'\t' -k1,1V "$scratch/stable" | tail -1)"
newest_tag="${newest%%$'\t'*}"
newest_id="${newest#*$'\t'}"
[ "$newest_tag" = "$tag" ] && [ "$newest_id" = "$release_id" ] || {
  echo "floating pointer: REFUSED downgrade from newest ${newest_tag} (${newest_id}) to ${tag} (${release_id})" >&2
  exit 73
}
bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" >/dev/null
