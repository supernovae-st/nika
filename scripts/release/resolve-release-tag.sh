#!/usr/bin/env bash
# Resolve a canonical release tag to one peeled commit and optionally prove it
# has not moved since the train began.
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <tag> <owner/repo> [expected-sha]" >&2
  exit 64
fi

tag="$1"
repo="$2"
expected="${3:-}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

bash "$here/check-release-tag.sh" "$tag"
if [[ ! "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "release coordinate: invalid repository: $repo" >&2
  exit 64
fi
if [ -n "$expected" ] && [[ ! "$expected" =~ ^[0-9a-f]{40}$ ]]; then
  echo "release coordinate: invalid expected commit: $expected" >&2
  exit 64
fi

refs="$(git ls-remote --exit-code "https://github.com/${repo}.git" \
  "refs/tags/${tag}" "refs/tags/${tag}^{}")" || {
  echo "release coordinate: tag not found: $tag" >&2
  exit 66
}
direct="$(printf '%s\n' "$refs" | awk -v ref="refs/tags/${tag}" '$2 == ref { print $1 }')"
peeled="$(printf '%s\n' "$refs" | awk -v ref="refs/tags/${tag}^{}" '$2 == ref { print $1 }')"
sha="${peeled:-$direct}"

if [[ ! "$sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "release coordinate: tag did not resolve to exactly one commit: $tag" >&2
  exit 73
fi
if [ -n "$expected" ] && [ "$sha" != "$expected" ]; then
  echo "release coordinate: REFUSED moved tag ${tag}: began at ${expected}, now ${sha}" >&2
  exit 73
fi
printf '%s\n' "$sha"
