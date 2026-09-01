#!/usr/bin/env bash
# Idempotently move a mutable OCI pointer after its newest-release guard passes.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <image> <pointer> <target-digest>" >&2
  exit 64
fi

image="$1"
pointer="$2"
target="$3"
[[ "$image" =~ ^[A-Za-z0-9._/-]+$ ]] || {
  echo "oci pointer: invalid image: $image" >&2
  exit 64
}
[[ "$pointer" =~ ^[A-Za-z0-9._-]+$ ]] || {
  echo "oci pointer: invalid pointer: $pointer" >&2
  exit 64
}
[[ "$target" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "oci pointer: invalid target digest" >&2
  exit 64
}

ref="${image}:${pointer}"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT

digest_of() {
  docker buildx imagetools inspect "$ref" \
    --format '{{json .Manifest.Digest}}' | tr -d '"\r\n'
}

is_explicit_absence() {
  local error_file="$1"
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

lookup() {
  local error_file="$1"
  local state=0
  local occupied
  occupied="$(digest_of 2>"$error_file")" || state=$?
  if [ "$state" -eq 0 ]; then
    [ -n "$occupied" ] || {
      echo "oci pointer: empty successful digest lookup" >&2
      return 69
    }
    printf '%s\n' "$occupied"
    return 0
  fi
  if is_explicit_absence "$error_file"; then
    return 44
  fi
  echo "oci pointer: lookup failed without explicit absence" >&2
  cat "$error_file" >&2
  return 69
}

state=0
occupied="$(lookup "$scratch/lookup-error")" || state=$?
if [ "$state" -eq 0 ] && [ "$occupied" = "$target" ]; then
  echo "oci pointer: ${pointer} already equals ${target}"
  exit 0
fi
case "$state" in 0 | 44) ;; *) exit "$state" ;; esac

# Recheck immediately before the mutable write. A concurrent convergence to
# the same digest is success; an unknown read never grants write authority.
: >"$scratch/recheck-error"
state=0
occupied="$(lookup "$scratch/recheck-error")" || state=$?
if [ "$state" -eq 0 ] && [ "$occupied" = "$target" ]; then
  echo "oci pointer: ${pointer} converged concurrently"
  exit 0
fi
case "$state" in 0 | 44) ;; *) exit "$state" ;; esac

create_failed=false
if ! docker buildx imagetools create --tag "$ref" "${image}@${target}"; then
  create_failed=true
fi
: >"$scratch/committed-error"
state=0
committed="$(lookup "$scratch/committed-error")" || state=$?
if [ "$state" -eq 0 ] && [ "$committed" = "$target" ]; then
  echo "oci pointer: ${pointer} now equals ${target}"
  exit 0
fi
if [ "$create_failed" = true ]; then
  echo "oci pointer: write failed and target digest did not commit" >&2
else
  echo "oci pointer: write returned success but target digest did not commit" >&2
fi
exit 69
