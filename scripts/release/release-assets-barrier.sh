#!/usr/bin/env bash
# Converge exactly eight release assets without replacing an occupied identity.
set -euo pipefail

if [ "$#" -lt 4 ]; then
  echo "usage: $0 <stage|verify> <tag> <owner/repo> <asset>..." >&2
  exit 64
fi

mode="$1"
tag="$2"
repo="$3"
shift 3
case "$mode" in stage | verify) ;; *)
  echo "release assets: invalid mode: $mode" >&2
  exit 64
  ;;
esac
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bash "$here/check-release-tag.sh" "$tag"
version="${tag#v}"

expected_names=(
  "nika-macos-arm64-${version}.tar.gz"
  "nika-macos-x64-${version}.tar.gz"
  "nika-linux-arm64-${version}.tar.gz"
  "nika-linux-x64-${version}.tar.gz"
  SHA256SUMS
  multiple.intoto.jsonl
  "supernovae-st-nika-check-wasm-${version}.tgz"
  "supernovae-st-nika-check-wasm-${version}.tgz.sha256"
)

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
local_manifest="$scratch/local"
remote_manifest="$scratch/remote"
: >"$local_manifest"

for asset in "$@"; do
  [ -f "$asset" ] || {
    echo "release assets: missing local asset: $asset" >&2
    exit 66
  }
  basename "$asset" >>"$local_manifest"
done
LC_ALL=C sort -o "$local_manifest" "$local_manifest"
if [ "$(wc -l <"$local_manifest" | tr -d ' ')" -ne 8 ] \
  || [ "$(uniq -d "$local_manifest" | wc -l | tr -d ' ')" -ne 0 ]; then
  echo "release assets: REFUSED missing, extra, or duplicate local asset" >&2
  exit 73
fi
printf '%s\n' "${expected_names[@]}" | LC_ALL=C sort >"$scratch/expected"
cmp -s "$scratch/expected" "$local_manifest" || {
  echo "release assets: REFUSED local allowlist differs from the exact eight names" >&2
  diff -u "$scratch/expected" "$local_manifest" >&2 || true
  exit 73
}

gh api "repos/${repo}/releases/tags/${tag}" --paginate --jq '.assets[].name' \
  | LC_ALL=C sort >"$remote_manifest"
if [ "$(uniq -d "$remote_manifest" | wc -l | tr -d ' ')" -ne 0 ]; then
  echo "release assets: REFUSED duplicate public asset name" >&2
  exit 73
fi
extras="$(comm -13 "$scratch/expected" "$remote_manifest")"
if [ -n "$extras" ]; then
  echo "release assets: REFUSED extra public assets:" >&2
  printf '%s\n' "$extras" >&2
  exit 73
fi

missing=()
for asset in "$@"; do
  name="$(basename "$asset")"
  if grep -Fqx -- "$name" "$remote_manifest"; then
    gh release download "$tag" --repo "$repo" --pattern "$name" --dir "$scratch"
    cmp -s "$asset" "$scratch/$name" || {
      echo "release assets: REFUSED different bytes for occupied asset ${name}" >&2
      exit 73
    }
  else
    missing+=("$asset")
  fi
done

if [ "$mode" = verify ] && [ "${#missing[@]}" -ne 0 ]; then
  echo "release assets: REFUSED missing public assets" >&2
  printf 'missing: %s\n' "${missing[@]##*/}" >&2
  exit 73
fi

# No write occurs until every occupied identity and the full remote allowlist
# have compared. Concurrent same-tag uploads may race; re-run the full compare
# after any failed upload and accept only an identical committed result.
for asset in "${missing[@]}"; do
  name="$(basename "$asset")"
  if ! gh release upload "$tag" --repo "$repo" "$asset"; then
    rm -f "$scratch/$name"
    gh release download "$tag" --repo "$repo" --pattern "$name" --dir "$scratch" \
      || {
        echo "release assets: upload state unknown for ${name}" >&2
        exit 69
      }
    cmp -s "$asset" "$scratch/$name" || {
      echo "release assets: concurrent upload committed divergent ${name}" >&2
      exit 73
    }
  fi
done

remote_count="$(gh api "repos/${repo}/releases/tags/${tag}" --jq '.assets | length')"
[ "$remote_count" = 8 ] || {
  echo "release assets: final asset count is ${remote_count}, expected 8" >&2
  exit 73
}
