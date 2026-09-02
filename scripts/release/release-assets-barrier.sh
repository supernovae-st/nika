#!/usr/bin/env bash
# Converge exactly eight assets on one immutable GitHub release identity.
set -euo pipefail

if [ "$#" -lt 6 ]; then
  echo "usage: $0 <stage|verify> <owner/repo> <release-id> <tag> <sha> <asset>..." >&2
  exit 64
fi

mode="$1"
repo="$2"
release_id="$3"
tag="$4"
sha="$5"
shift 5
case "$mode" in stage | verify) ;; *)
  echo "release assets: invalid mode: $mode" >&2
  exit 64
  ;;
esac
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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
inventory="$scratch/inventory"
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

assert_identity() {
  bash "$here/read-release-state.sh" "$repo" "$release_id" "$tag" "$sha" \
    >/dev/null
}

load_inventory() {
  assert_identity
  if ! gh api "repos/${repo}/releases/${release_id}/assets" --paginate \
    --jq '.[] | [.id, .name] | @tsv' >"$inventory"; then
    echo "release assets: release-ID asset census failed" >&2
    return 69
  fi
  if [ -s "$inventory" ] \
    && ! awk -F '\t' 'NF != 2 || $1 !~ /^[1-9][0-9]*$/ || $2 == "" { exit 1 }' \
      "$inventory"; then
    echo "release assets: REFUSED malformed release-ID asset census" >&2
    return 73
  fi
  cut -f2- "$inventory" | LC_ALL=C sort >"$remote_manifest"
}

validate_inventory() {
  if [ "$(uniq -d "$remote_manifest" | wc -l | tr -d ' ')" -ne 0 ] \
    || [ "$(cut -f1 "$inventory" | LC_ALL=C sort | uniq -d | wc -l | tr -d ' ')" -ne 0 ]; then
    echo "release assets: REFUSED duplicate public asset identity" >&2
    return 73
  fi
  local extras
  extras="$(LC_ALL=C comm -13 "$scratch/expected" "$remote_manifest")"
  if [ -n "$extras" ]; then
    echo "release assets: REFUSED extra public assets:" >&2
    printf '%s\n' "$extras" >&2
    return 73
  fi
}

asset_id_for_name() {
  local name="$1"
  awk -F '\t' -v target="$name" '$2 == target { print $1 }' "$inventory"
}

compare_asset() {
  local asset="$1"
  local asset_id="$2"
  local name
  name="$(basename "$asset")"
  rm -f "$scratch/download"
  if ! gh api "repos/${repo}/releases/assets/${asset_id}" \
    -H 'Accept: application/octet-stream' >"$scratch/download"; then
    echo "release assets: release-ID download failed for ${name}" >&2
    return 69
  fi
  cmp -s "$asset" "$scratch/download" || {
    echo "release assets: REFUSED different bytes for occupied asset ${name}" >&2
    return 73
  }
}

load_inventory
validate_inventory
missing=()
for asset in "$@"; do
  name="$(basename "$asset")"
  asset_id="$(asset_id_for_name "$name")"
  if [ -n "$asset_id" ]; then
    compare_asset "$asset" "$asset_id"
  else
    missing+=("$asset")
  fi
done

if [ "$mode" = verify ] && [ "${#missing[@]}" -ne 0 ]; then
  echo "release assets: REFUSED missing public assets" >&2
  printf 'missing: %s\n' "${missing[@]##*/}" >&2
  exit 73
fi

upload_url=""
if [ "${#missing[@]}" -ne 0 ]; then
  [ -n "${GH_TOKEN:-}" ] || {
    echo "release assets: GH_TOKEN is mandatory for asset upload" >&2
    exit 77
  }
  upload_template="$(gh api "repos/${repo}/releases/${release_id}" \
    --jq '.upload_url // ""')"
  upload_url="${upload_template%%\{*}"
  expected_upload_url="https://uploads.github.com/repos/${repo}/releases/${release_id}/assets"
  [ "$upload_url" = "$expected_upload_url" ] || {
    echo "release assets: REFUSED unexpected release-ID upload URL" >&2
    exit 73
  }
  umask 077
  printf 'Authorization: Bearer %s\n' "$GH_TOKEN" >"$scratch/auth-header"
fi

# Every upload targets the immutable release-ID URL. Re-census the same ID and
# revalidate its tag/SHA immediately before each write. GitHub has no
# conditional upload primitive, so an administrator may still mutate release
# metadata between these API calls; the post-write and final reads refuse any
# drift they can observe, while the upload itself can never resolve another tag.
for asset in "${missing[@]}"; do
  name="$(basename "$asset")"
  load_inventory
  validate_inventory
  asset_id="$(asset_id_for_name "$name")"
  if [ -n "$asset_id" ]; then
    compare_asset "$asset" "$asset_id"
    continue
  fi
  assert_identity
  upload_failed=false
  if ! curl --fail --location --silent --show-error \
    --request POST \
    --header "@$scratch/auth-header" \
    --header 'Accept: application/vnd.github+json' \
    --header 'Content-Type: application/octet-stream' \
    --header 'X-GitHub-Api-Version: 2022-11-28' \
    --data-binary "@${asset}" \
    --output "$scratch/upload-response" \
    "${upload_url}?name=${name}"; then
    upload_failed=true
  fi
  assert_identity
  if [ "$upload_failed" = true ]; then
    load_inventory
    validate_inventory
    asset_id="$(asset_id_for_name "$name")"
    [ -n "$asset_id" ] || {
      echo "release assets: upload state unknown for ${name}" >&2
      exit 69
    }
    compare_asset "$asset" "$asset_id"
  fi
done

# Accept convergence only after the immutable ID still owns the expected tag
# and every one of the exact eight asset IDs downloads to the expected bytes.
assert_identity
load_inventory
validate_inventory
[ "$(wc -l <"$remote_manifest" | tr -d ' ')" = 8 ] || {
  echo "release assets: final asset count is not 8" >&2
  exit 73
}
for asset in "$@"; do
  name="$(basename "$asset")"
  asset_id="$(asset_id_for_name "$name")"
  [ -n "$asset_id" ] || {
    echo "release assets: final asset is missing: ${name}" >&2
    exit 73
  }
  compare_asset "$asset" "$asset_id"
done
assert_identity
