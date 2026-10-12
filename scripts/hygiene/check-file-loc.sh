#!/usr/bin/env bash
# Vector 12: file-LOC discipline (the three tiers of ADR-023, values of ADR-154).
#
# Three thresholds, escalating consequence:
#   1500 LOC -- YELLOW above this size (cohesion suggestion, never blocks)
#   2500 LOC -- RED above this size (cannot push)
#   5000 LOC -- CRITICAL above this size (the ceiling even with a LOC-EXEMPT marker)
#
# `// LOC-EXEMPT: <reason> [<owner>]` marker on one of the first 5 lines
# of a .rs file allows the file to exceed 2500 (up to 5000). Reasons are
# whitelisted to prevent silent growth: codegen, lookup-table, enum-mega.
# Each level is crossed only by MORE lines than it.
#
# The levels, the marker parser and the verdict sentence live in ONE place,
# scripts/ci/file-size-policy.sh, which the CI file ratchet
# (scripts/ci/check-loc-limits.sh) reads too: both gates judge a file the
# same way and state the same sentence for it.
#
# Per-function cap (100 LOC soft warn) is handled by `clippy::too_many_lines`
# in the workspace lints — out of scope for this hygiene vector.
#
# Scope: scans crates/*/src/**/*.rs and crates/*/build.rs and crates/*/build/*.rs.
# Does NOT scan tests/ (often legitimately long with table-driven cases),
# benches/, or examples/.
#
# Exit codes:
#   0 -- GREEN  (no file > 1500 LOC, or only LOC-EXEMPT files within the ceiling)
#   1 -- YELLOW (one or more files in (1500, 2500] — informational)
#   2 -- RED    (one or more files > 2500 LOC without valid LOC-EXEMPT,
#                OR LOC-EXEMPT file > 5000, OR LOC-EXEMPT with bad reason)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2
# shellcheck source=scripts/ci/file-size-policy.sh
. "$REPO_ROOT/scripts/ci/file-size-policy.sh"

# Collect files. Sources of truth: src/, build.rs, build/.
mapfile -t FILES < <(
  find crates -path '*/src/*.rs' -not -path '*/target/*' 2>/dev/null
  find crates -name 'build.rs' -not -path '*/target/*' 2>/dev/null
  find crates -path '*/build/*.rs' -not -path '*/target/*' 2>/dev/null
)

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "WARN: no .rs files found under crates/"
  exit 1
fi

yellows=""
reds=""
criticals=""

for file in "${FILES[@]}"; do
  loc=$(wc -l <"$file" | tr -d ' ')
  [ "$loc" -le "$FILE_LOC_WARN" ] && continue
  verdict=$(file_loc_verdict "$file" "$loc")
  entry="\n  $file: $loc LOC ($(file_loc_explain "$verdict"))"
  case "$verdict" in
    warn) yellows+="$entry" ;;
    block:ceiling) criticals+="$entry" ;;
    block:*) reds+="$entry" ;;
  esac
  # green and exempt:<reason> (within the ceiling) add no entry.
done

# Critical wins over red wins over yellow.
if [ -n "$criticals" ]; then
  printf "CRITICAL: file(s) > %d LOC:" "$FILE_LOC_EXEMPT_CEILING"
  printf "%b\n" "$criticals"
  echo ""
  echo "Hint: $FILE_LOC_EXEMPT_CEILING LOC is the absolute cap — split the file regardless of marker."
  exit 2
fi

if [ -n "$reds" ]; then
  printf "RED: file(s) > %d LOC without valid LOC-EXEMPT:" "$FILE_LOC_LIMIT"
  printf "%b\n" "$reds"
  echo ""
  echo "Hint: split the file into modules, OR add a marker on line 1-5:"
  echo "  // LOC-EXEMPT: <reason> [<owner>]"
  echo "Allowed reasons: $FILE_LOC_EXEMPT_REASONS"
  echo "See ADR-023 for the policy and ADR-154 for the values."
  exit 2
fi

if [ -n "$yellows" ]; then
  printf "YELLOW: file(s) in (%d, %d] LOC range (cohesion suggestion):" \
    "$FILE_LOC_WARN" "$FILE_LOC_LIMIT"
  printf "%b\n" "$yellows"
  echo ""
  echo "Hint: consider splitting at module boundaries before crossing the"
  echo "$FILE_LOC_LIMIT hard cap. Not blocking."
  exit 1
fi

echo "OK: ${#FILES[@]} file(s) scanned, none over ${FILE_LOC_WARN} LOC outside a LOC-EXEMPT marker"
exit 0
