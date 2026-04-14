#!/usr/bin/env bash
# Nika Hygiene Dashboard — runs all drift checks.
#
# Usage:
#   ./scripts/hygiene/check-all.sh [--format=table|json] [--quiet]
#
# Exit codes:
#   0  — all green (no drift)
#   1  — at least one YELLOW (< 2% drift or < 48h lag)
#   2  — at least one RED (hard divergence)
#
# Authored 🦋 for ecosystem self-healing.

set -u

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

FORMAT="table"
QUIET=0
for arg in "$@"; do
  case "$arg" in
    --format=*) FORMAT="${arg#--format=}" ;;
    --quiet) QUIET=1 ;;
  esac
done

GREEN="\033[0;32m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
RESET="\033[0m"

declare -a RESULTS

record() {
  # record <vector> <status: green|yellow|red> <detail>
  RESULTS+=("$1|$2|$3")
}

run_check() {
  local name="$1"
  local script="scripts/hygiene/$2"
  if [ -x "$script" ]; then
    output=$("$script" 2>&1)
    status=$?
    case $status in
      0) record "$name" "green"  "${output:-OK}" ;;
      1) record "$name" "yellow" "${output:-warn}" ;;
      *) record "$name" "red"    "${output:-fail}" ;;
    esac
  else
    record "$name" "yellow" "check script missing: $script"
  fi
}

# --- All 10 vectors ---
run_check "1  memory-head-sha       " "check-memory-head.sh"
run_check "2  crate-count           " "check-crate-count.sh"
run_check "3  loc-totals            " "check-loc.sh"
run_check "4  changelog-dates       " "check-changelog-dates.sh"
run_check "5  roadmap-crate-status  " "check-roadmap-status.sh"
run_check "6  crate-spec-metrics    " "check-crate-specs.sh"
run_check "7  linear-issue-states   " "check-linear.sh"
run_check "8  gh-milestones         " "check-milestones.sh"
run_check "9  org-profile-repos     " "check-org-readme.sh"
run_check "10 citation-version      " "check-citation.sh"

# --- Output ---
g=0; y=0; r=0
if [ "$FORMAT" = "json" ]; then
  printf '['
  first=1
  for line in "${RESULTS[@]}"; do
    IFS='|' read -r n s d <<< "$line"
    [ $first -eq 0 ] && printf ','
    first=0
    printf '{"vector":%s,"status":"%s","detail":%s}' \
      "$(printf '%s' "$n" | sed 's/^ *//' | jq -R .)" \
      "$s" \
      "$(printf '%s' "$d" | jq -R .)"
    case "$s" in green) g=$((g+1));; yellow) y=$((y+1));; red) r=$((r+1));; esac
  done
  printf ']\n'
else
  [ "$QUIET" -eq 0 ] && printf "\n%-28s %-8s %s\n" "VECTOR" "STATUS" "DETAIL"
  [ "$QUIET" -eq 0 ] && printf "%-28s %-8s %s\n" "─────────────────────────────" "──────" "────────────────────"
  for line in "${RESULTS[@]}"; do
    IFS='|' read -r n s d <<< "$line"
    case "$s" in
      green)  color=$GREEN;  label="GREEN" ; g=$((g+1));;
      yellow) color=$YELLOW; label="YELLOW"; y=$((y+1));;
      red)    color=$RED;    label="RED"   ; r=$((r+1));;
    esac
    if [ "$QUIET" -eq 0 ] || [ "$s" != "green" ]; then
      printf "%-28s ${color}%-8s${RESET} %s\n" "$n" "$label" "$d"
    fi
  done
  [ "$QUIET" -eq 0 ] && printf "\n%d green | %d yellow | %d red\n" "$g" "$y" "$r"
fi

# Exit code
if [ "$r" -gt 0 ]; then exit 2
elif [ "$y" -gt 0 ]; then exit 1
else exit 0
fi
