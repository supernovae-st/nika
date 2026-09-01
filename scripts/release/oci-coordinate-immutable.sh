#!/usr/bin/env bash
# Converge one immutable GHCR version tag from a content-addressed manifest.
set -euo pipefail

if [ "$#" -ne 6 ]; then
  echo "usage: $0 <discover|publish|verify> <image> <version> <candidate-digest|-> <sha> <source-url>" >&2
  exit 64
fi
mode="$1"
image="$2"
version="$3"
candidate="$4"
sha="$5"
source_url="$6"
case "$mode" in discover | publish | verify) ;; *)
  echo "oci barrier: invalid mode: $mode" >&2
  exit 64
  ;;
esac

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT

inspect_raw() {
  docker buildx imagetools inspect "$1" --raw
}

digest_of() {
  docker buildx imagetools inspect "$1" --format '{{json .Manifest.Digest}}' | tr -d '"\r\n'
}

is_explicit_absence() {
  grep -Eqi \
    'manifest unknown|MANIFEST_UNKNOWN|NAME_UNKNOWN|unexpected status from HEAD request.*404 Not Found' "$1" \
    || grep -Fqx "ERROR: ${version_ref}: not found" "$1" \
    || grep -Fqx "ERROR: no such manifest: ${version_ref}" "$1"
}

verify_identity() {
  local ref="$1"
  local raw="$2"
  inspect_raw "$ref" >"$raw"
  platforms="$(jq -c '[.manifests[].platform | "\(.os)/\(.architecture)"] | sort' "$raw")"
  [ "$platforms" = '["linux/amd64","linux/arm64"]' ] || {
    echo "oci barrier: expected exactly one linux/amd64 and one linux/arm64" >&2
    return 73
  }
  for platform in linux/amd64 linux/arm64; do
    config="$(docker buildx imagetools inspect "$ref" --format "{{json (index .Image \"${platform}\").Config.Labels}}")"
    jq -e \
      --arg revision "$sha" --arg version "$version" --arg source "$source_url" \
      '."org.opencontainers.image.revision" == $revision and
       ."org.opencontainers.image.version" == $version and
       ."org.opencontainers.image.source" == $source and
       ."org.opencontainers.image.licenses" == "AGPL-3.0-or-later"' \
      <<<"$config" >/dev/null || {
      echo "oci barrier: label drift on ${platform}" >&2
      return 73
    }
  done
}

version_ref="${image}:${version}"
lookup_error="$scratch/lookup-error"
if occupied="$(digest_of "$version_ref" 2>"$lookup_error")"; then
  [ -n "$occupied" ] || {
    echo "oci barrier: empty successful digest lookup" >&2
    exit 69
  }
  if [ "$candidate" != - ] && [ "$occupied" != "$candidate" ]; then
    echo "oci barrier: REFUSED divergent occupied version digest" >&2
    exit 73
  fi
  verify_identity "$version_ref" "$scratch/version.json"
  verify_identity "${image}@${occupied}" "$scratch/digest.json"
  printf '%s\n' "$occupied"
  exit 0
fi
if ! is_explicit_absence "$lookup_error"; then
  echo "oci barrier: version lookup failed without explicit absence" >&2
  cat "$lookup_error" >&2
  exit 69
fi
[ "$mode" != discover ] || exit 44
[ "$mode" = publish ] || {
  echo "oci barrier: version is absent" >&2
  exit 73
}
[[ "$candidate" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "oci barrier: invalid candidate digest" >&2
  exit 64
}

verify_identity "${image}@${candidate}" "$scratch/candidate.json"
# Close the lookup/write race. If another train committed the coordinate,
# accept only the same digest with the same source identity.
: >"$lookup_error"
state=0
occupied="$(digest_of "$version_ref" 2>"$lookup_error")" || state=$?
if [ "$state" -eq 0 ]; then
  [ "$occupied" = "$candidate" ] || {
    echo "oci barrier: REFUSED concurrently occupied version digest" >&2
    exit 73
  }
  verify_identity "$version_ref" "$scratch/concurrent-version.json"
  verify_identity "${image}@${occupied}" "$scratch/concurrent-digest.json"
  printf '%s\n' "$occupied"
  exit 0
fi
is_explicit_absence "$lookup_error" || {
  echo "oci barrier: version recheck failed without explicit absence" >&2
  cat "$lookup_error" >&2
  exit 69
}
docker buildx imagetools create --tag "$version_ref" "${image}@${candidate}"
occupied="$(digest_of "$version_ref")"
[ "$occupied" = "$candidate" ] || {
  echo "oci barrier: committed version digest differs" >&2
  exit 73
}
verify_identity "$version_ref" "$scratch/version.json"
printf '%s\n' "$occupied"
