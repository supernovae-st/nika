#!/usr/bin/env bash
# Vector 17: Detect supersession cycles in ADR graph.
#
# Exit codes:
#   0 -- GREEN (no cycles)
#   2 -- RED (cycle detected)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

ADR_DIR="docs/adr"

# Build adjacency list: ADR-NNN -> [supersedes targets]
declare -A GRAPH

for f in "$ADR_DIR"/adr-[0-9][0-9][0-9]-*.md; do
  [ -f "$f" ] || continue
  fm="$(awk '/^---$/{n++; next} n==1{print} n>=2{exit}' "$f")"
  [ -z "$fm" ] && continue
  adr_id="$(printf '%s\n' "$fm" | grep -E '^id:' | head -1 | sed -E 's/^id:[[:space:]]*//' | tr -d '"'"'")"
  targets="$(printf '%s\n' "$fm" | grep -E '^supersedes:' | head -1 \
    | sed -E 's/^supersedes:[[:space:]]*//' | tr -d '[]"'"'" | tr ',' ' ')"
  GRAPH["$adr_id"]="${targets:-}"
done

# DFS cycle detection
declare -A VISITED STACK
cycles=0

dfs() {
  local node="$1"
  VISITED["$node"]=1
  STACK["$node"]=1

  for neighbor in ${GRAPH["$node"]:-}; do
    [ -z "$neighbor" ] && continue
    if [ "${STACK[$neighbor]:-0}" -eq 1 ]; then
      echo "CYCLE: $node -> $neighbor (back edge)" >&2
      cycles=$((cycles + 1))
    elif [ "${VISITED[$neighbor]:-0}" -eq 0 ]; then
      dfs "$neighbor"
    fi
  done

  STACK["$node"]=0
}

for node in "${!GRAPH[@]}"; do
  [ "${VISITED[$node]:-0}" -eq 0 ] && dfs "$node"
done

if [ "$cycles" -gt 0 ]; then
  echo "$cycles supersession cycle(s) detected"
  exit 2
fi

echo "OK (0 cycles in ${#GRAPH[@]} ADRs)"
exit 0
