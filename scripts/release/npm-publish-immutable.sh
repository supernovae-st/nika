#!/usr/bin/env bash
# Validate one npm tarball/sidecar and converge an immutable package version.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <publish|verify> <package@version> <tgz> <sha256-sidecar>" >&2
  exit 64
fi
mode="$1"
coordinate="$2"
tgz="$3"
sidecar="$4"
case "$mode" in publish | verify) ;; *)
  echo "npm barrier: invalid mode: $mode" >&2
  exit 64
  ;;
esac
[ -f "$tgz" ] && [ -f "$sidecar" ] || {
  echo "npm barrier: tarball or sidecar missing" >&2
  exit 66
}
[ "$(basename "$sidecar")" = "$(basename "$tgz").sha256" ] \
  || {
    echo "npm barrier: sidecar name does not match tarball" >&2
    exit 73
  }
(cd "$(dirname "$tgz")" && sha256sum -c "$(basename "$sidecar")") >/dev/null
have="sha512-$(openssl dgst -sha512 -binary "$tgz" | base64 | tr -d '\n')"

lookup() {
  local out="$1"
  local err="$2"
  if npm view "$coordinate" dist.integrity >"$out" 2>"$err"; then
    test -s "$out" || {
      echo "npm barrier: empty successful integrity lookup" >&2
      return 69
    }
    return 0
  fi
  local npm_codes http_codes
  npm_codes="$(grep -Eo 'E[0-9]{3}' "$err" || true)"
  http_codes="$(grep -Eo 'HTTP([ /][^ ]+)?[[:space:]]+[0-9]{3}|\(HTTP [0-9]{3}\)' \
    "$err" || true)"
  if printf '%s\n' "$npm_codes" | grep -Fqx E404 \
    && ! printf '%s\n' "$npm_codes" | grep -Fvx E404 | grep -q . \
    && ! printf '%s\n' "$http_codes" | grep -Ev '(^|[[:space:]])404\)?$' | grep -q . \
    && ! grep -Eqi 'unauthori[sz]ed|forbidden|authentication required|access denied' \
      "$err"; then
    return 44
  fi
  echo "npm barrier: lookup failed without explicit E404" >&2
  cat "$err" >&2
  return 69
}

scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
state=0
lookup "$scratch/integrity" "$scratch/error" || state=$?
case "$state" in
  0)
    want="$(tr -d '\r\n' <"$scratch/integrity")"
    [ "$want" = "$have" ] || {
      echo "npm barrier: REFUSED divergent occupied version" >&2
      exit 73
    }
    echo "npm barrier: occupied version is byte-identical"
    exit 0
    ;;
  44) ;;
  *) exit "$state" ;;
esac

[ "$mode" = publish ] || {
  echo "npm barrier: package version is absent" >&2
  exit 73
}
[ -n "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" ] && [ -n "${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-}" ] || {
  echo "npm barrier: GitHub OIDC is required to publish an absent version · id-token: write on the job, and the package's trusted publisher names this repository and workflow" >&2
  exit 77
}

# npm itself warns a processed publish "may take a few minutes" to become
# visible: v0.120.0 and v0.120.1 both committed, yet answered E404 for over
# two minutes, and the old ~52s window declared a successful publish lost,
# costing a full train replay. Readiness is a BOUNDED budget, not a fixed
# retry count: ten-second cadence under a five-minute default, overridable
# for a slower registry. An occupied divergent identity still refuses on
# first sight, and nothing here ever publishes twice. The budget validates
# before any registry write.
readiness_budget="${NIKA_NPM_READINESS_SECONDS:-300}"
# `[ -ge ]` reads the budget as decimal while `$(( ))` reads it as octal, so
# a leading zero would make the two disagree (010 floors to zero lookups, 090
# dies mid-publish); reject it, zero, and six digits or more up front.
case "$readiness_budget" in
  '' | *[!0-9]* | 0* | ??????*)
    echo "npm barrier: NIKA_NPM_READINESS_SECONDS must be a positive integer of seconds (1-99999, no leading zeros)" >&2
    exit 64
    ;;
esac
readiness_cadence=10
[ "$readiness_budget" -ge "$readiness_cadence" ] || readiness_budget="$readiness_cadence"
readiness_attempts=$((readiness_budget / readiness_cadence))

# npm records its OIDC exchange in the private debug log even at the
# default console level. Keep that log ephemeral; emit only fixed diagnoses,
# never its token-bearing contents or a registry-supplied error message.
if npm publish "$tgz" --provenance --access public --logs-dir "$scratch/npm-logs"; then
  publish_failed=false
else
  publish_failed=true
fi
attempt=1
while [ "$attempt" -le "$readiness_attempts" ]; do
  : >"$scratch/integrity"
  : >"$scratch/error"
  state=0
  lookup "$scratch/integrity" "$scratch/error" || state=$?
  if [ "$state" -eq 0 ]; then
    want="$(tr -d '\r\n' <"$scratch/integrity")"
    [ "$want" = "$have" ] || {
      echo "npm barrier: REFUSED divergent committed publish" >&2
      exit 73
    }
    echo "npm barrier: publish committed with exact SRI"
    exit 0
  fi
  [ "$state" -eq 44 ] || exit "$state"
  [ "$attempt" -eq "$readiness_attempts" ] || sleep "$readiness_cadence"
  attempt=$((attempt + 1))
done
if [ "$publish_failed" = true ]; then
  echo "npm barrier: publish failed and the version remains absent" >&2
  if grep -Fq 'verbose oidc Successfully retrieved and set token' "$scratch"/npm-logs/*-debug-0.log 2>/dev/null; then
    echo "npm barrier: OIDC exchange succeeded; check that the trusted publisher allows direct npm publish, not only npm stage publish, and has publishing access to this package" >&2
  elif grep -Fq 'verbose oidc Failed token exchange request' "$scratch"/npm-logs/*-debug-0.log 2>/dev/null; then
    echo "npm barrier: OIDC exchange was rejected; verify the trusted publisher organization, repository, workflow filename and optional environment exactly match this job" >&2
  elif grep -Fq 'verbose oidc Failed to fetch id_token from GitHub' "$scratch"/npm-logs/*-debug-0.log 2>/dev/null; then
    echo "npm barrier: GitHub did not return an OIDC identity; inspect the job id-token permission and runner response" >&2
  else
    echo "npm barrier: OIDC exchange outcome unavailable; no authentication conclusion can be drawn from the publish error alone" >&2
  fi
else
  echo "npm barrier: publish returned success but the version never became visible within ${readiness_budget}s" >&2
fi
exit 69
