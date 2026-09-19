#!/usr/bin/env bash
# Converge one immutable GHCR version tag from a content-addressed manifest.
# The create path converges the v-prefixed alias (nika#1634) BEFORE the
# version tag, which stays the commit marker: a failed alias leaves the
# version absent so a re-run re-enters the create path, and the occupied
# early exits never touch the alias, so replaying a published train cannot
# retag.
set -euo pipefail

if [ "$#" -ne 6 ]; then
  echo "usage: $0 <discover|inspect|publish|verify> <image> <version> <candidate-digest|-> <sha> <source-url>" >&2
  exit 64
fi
mode="$1"
image="$2"
version="$3"
candidate="$4"
sha="$5"
source_url="$6"
case "$mode" in discover | inspect | publish | verify) ;; *)
  echo "oci barrier: invalid mode: $mode" >&2
  exit 64
  ;;
esac
# The v-prefixed alias is derived from the bare version; a v input would
# double the prefix.
[ "${version#v}" = "$version" ] || {
  echo "oci barrier: version must be bare X.Y.Z, not: $version" >&2
  exit 64
}

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

inspect_raw() {
  docker buildx imagetools inspect "$1" --raw
}

digest_of() {
  docker buildx imagetools inspect "$1" --format '{{json .Manifest.Digest}}' | tr -d '"\r\n'
}

is_explicit_absence() {
  local ref="$1"
  local error_file="$2"
  local statuses
  statuses="$(grep -Eo 'HTTP[/ ][^ ]*[[:space:]]+[0-9]{3}|HTTP [0-9]{3}|[0-9]{3} (Not Found|Unauthorized|Forbidden|Internal Server Error)' \
    "$error_file" || true)"
  if printf '%s\n' "$statuses" | grep -Ev '(^|[[:space:]])404([[:space:]]|$)' | grep -q . \
    || grep -Eqi 'unauthori[sz]ed|forbidden|authentication required|access denied' \
      "$error_file"; then
    return 1
  fi
  grep -Eqi \
    'manifest unknown|MANIFEST_UNKNOWN|NAME_UNKNOWN|unexpected status from HEAD request.*404 Not Found' "$error_file" \
    || grep -Fqx "ERROR: ${ref}: not found" "$error_file" \
    || grep -Fqx "ERROR: no such manifest: ${ref}" "$error_file"
}

verify_identity() {
  local ref="$1"
  local raw="$2"
  inspect_raw "$ref" >"$raw"
  jq -s -e -f "$script_dir/verify-oci-index.jq" "$raw" >/dev/null || {
    echo "oci barrier: expected two Linux platforms and their two bound BuildKit attestations" >&2
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
if [ "$mode" = inspect ]; then
  [[ "$candidate" =~ ^sha256:[0-9a-f]{64}$ ]] || {
    echo "oci barrier: invalid candidate digest" >&2
    exit 64
  }
  verify_identity "${image}@${candidate}" "$scratch/candidate.json"
  printf '%s\n' "$candidate"
  exit 0
fi
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
if ! is_explicit_absence "$version_ref" "$lookup_error"; then
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
is_explicit_absence "$version_ref" "$lookup_error" || {
  echo "oci barrier: version recheck failed without explicit absence" >&2
  cat "$lookup_error" >&2
  exit 69
}

# nika#1634: the release page prints vX.Y.Z, so the manifest this run
# commits also carries the v-prefixed alias. The alias converges BEFORE the
# version tag, which stays the commit marker: a failed alias leaves the
# version absent, so a re-run re-enters this create path (an equal alias is
# then a no-op), and a divergent alias refuses here with zero writes on
# every replay. The occupied early exits above never touch the alias.
alias_ref="${image}:v${version}"
: >"$lookup_error"
state=0
alias_occupied="$(digest_of "$alias_ref" 2>"$lookup_error")" || state=$?
if [ "$state" -eq 0 ]; then
  [ -n "$alias_occupied" ] || {
    echo "oci barrier: empty successful v-alias digest lookup" >&2
    exit 69
  }
  [ "$alias_occupied" = "$candidate" ] || {
    echo "oci barrier: REFUSED divergent occupied v-alias digest" >&2
    exit 73
  }
else
  is_explicit_absence "$alias_ref" "$lookup_error" || {
    echo "oci barrier: v-alias lookup failed without explicit absence" >&2
    cat "$lookup_error" >&2
    exit 69
  }
  docker buildx imagetools create --tag "$alias_ref" "${image}@${candidate}" || {
    echo "oci barrier: v-alias write failed and the version tag remains absent" >&2
    exit 69
  }
  alias_occupied="$(digest_of "$alias_ref")"
  [ "$alias_occupied" = "$candidate" ] || {
    echo "oci barrier: committed v-alias digest differs" >&2
    exit 73
  }
fi

docker buildx imagetools create --tag "$version_ref" "${image}@${candidate}"
occupied="$(digest_of "$version_ref")"
[ "$occupied" = "$candidate" ] || {
  echo "oci barrier: committed version digest differs" >&2
  exit 73
}
verify_identity "$version_ref" "$scratch/version.json"
printf '%s\n' "$occupied"
