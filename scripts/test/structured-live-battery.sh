#!/usr/bin/env bash
# structured-live-battery.sh — Rail C · the LIVE provider rail for
# structured output (master plan 2026-07-08 · P0 slice).
#
# The golden rail (`nika test`) proves the engine under mock; the SeamHttp
# pins prove failure classes; THIS proves the WIRE truth per provider —
# key-gated, skip-honest, structural asserts only (never content equality).
#
#   S1 nested        enum + required + closed objects   → exit 0 · usage>0
#   S2 grammar-blind not + uniqueItems (local floor)    → exit 0 · usage>0
#   S3 underspec.    {type: object} (ADR-098 fallback)  → exit 0 · usage>0
#   S5 truncation    max_tokens:16 + demanding schema   → exit≠0 · names the
#                                                          token limit · attempts=1
#
# Usage:
#   scripts/test/structured-live-battery.sh [--quick] [--nika <bin>] [ids…]
#     --quick     S1 only
#     --nika BIN  binary under test (default: target/release/nika, then PATH)
#     ids…        restrict to manifest ids (e.g. `anthropic ollama`)
#
# Contract: a missing key / unreachable local server prints a loud SKIP —
# NEVER a false green. Exit ≠0 iff any ✖. One retry on transport-class
# failures only. Every scenario caps max_tokens ≤300 (a full default run
# costs cents).
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BATTERY="$HERE/battery"
MANIFEST="$BATTERY/manifest.tsv"
QUICK=0
NIKA=""
IDS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --quick) QUICK=1 ;;
    --nika)
      shift
      NIKA="$1"
      ;;
    *) IDS+=("$1") ;;
  esac
  shift
done
if [ -z "$NIKA" ]; then
  # the bin target IS the public name `nika` (ADR-135); brew installs the same.
  if [ -x "$HERE/../../target/release/nika" ]; then
    NIKA="$HERE/../../target/release/nika"
  else NIKA="$(command -v nika || true)"; fi
fi
[ -x "${NIKA:-/nonexistent}" ] || {
  echo "no nika binary (build release or --nika)"
  exit 2
}
echo "binary under test: $NIKA ($("$NIKA" --version 2>/dev/null))"

SCENARIOS=(s1-nested s2-grammar-blind s3-underspecified s5-truncation)
[ "$QUICK" = 1 ] && SCENARIOS=(s1-nested)

probe_local() { # id → 0 if the local server answers
  case "$1" in
    ollama) curl -s -m 3 http://localhost:11434/v1/models >/dev/null 2>&1 ;;
    llamacpp) curl -s -m 3 http://localhost:8080/v1/models >/dev/null 2>&1 ;;
    localai) curl -s -m 3 http://localhost:8081/v1/models >/dev/null 2>&1 ;;
    *) return 1 ;;
  esac
}

fails=0
printf '%-11s' "provider"
for s in "${SCENARIOS[@]}"; do printf '%-20s' "$s"; done
echo

while IFS=$'\t' read -r id model key_env base_url_env _notes; do
  case "$id" in \#* | "") continue ;; esac
  if [ "${#IDS[@]}" -gt 0 ]; then
    printf '%s\n' "${IDS[@]}" | grep -qx "$id" || continue
  fi

  # ── gate: key (the REAL ladder: NIKA_<ID>_API_KEY wins · #286) or local probe
  skip=""
  upper="$(echo "$id" | tr '[:lower:]' '[:upper:]')"
  if [ "$key_env" = "-" ]; then
    probe_local "$id" || skip="server down"
  else
    ladder="NIKA_${upper}_API_KEY"
    if [ -z "${!ladder:-}" ] && [ -z "${!key_env:-}" ]; then skip="no key"; fi
    if [ "$base_url_env" != "-" ] && [ -z "${!base_url_env:-}" ]; then skip="no base url"; fi
  fi

  printf '%-11s' "$id"
  if [ -n "$skip" ]; then
    for _s in "${SCENARIOS[@]}"; do printf '%-20s' "SKIP($skip)"; done
    echo
    continue
  fi

  for s in "${SCENARIOS[@]}"; do
    wf="$BATTERY/$s.nika.yaml"
    t0=$(date +%s)
    out="$({ timeout 240 "$NIKA" run "$wf" --model "$model" --color never --json; } 2>&1)"
    rc=$?
    # one retry ONLY on transport-class failures (5xx/timeout/connect) — never on assertion failures
    if [ $rc -ne 0 ] && echo "$out" | grep -qiE 'HTTP 5[0-9][0-9]|timed? ?out|connection|temporary|temporarily'; then
      sleep 3
      out="$({ timeout 240 "$NIKA" run "$wf" --model "$model" --color never --json; } 2>&1)"
      rc=$?
    fi
    ms=$(($(date +%s) - t0))s
    if [ $rc -ne 0 ] && echo "$out" | grep -qiE 'credit balance is too low|insufficient credit'; then
      # key present but unfunded — an honest SKIP, never a ✖ (the #264 class)
      v="SKIP(no credits)"
    elif [ "$s" = s5-truncation ]; then
      # structural: the run FAILS and the error names the token limit (fast-fail #268)
      if [ $rc -ne 0 ] && echo "$out" | grep -qi "token limit"; then
        v="✔ fast-fail"
        echo "$out" | grep -qE 'after 1 attempt|attempts[": ]+1' || v="✔ (attempts?)"
      else
        v="✖(rc=$rc)"
        fails=$((fails + 1))
      fi
    else
      # structural: exit 0 + a nonzero token count in the trace — the NDJSON
      # carries usage as an event attribute pair `{"key":"tokens","value":N}`
      if [ $rc -eq 0 ] && echo "$out" | grep -qE '"key":"tokens","value":[1-9]'; then
        v="✔"
      elif [ $rc -eq 0 ]; then
        v="✖(no-usage)"
        fails=$((fails + 1))
      else
        v="✖(rc=$rc)"
        fails=$((fails + 1))
      fi
    fi
    printf '%-20s' "$v $ms"
  done
  echo
done <"$MANIFEST"

echo "────────────────────────────────────────────────"
if [ "$fails" -gt 0 ]; then
  echo "battery: $fails ✖ — every ✖ graduates: SeamHttp pin + audit row (+ Profile fix if a capability fact)"
  exit 1
fi
echo "battery: ALL GREEN on every non-skipped cell"
