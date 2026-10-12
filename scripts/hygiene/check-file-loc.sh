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
# ADR-154 raised the tiers from 800 / 1500 / 3000. Each level is crossed only
# by MORE lines than it: a file of exactly 1500 is green, exactly 2500 is
# yellow, and with a valid marker exactly 5000 passes, as the CI ratchet
# (scripts/ci/check-loc-limits.sh, `-gt MAX`) and the crate wall read theirs.
#
# Per-function cap (100 LOC soft warn) is handled by `clippy::too_many_lines`
# in the workspace lints — out of scope for this hygiene vector.
#
# Scope: scans crates/*/src/**/*.rs and crates/*/build.rs and crates/*/build/*.rs.
# Does NOT scan tests/ (often legitimately long with table-driven cases),
# benches/, or examples/.
#
# Exit codes:
#   0 -- GREEN  (no file > 1500 LOC)
#   1 -- YELLOW (one or more files in (1500, 2500] — informational)
#   2 -- RED    (one or more files > 2500 LOC without valid LOC-EXEMPT,
#                OR LOC-EXEMPT file > 5000, OR LOC-EXEMPT with bad reason)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

WARN_THRESHOLD=1500
FAIL_THRESHOLD=2500
CRITICAL_THRESHOLD=5000

# Whitelisted reasons for LOC-EXEMPT marker. Anything else = RED.
ALLOWED_REASONS="codegen lookup-table enum-mega"

# Check if file has valid LOC-EXEMPT marker in first 5 lines.
# Sets HAS_EXEMPT=1 + EXEMPT_REASON globals when found.
# Sets HAS_EXEMPT=0 otherwise.
check_loc_exempt() {
  local file="$1"
  HAS_EXEMPT=0
  EXEMPT_REASON=""

  local marker_line
  marker_line=$(head -5 "$file" | grep -E '^//[[:space:]]*LOC-EXEMPT:' | head -1)
  if [ -z "$marker_line" ]; then
    return
  fi

  HAS_EXEMPT=1
  EXEMPT_REASON=$(echo "$marker_line" | sed -E 's|^//[[:space:]]*LOC-EXEMPT:[[:space:]]*([a-z-]+).*|\1|')
}

# Validate exempt reason against allowlist.
is_allowed_reason() {
  local reason="$1"
  for r in $ALLOWED_REASONS; do
    [ "$reason" = "$r" ] && return 0
  done
  return 1
}

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

  if [ "$loc" -le "$WARN_THRESHOLD" ]; then
    continue
  fi

  if [ "$loc" -le "$FAIL_THRESHOLD" ]; then
    yellows+="\n  $file: $loc LOC (> $WARN_THRESHOLD warn)"
    continue
  fi

  # > FAIL_THRESHOLD — needs LOC-EXEMPT to survive
  check_loc_exempt "$file"

  if [ "$loc" -gt "$CRITICAL_THRESHOLD" ]; then
    if [ "$HAS_EXEMPT" -eq 1 ] && is_allowed_reason "$EXEMPT_REASON"; then
      criticals+="\n  $file: $loc LOC (> $CRITICAL_THRESHOLD CRITICAL — exempt with reason '$EXEMPT_REASON' but absolute cap exceeded)"
    else
      criticals+="\n  $file: $loc LOC (> $CRITICAL_THRESHOLD CRITICAL — no valid exempt)"
    fi
    continue
  fi

  # > FAIL_THRESHOLD and ≤ CRITICAL_THRESHOLD
  if [ "$HAS_EXEMPT" -eq 0 ]; then
    reds+="\n  $file: $loc LOC (> $FAIL_THRESHOLD fail — no LOC-EXEMPT marker)"
  elif ! is_allowed_reason "$EXEMPT_REASON"; then
    reds+="\n  $file: $loc LOC (LOC-EXEMPT reason '$EXEMPT_REASON' not in allowlist: $ALLOWED_REASONS)"
  fi
  # else: valid exempt, in (2500, 5000] — allowed, no entry
done

# Critical wins over red wins over yellow.
if [ -n "$criticals" ]; then
  printf "CRITICAL: file(s) > %d LOC:" "$CRITICAL_THRESHOLD"
  printf "%b\n" "$criticals"
  echo ""
  echo "Hint: $CRITICAL_THRESHOLD LOC is the absolute cap — split the file regardless of marker."
  exit 2
fi

if [ -n "$reds" ]; then
  printf "RED: file(s) > %d LOC without valid LOC-EXEMPT:" "$FAIL_THRESHOLD"
  printf "%b\n" "$reds"
  echo ""
  echo "Hint: split the file into modules, OR add a marker on line 1-5:"
  echo "  // LOC-EXEMPT: <reason> [<owner>]"
  echo "Allowed reasons: $ALLOWED_REASONS"
  echo "See ADR-023 for the policy and ADR-154 for the values."
  exit 2
fi

if [ -n "$yellows" ]; then
  printf "YELLOW: file(s) in (%d, %d] LOC range (cohesion suggestion):" \
    "$WARN_THRESHOLD" "$FAIL_THRESHOLD"
  printf "%b\n" "$yellows"
  echo ""
  echo "Hint: consider splitting at module boundaries before crossing the"
  echo "$FAIL_THRESHOLD hard cap. Not blocking."
  exit 1
fi

echo "OK: ${#FILES[@]} file(s) scanned, none over ${WARN_THRESHOLD} LOC"
exit 0
