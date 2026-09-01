#!/usr/bin/env bash
# Cryptographically verify the generic SLSA statement, source, tag, and subjects.
set -euo pipefail

if [ "$#" -ne 7 ]; then
  echo "usage: $0 <tag> <owner/repo> <provenance> <four-native-assets...>" >&2
  exit 64
fi

tag="$1"
repo="$2"
provenance="$3"
shift 3
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$here/check-release-tag.sh" "$tag"
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
  echo "slsa barrier: invalid repository: $repo" >&2
  exit 64
}
[ -s "$provenance" ] || {
  echo "slsa barrier: provenance is missing or empty" >&2
  exit 66
}
command -v slsa-verifier >/dev/null 2>&1 || {
  echo "slsa barrier: slsa-verifier is required" >&2
  exit 69
}

version="${tag#v}"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
printf '%s\n' \
  "nika-linux-arm64-${version}.tar.gz" \
  "nika-linux-x64-${version}.tar.gz" \
  "nika-macos-arm64-${version}.tar.gz" \
  "nika-macos-x64-${version}.tar.gz" | LC_ALL=C sort >"$scratch/expected"
for asset in "$@"; do
  [ -f "$asset" ] || {
    echo "slsa barrier: missing subject: $asset" >&2
    exit 66
  }
  basename "$asset"
done | LC_ALL=C sort >"$scratch/actual"
cmp -s "$scratch/expected" "$scratch/actual" || {
  echo "slsa barrier: REFUSED subjects differ from the four native archives" >&2
  exit 73
}

slsa-verifier verify-artifact "$@" \
  --provenance-path "$provenance" \
  --source-uri "github.com/${repo}" \
  --source-tag "$tag" >/dev/null
