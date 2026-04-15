#!/usr/bin/env bash
# Vector 18: Detect references to non-existent ADR IDs in frontmatter.
#
# Exit codes:
#   0 -- GREEN (no dangling refs)
#   2 -- RED (dangling refs found)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

ADR_DIR="docs/adr"
REF_FIELDS="supersedes superseded_by related requires enables amends"

# Collect all known IDs
declare -A KNOWN_IDS

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  adr_id="$(awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$f" \
    | grep -E '^id:' | head -1 | sed -E 's/^id:[[:space:]]*//' | tr -d '"'"'")"
  [ -n "$adr_id" ] && KNOWN_IDS["$adr_id"]=1
done

danglers=0

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  fm="$(awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$f")"
  [ -z "$fm" ] && continue
  adr_id="$(printf '%s\n' "$fm" | grep -E '^id:' | head -1 \
    | sed -E 's/^id:[[:space:]]*//' | tr -d '"'"'")"

  for field in $REF_FIELDS; do
    refs="$(printf '%s\n' "$fm" | grep -E "^${field}:" | head -1 \
      | sed -E "s/^${field}:[[:space:]]*//" | tr -d '[]"'"'" | tr ',' ' ')"
    for ref in $refs; do
      [ -z "$ref" ] && continue
      if [ "${KNOWN_IDS[$ref]:-0}" != "1" ]; then
        echo "DANGLING: ${adr_id}.${field} -> ${ref}" >&2
        danglers=$((danglers + 1))
      fi
    done
  done
done

if [ "$danglers" -gt 0 ]; then
  echo "$danglers dangling ADR reference(s)"
  exit 2
fi

echo "OK (0 dangling refs across ${#KNOWN_IDS[@]} ADRs)"
exit 0
