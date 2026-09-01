#!/usr/bin/env bash
# Validate the one publication coordinate shared by every release surface.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <vMAJOR.MINOR.PATCH[-PRERELEASE]>" >&2
  exit 64
fi

tag="$1"

# SemVer 2.0 core + prerelease. Build metadata is deliberately excluded: npm,
# Homebrew, OCI, and GitHub must share one unambiguous publication coordinate,
# while SemVer precedence intentionally ignores `+build` identifiers.
core='(0|[1-9][0-9]*)'
prerelease_id='(0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)'
semver_tag="^v${core}\.${core}\.${core}(-${prerelease_id}(\.${prerelease_id})*)?$"
if [[ ! "$tag" =~ $semver_tag ]]; then
  echo "release coordinate: invalid canonical semver tag: $tag" >&2
  exit 64
fi
