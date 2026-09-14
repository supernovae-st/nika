#!/usr/bin/env bash
# Bump the Homebrew formula to <version> from the digests recorded in
# <artifacts-dir>: the published `SHA256SUMS` manifest (the release payload the
# workflow hands this script, and what `gh release download` fetches), or the
# per-build `nika-<platform>-<version>.tar.gz.sha256` sidecars as a fallback.
# The payload never carries the sidecars: v0.119.0 published, then its
# Homebrew leg died on `missing checksum … .tar.gz.sha256` (2026-09-13).
#
# Used by .github/workflows/release.yml AND runnable by hand against a checked-
# out tap when the workflow's tap deploy key is absent:
#
#   gh release download v0.90.0 --repo supernovae-st/nika --dir /tmp/rel
#   scripts/release/update-formula.sh \
#     ../homebrew/Formula/nika.rb 0.90.0 /tmp/rel
#
# The url lines carry `#{version}` (brew interpolates them) so only the
# `version` line and the four `sha256` lines change — each sha matched to the
# `url` line above it by platform. When the tarball travels with its digest,
# the bytes must agree before the tap points at them.
set -euo pipefail

formula="${1:?usage: update-formula.sh <formula.rb> <version> <artifacts-dir>}"
version="${2:?missing version}"
artifacts="${3:?missing artifacts dir}"

digest_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

sha_for() {
  local platform="$1" sha=""
  local asset="nika-${platform}-${version}.tar.gz"
  local manifest="${artifacts}/SHA256SUMS"
  local sidecar="${artifacts}/${asset}.sha256"
  if [ -f "$manifest" ]; then
    sha="$(awk -v a="$asset" '{ n = $2; sub(/^\*/, "", n) } n == a { print $1; exit }' "$manifest")"
    [ -n "$sha" ] || {
      echo "update-formula: SHA256SUMS carries no entry for $asset" >&2
      exit 1
    }
  elif [ -f "$sidecar" ]; then
    sha="$(awk '{ print $1; exit }' "$sidecar")"
  else
    echo "update-formula: no SHA256SUMS or ${asset}.sha256 in $artifacts" >&2
    exit 1
  fi
  [[ "$sha" =~ ^[0-9a-f]{64}$ ]] || {
    echo "update-formula: malformed digest for $asset" >&2
    exit 1
  }
  if [ -f "${artifacts}/${asset}" ] && [ "$(digest_of "${artifacts}/${asset}")" != "$sha" ]; then
    echo "update-formula: $asset does not match its recorded digest" >&2
    exit 1
  fi
  printf '%s\n' "$sha"
}

s_ma="$(sha_for macos-arm64)"
s_mx="$(sha_for macos-x64)"
s_la="$(sha_for linux-arm64)"
s_lx="$(sha_for linux-x64)"

tmp="$(mktemp)"
awk -v ver="$version" -v s_ma="$s_ma" -v s_mx="$s_mx" -v s_la="$s_la" -v s_lx="$s_lx" '
  /^[[:space:]]*version "/ { sub(/"[^"]*"/, "\"" ver "\""); print; next }
  /url ".*nika-macos-arm64-/ { plat = "ma" }
  /url ".*nika-macos-x64-/   { plat = "mx" }
  /url ".*nika-linux-arm64-/ { plat = "la" }
  /url ".*nika-linux-x64-/   { plat = "lx" }
  /sha256 "/ && plat != "" {
    sha = (plat == "ma" ? s_ma : (plat == "mx" ? s_mx : (plat == "la" ? s_la : s_lx)))
    sub(/"[0-9a-f]*"/, "\"" sha "\"")
    plat = ""
    print
    next
  }
  { print }
' "$formula" >"$tmp"
mv "$tmp" "$formula"
echo "update-formula: $formula → $version"
