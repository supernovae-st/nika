#!/usr/bin/env bash
# Vector 43: the ADR machine index agrees with frontmatter — per id.
#
# The index (docs/adr/index.{json,toml}) is a PROJECTION of the ADR
# frontmatter via scripts/adr/generate-index.sh. The schema gate
# (vector 16) validates each artifact ALONE — nothing cross-checked
# projection == source, and 7 statuses sat stale in the index while
# their frontmatter flipped (caught by the 2026-07-13 SOTA audit).
# This gate closes that class: for every ADR file, the id's status in
# index.json must equal the frontmatter status. Fix on breach is
# NEVER a hand edit: bash scripts/adr/generate-index.sh
#
# Exit codes:
#   0 -- GREEN (index status matches frontmatter for every ADR)
#   2 -- RED   (mismatch or missing index entry — regenerate the index)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit 2

ADR_DIR="docs/adr"
INDEX_JSON="$ADR_DIR/index.json"

if [ ! -f "$INDEX_JSON" ]; then
  echo "RED: $INDEX_JSON missing — bash scripts/adr/generate-index.sh"
  exit 2
fi

mismatch=0
checked=0

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  fm="$(awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$f")"
  [ -z "$fm" ] && continue

  id="$(printf '%s\n' "$fm" | grep -E '^id:' | head -1 \
    | sed -E 's/^id:[[:space:]]*//' | tr -d '"'"'")"
  status="$(printf '%s\n' "$fm" | grep -E '^status:' | head -1 \
    | sed -E 's/^status:[[:space:]]*//' | tr -d '"'"'")"
  [ -z "$id" ] && continue

  index_status="$(python3 -c "
import json, sys
entries = json.load(open('$INDEX_JSON'))
if isinstance(entries, dict):
    entries = entries.get('adrs', entries.get('entries', []))
for e in entries:
    if e.get('id') == '$id':
        print(e.get('status', ''))
        break
" 2>/dev/null)"

  checked=$((checked + 1))
  if [ -z "$index_status" ]; then
    echo "MISSING: $id has no index entry"
    mismatch=$((mismatch + 1))
  elif [ "$index_status" != "$status" ]; then
    echo "STALE: $id — frontmatter '$status' · index '$index_status'"
    mismatch=$((mismatch + 1))
  fi
done

if [ "$mismatch" -gt 0 ]; then
  echo "RED: $mismatch index/frontmatter divergence(s) across $checked ADR(s)"
  echo "  fix: bash scripts/adr/generate-index.sh   # regenerate — never hand-edit"
  exit 2
fi

echo "OK: index status matches frontmatter for all $checked ADR(s)"
exit 0
