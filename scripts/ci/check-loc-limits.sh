#!/usr/bin/env bash
# Ratchet: every tracked .rs file must be <= the file limit, unless a whitelisted LOC-EXEMPT marker
# carries it up to the exempt ceiling. The levels, the marker parser and the verdict live in
# scripts/ci/file-size-policy.sh, which hygiene vector 12 reads too (ADR-154).
# See CONSTELLATION_PLAN §11 rule 19.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./file-size-policy.sh
. "$HERE/file-size-policy.sh"

violations=0

while IFS= read -r f; do
  [ -z "$f" ] && continue
  loc=$(wc -l <"$f" | tr -d ' ')
  # Only a file above the limit can be refused; it alone pays for reading its marker.
  [ "$loc" -le "$FILE_LOC_LIMIT" ] && continue
  verdict=$(file_loc_verdict "$f" "$loc")
  case "$verdict" in
    block:*)
      printf 'FAIL  %s  %d LOC (%s)\n' "$f" "$loc" "$(file_loc_explain "$verdict")"
      violations=$((violations + 1))
      ;;
  esac
done < <(rs_all_files)

if [ "$violations" -gt 0 ]; then
  printf '\n%d file(s) over the file-size policy (ADR-154).\n' "$violations" >&2
  exit 1
fi

echo "OK  all .rs files <= ${FILE_LOC_LIMIT} LOC, or LOC-EXEMPT up to ${FILE_LOC_EXEMPT_CEILING}"
