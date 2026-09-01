#!/usr/bin/env bash
# Verify five GitHub attestations against the exact tag, commit, repository,
# and workflow identity. This is used on both first publication and replay.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <tag> <sha> <owner/repo> <asset-dir>" >&2
  exit 64
fi
tag="$1"
sha="$2"
repo="$3"
asset_dir="$4"
version="${tag#v}"
assets=(
  "nika-macos-arm64-${version}.tar.gz"
  "nika-macos-x64-${version}.tar.gz"
  "nika-linux-arm64-${version}.tar.gz"
  "nika-linux-x64-${version}.tar.gz"
  "supernovae-st-nika-check-wasm-${version}.tgz"
)
for name in "${assets[@]}"; do
  test -f "$asset_dir/$name"
  gh attestation verify "$asset_dir/$name" \
    --repo "$repo" \
    --source-ref "refs/tags/${tag}" \
    --source-digest "$sha" \
    --signer-workflow "${repo}/.github/workflows/release.yml" >/dev/null
done
