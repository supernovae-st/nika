#!/usr/bin/env bash
# Re-prove the mutable GitHub barrier, then commit the draft-to-public transition.
set -euo pipefail

if [ "$#" -ne 7 ]; then
  echo "usage: $0 <owner/repo> <release-id> <tag> <sha> <proven-digest> <artifacts-dir> <tap-ready>" >&2
  exit 64
fi

repo="$1"
release_id="$2"
tag="$3"
sha="$4"
digest="$5"
artifacts="$6"
tap_ready="$7"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version="${tag#v}"

case "$tap_ready" in true | false) ;; *)
  echo "release finalizer: tap readiness must be true or false" >&2
  exit 64
  ;;
esac

[ -d "$artifacts" ] || {
  echo "release finalizer: artifacts directory is missing" >&2
  exit 66
}

assets=(
  "$artifacts/nika-macos-arm64-${version}.tar.gz"
  "$artifacts/nika-macos-x64-${version}.tar.gz"
  "$artifacts/nika-linux-arm64-${version}.tar.gz"
  "$artifacts/nika-linux-x64-${version}.tar.gz"
  "$artifacts/SHA256SUMS"
  "$artifacts/multiple.intoto.jsonl"
  "$artifacts/supernovae-st-nika-check-wasm-${version}.tgz"
  "$artifacts/supernovae-st-nika-check-wasm-${version}.tgz.sha256"
)
read_state() {
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha"
}

# The read-only release-final-proof job owns immutable registry/provenance
# verification and supplies digest. This write-authority helper deliberately
# invokes no external verifier: immediately before either success path, it is
# independently authoritative for the mutable GitHub assets and release state.
bash "$here/release-assets-barrier.sh" \
  verify "$repo" "$release_id" "$tag" "$sha" "${assets[@]}"

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
printf '%s\n' \
  "nika-linux-arm64-${version}.tar.gz" \
  "nika-linux-x64-${version}.tar.gz" \
  "nika-macos-arm64-${version}.tar.gz" \
  "nika-macos-x64-${version}.tar.gz" | LC_ALL=C sort >"$scratch/expected-checksums"
awk '{ print $2 }' "$artifacts/SHA256SUMS" | LC_ALL=C sort \
  | sed 's/^\*//' >"$scratch/actual-checksums"
cmp -s "$scratch/expected-checksums" "$scratch/actual-checksums" || {
  echo "release finalizer: REFUSED checksum manifest names" >&2
  exit 73
}
(cd "$artifacts" && sha256sum -c SHA256SUMS) >/dev/null

# GitHub has no conditional release PATCH. Re-read identity, state, and marker
# immediately before the decision; an administrator can still race after this
# read, and the post-PATCH reads below detect only mutations visible afterward.
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
if [ "$prerelease" = false ] && [ "$tap_ready" != true ]; then
  echo "release finalizer: tap readiness is mandatory before a stable draft transition" >&2
  exit 77
fi
make_latest=false
if [ "$prerelease" = false ]; then
  # GitHub's documented legacy policy chooses Latest by SemVer/creation date;
  # an explicit true would let delayed recovery of an older stable regress it.
  make_latest=legacy
fi

publish_failed=false
# Omitted metadata is not proof of preservation: a body-only draft PATCH
# orphaned 0.118.5. Bind this transition to the tag and SHA just re-proved.
if ! gh api --method PATCH "repos/${repo}/releases/${release_id}" \
  -f "tag_name=$tag" -f "target_commitish=$sha" \
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
