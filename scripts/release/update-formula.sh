#!/usr/bin/env bash
# Bump the Homebrew formula to <version> using the sha256 sidecar files of the
# release tarballs in <artifacts-dir> (`nika-<platform>-<version>.tar.gz.sha256`).
#
# Used by .github/workflows/release.yml AND runnable by hand against a checked-
# out tap when the workflow's TAP_GITHUB_TOKEN is absent:
#
#   gh release download v0.90.0 --repo supernovae-st/nika --dir /tmp/rel
#   scripts/release/update-formula.sh \
#     ../homebrew/Formula/nika.rb 0.90.0 /tmp/rel
#
# The url lines carry `#{version}` (brew interpolates them) so only the
# `version` line and the four `sha256` lines change — each sha matched to the
# `url` line above it by platform.
set -euo pipefail

formula="${1:?usage: update-formula.sh <formula.rb> <version> <artifacts-dir>}"
version="${2:?missing version}"
artifacts="${3:?missing artifacts dir}"

sha_for() {
  local platform="$1"
  local f="${artifacts}/nika-${platform}-${version}.tar.gz.sha256"
  [ -f "$f" ] || {
    echo "update-formula: missing checksum $f" >&2
    exit 1
  }
  awk '{ print $1; exit }' "$f"
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
