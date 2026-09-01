#!/usr/bin/env bash
# Upload release assets without ever replacing bytes under an occupied name.
set -euo pipefail

if [ "$#" -lt 3 ]; then
  echo "usage: $0 <tag> <owner/repo> <asset>..." >&2
  exit 64
fi

tag="$1"
repo="$2"
shift 2

# SemVer 2.0 core + prerelease. Build metadata is deliberately excluded: npm,
# Homebrew, OCI, and GitHub must share one unambiguous publication coordinate,
# while SemVer precedence intentionally ignores `+build` identifiers.
core='(0|[1-9][0-9]*)'
prerelease_id='(0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)'
semver_tag="^v${core}\.${core}\.${core}(-${prerelease_id}(\.${prerelease_id})*)?$"
if [[ ! "$tag" =~ $semver_tag ]]; then
  echo "release asset upload: invalid semver tag: $tag" >&2
  exit 64
fi
if [[ ! "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  echo "release asset upload: invalid repository: $repo" >&2
  exit 64
fi

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT

asset_names="$(gh api "repos/${repo}/releases/tags/${tag}" --paginate \
  --jq '.assets[].name')"

missing_assets=()
seen_names=""
for asset in "$@"; do
  [ -f "$asset" ] || {
    echo "release asset upload: missing local asset: $asset" >&2
    exit 66
  }
  name="$(basename "$asset")"
  if printf '%s\n' "$seen_names" | grep -Fqx -- "$name"; then
    echo "release asset upload: REFUSED duplicate local asset name ${name}" >&2
    exit 73
  fi
  seen_names="${seen_names}${seen_names:+$'\n'}${name}"
  matches="$(printf '%s\n' "$asset_names" | grep -Fxc -- "$name" || true)"
  case "$matches" in
    0)
      missing_assets+=("$asset")
      ;;
    1)
      gh release download "$tag" --repo "$repo" --pattern "$name" --dir "$scratch"
      if ! cmp -s "$asset" "$scratch/$name"; then
        echo "release asset upload: REFUSED different bytes for occupied asset ${name}" >&2
        exit 73
      fi
      echo "release asset upload: ${name} already exists with identical bytes"
      ;;
    *)
      echo "release asset upload: REFUSED duplicate public asset name ${name}" >&2
      exit 73
      ;;
  esac
done

# Validate the entire occupied set before mutating the release. If any public
# byte differs, even earlier missing names remain absent.
for asset in "${missing_assets[@]}"; do
  gh release upload "$tag" --repo "$repo" "$asset"
  echo "release asset upload: added $(basename "$asset")"
done
