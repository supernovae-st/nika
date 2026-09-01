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
[ -n "${NODE_AUTH_TOKEN:-}" ] || {
  echo "npm barrier: NPM_TOKEN is required to publish an absent version" >&2
  exit 77
}

if npm publish "$tgz" --provenance --access public; then
  publish_failed=false
else
  publish_failed=true
fi
for attempt in 1 2 3 4 5 6; do
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
  [ "$attempt" -eq 6 ] || sleep 10
done
if [ "$publish_failed" = true ]; then
  echo "npm barrier: publish failed and the version remains absent" >&2
else
  echo "npm barrier: publish returned success but the version never became visible" >&2
fi
exit 69
