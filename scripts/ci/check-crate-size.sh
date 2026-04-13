#!/usr/bin/env bash
# Ratchet: every member crate must have <= MAX LOC of tracked .rs source
# (src/ + tests/ + benches/). See CONSTELLATION_PLAN §7 criterion 3.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

MAX=15000
violations=0

while IFS= read -r manifest; do
  [ -z "$manifest" ] && continue
  crate_dir=$(dirname "$manifest")
  total=$(
    git ls-files -- "$crate_dir/*.rs" 2>/dev/null \
      | xargs -I {} wc -l {} 2>/dev/null \
      | awk 'NR > 0 { sum += $1 } END { print sum + 0 }'
  )
  if [ "$total" -gt "$MAX" ]; then
    printf 'FAIL  %s  %d LOC (max %d)\n' "$crate_dir" "$total" "$MAX"
    violations=$((violations + 1))
  fi
done < <(package_manifests)

if [ "$violations" -gt 0 ]; then
  printf '\n%d crate(s) over the %d-LOC limit.\n' "$violations" "$MAX" >&2
  exit 1
fi

echo "OK  all crates <= ${MAX} LOC"
