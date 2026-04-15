#!/usr/bin/env bash
# Vector 19: Warn on ADRs stuck in proposed/draft status >30 days.
#
# Exit codes:
#   0 -- GREEN (no stale ADRs)
#   1 -- YELLOW (stale proposed/draft found)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

ADR_DIR="docs/adr"
TODAY_EPOCH=$(date +%s)
THRESHOLD_DAYS=30
stale=0

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  fm="$(awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$f")"
  [ -z "$fm" ] && continue

  status="$(printf '%s\n' "$fm" | grep -E '^status:' | head -1 \
    | sed -E 's/^status:[[:space:]]*//' | tr -d '"'"'")"

  case "$status" in
    proposed | draft) ;;
    *) continue ;;
  esac

  date_str="$(printf '%s\n' "$fm" | grep -E '^date:' | head -1 \
    | sed -E 's/^date:[[:space:]]*//' | tr -d '"'"'")"
  [ -z "$date_str" ] && continue

  # Parse date (portable: works on macOS + Linux)
  if date -j -f "%Y-%m-%d" "$date_str" "+%s" >/dev/null 2>&1; then
    adr_epoch=$(date -j -f "%Y-%m-%d" "$date_str" "+%s")
  elif date -d "$date_str" "+%s" >/dev/null 2>&1; then
    adr_epoch=$(date -d "$date_str" "+%s")
  else
    continue
  fi

  age_days=$(((TODAY_EPOCH - adr_epoch) / 86400))
  if [ "$age_days" -gt "$THRESHOLD_DAYS" ]; then
    adr_id="$(printf '%s\n' "$fm" | grep -E '^id:' | head -1 \
      | sed -E 's/^id:[[:space:]]*//' | tr -d '"'"'")"
    echo "STALE: ${adr_id} ${status} for ${age_days} days" >&2
    stale=$((stale + 1))
  fi
done

if [ "$stale" -gt 0 ]; then
  echo "$stale ADR(s) in proposed/draft >30 days"
  exit 1
fi

echo "OK (no stale proposed/draft ADRs)"
exit 0
