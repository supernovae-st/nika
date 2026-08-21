#!/usr/bin/env bash
# Ratchet: every tracked .rs file must be <= MAX lines.
# See CONSTELLATION_PLAN §11 rule 19.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

MAX=1500
violations=0

while IFS= read -r f; do
  [ -z "$f" ] && continue
  loc=$(wc -l <"$f" | tr -d ' ')
  if [ "$loc" -gt "$MAX" ]; then
    printf 'FAIL  %s  %d LOC (max %d)\n' "$f" "$loc" "$MAX"
    violations=$((violations + 1))
  fi
done < <(rs_all_files)

if [ "$violations" -gt 0 ]; then
  printf '\n%d file(s) over the %d-LOC limit.\n' "$violations" "$MAX" >&2
  exit 1
fi

echo "OK  all .rs files <= ${MAX} LOC"
