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

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT" || exit

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

# Portable timeout wrapper — uses `timeout` on Linux, `gtimeout` on macOS (brew coreutils),
# or falls back to running without timeout if neither is available.
#
# The timeout is a HANG guard, NOT a perf gate: every vector is a correctness
# check (schema validity, layer discipline, …) whose only failure mode should be
# a real violation. A vector timing out under load (e.g. a concurrent sibling
# `cargo` build pinning the CPU) is a FALSE red that blocks a correct push — the
# very "social noise" ADR-090 exists to kill. So the ceiling is generous (60s,
# ~5x the slowest vector's ~12s baseline) and env-overridable for extreme load.
# Still bounded so a genuinely-hung vector can't wedge the suite.
# 300s default · the 60s floor then the 120s floor both produced false-RED
# timeouts on the cargo-walking vectors (adr-schema-valid · doc-private-items):
# `cargo doc --document-private-items` over 37 crates is ~128s cold and exceeds
# 120s under any concurrent compile, false-RED-ing the pre-push gate (the
# recurring "engine red" push blocker · 2026-06-11 → 2026-06-14 stress-to-ratchet).
# This is a HANG guard, not a perf gate, so 300s is safe. Raise per-run via
# HYGIENE_VECTOR_TIMEOUT_SECS.
VECTOR_TIMEOUT_SECS="${HYGIENE_VECTOR_TIMEOUT_SECS:-300}"
TIMEOUT_CMD=""
if command -v timeout >/dev/null 2>&1; then
  TIMEOUT_CMD="timeout $VECTOR_TIMEOUT_SECS"
elif command -v gtimeout >/dev/null 2>&1; then
  TIMEOUT_CMD="gtimeout $VECTOR_TIMEOUT_SECS"
fi

run_check() {
  local name="$1"
  local script="scripts/hygiene/$2"
  if [ -x "$script" ]; then
    if [ -n "$TIMEOUT_CMD" ]; then
      output=$($TIMEOUT_CMD "$script" 2>&1)
    else
      output=$("$script" 2>&1)
    fi
    status=$?
    case $status in
      0) record "$name" "green" "${output:-OK}" ;;
      1) record "$name" "yellow" "${output:-warn}" ;;
      2) record "$name" "red" "${output:-fail}" ;;
      124) record "$name" "red" "timeout after ${VECTOR_TIMEOUT_SECS}s (raise HYGIENE_VECTOR_TIMEOUT_SECS if under load)" ;;
      127) record "$name" "red" "command not found" ;;
      *) record "$name" "red" "exit $status: ${output:-unknown}" ;;
    esac
  else
    record "$name" "yellow" "check script missing: $script"
  fi
}

# --- The live vectors. The count is NOT written here: this comment said
# "38 live", the README said "37 drift vectors", and there were 46
# run_check calls. Three surfaces, three numbers, one truth. It derives:
#   grep -c '^run_check ' scripts/hygiene/check-all.sh
# Numbering keeps its gaps — renumbering is churn for no value. ---
run_check "1  memory-head-sha       " "check-memory-head.sh"
run_check "2  crate-count           " "check-crate-count.sh"
run_check "3  loc-totals            " "check-loc.sh"
run_check "4  changelog-dates       " "check-changelog-dates.sh"
# Vector 5 (roadmap-crate-status) removed 2026-08-14 — it grepped ROADMAP.md
# for `- [ ] <crate>`, a syntax that has NEVER existed in that file across
# 171 commits, so no repo state could make it fire and it reported OK on
# every run at pre-push and nightly. The parity it might have been re-aimed
# at (the ROADMAP census vs Cargo.toml) is already enforced by vector 23,
# proven by mutation: drop a crate from `wip = [...]` and 23 goes RED while
# 5 still said "OK (roadmap consistent)". Kept the numbering gap.
run_check "6  crate-spec-metrics    " "check-crate-specs.sh"
# Vector 7 (linear-issue-states) removed 2026-04-17 — was a no-op stub
# without LINEAR_API_KEY in the dev environment. Misleading GREEN/YELLOW
# depending on env state; no value. Linear integration, when it lands,
# will surface in the dashboard via its own MCP, not via hygiene.
run_check "8  gh-milestones         " "check-milestones.sh"
run_check "9  org-profile-repos     " "check-org-readme.sh"
run_check "10 license-agpl          " "check-license.sh"
run_check "11 unwraps-in-src        " "check-unwraps.sh"
run_check "12 file-loc-cap          " "check-file-loc.sh"
run_check "13 claude-coauthor-leak  " "check-claude-coauthor.sh"
run_check "14 private-path-leak     " "check-private-leaks.sh"
run_check "15 cargo-audit-rustsec   " "check-cargo-audit.sh"
run_check "16 adr-schema-valid     " "check-adr-schema.sh"
# Vector 17 (adr-supersede-cycles) removed 2026-05-30 — subsumed by vector 16
# check-adr-schema.sh → validate.sh Pass 3 (DAG supersession-cycle detection,
# bash-3.2-safe worklist). The dedicated check-adr-cycles.sh used `declare -A`
# (bash 4+); validate.sh Pass 3 is portable + self-contained.
# Vector 18 (adr-dangling-refs) removed 2026-04-17 — subsumed by vector 16
# → validate.sh Pass 2 (dangling-ref check across all 6 ref fields).
# Kept the numbering gap (renumbering 30+ vectors is churn for no value).
run_check "19 adr-orphan-proposed  " "check-adr-orphan-proposed.sh"
run_check "20 adr-evidence-paths   " "check-adr-evidence.sh"
run_check "21 layer-discipline     " "check-layering.sh"
run_check "22 no-async-in-l0      " "check-no-async-in-l0.sh"
run_check "23 status-claims-sync   " "check-status-claims-sync.sh"
run_check "24 crate-size-15k       " "check-crate-size.sh"
run_check "25 l0-dep-fanout        " "check-l0-dep-fanout.sh"
run_check "26 kernel-no-spawn     " "check-kernel-no-spawn.sh"
run_check "27 box-dyn-error-ban   " "check-box-dyn-error.sh"
run_check "28 doc-private-items   " "check-doc-private-items.sh"
run_check "29 case-collisions     " "check-case-collisions.sh"
run_check "30 cancel-safety-docs  " "check-cancel-safety.sh"
run_check "31 owned-strings-cat   " "check-owned-strings.sh"
run_check "32 unsafe-count-ratchet" "check-unsafe-count.sh"
run_check "33 layer-deps-bans     " "check-layer-deps.sh"
run_check "34 cargo-deny-policy   " "check-cargo-deny.sh"
run_check "35 adr-081-guards      " "check-adr-081-guards.sh"
run_check "36 unused-deps-machete " "check-unused-deps.sh"
run_check "37 error-one-voice     " "check-error-one-voice.sh"
run_check "38 public-api-coverage " "check-public-api-coverage.sh"
run_check "39 gate5-attestation   " "check-gate5-attestation.sh"
run_check "40 kernel-io-typed-err  " "check-kernel-io-typed-errors.sh"
run_check "41 canon-stale-terms    " "check-canon-stale-terms.sh"
run_check "42 seam-discipline      " "check-seam-discipline.sh"
run_check "43 adr-index-parity     " "check-adr-index-parity.sh"
run_check "44 script-path-refs     " "check-script-path-refs.sh"
run_check "45 taught-commands      " "check-taught-commands.sh"
run_check "46 kit-script-tests     " "check-kit-script-tests.sh"
run_check "47 version-surfaces     " "check-version-surfaces.sh"
run_check "48 agent-plugins-1.0.0  " "check-agent-plugins.sh"
run_check "49 hygiene-self-tests   " "check-hygiene-self-tests.sh"
run_check "50 release-gate-envelope" "check-release-gate-envelope.sh"
# 51 is about THIS CLONE, not the repo. Every other vector asks whether the
# tree is right; this one asks whether the tree's local enforcement is even
# reachable — because an unregistered `merge.ours.driver` makes `/estate.yaml
# merge=ours` a no-op IN SILENCE, and an uninstalled lefthook makes every gate
# above it inert with no message at all.
run_check "51 clone-armed          " "check-clone-armed.sh"

# --- Output ---
g=0
y=0
r=0
if [ "$FORMAT" = "json" ]; then
  printf '['
  first=1
  for line in "${RESULTS[@]}"; do
    IFS='|' read -r n s d <<<"$line"
    [ $first -eq 0 ] && printf ','
    first=0
    printf '{"vector":%s,"status":"%s","detail":%s}' \
      "$(printf '%s' "$n" | sed 's/^ *//' | jq -R .)" \
      "$s" \
      "$(printf '%s' "$d" | jq -R .)"
    case "$s" in green) g=$((g + 1)) ;; yellow) y=$((y + 1)) ;; red) r=$((r + 1)) ;; esac
  done
  printf ']\n'
else
  [ "$QUIET" -eq 0 ] && printf "\n%-28s %-8s %s\n" "VECTOR" "STATUS" "DETAIL"
  [ "$QUIET" -eq 0 ] && printf "%-28s %-8s %s\n" "─────────────────────────────" "──────" "────────────────────"
  for line in "${RESULTS[@]}"; do
    IFS='|' read -r n s d <<<"$line"
    case "$s" in
      green)
        color=$GREEN
        label="GREEN"
        g=$((g + 1))
        ;;
      yellow)
        color=$YELLOW
        label="YELLOW"
        y=$((y + 1))
        ;;
      red)
        color=$RED
        label="RED"
        r=$((r + 1))
        ;;
      *)
        color=$RED
        label="FAIL"
        r=$((r + 1))
        ;;
    esac
    if [ "$QUIET" -eq 0 ] || [ "$s" != "green" ]; then
      printf "%-28s ${color}%-8s${RESET} %s\n" "$n" "$label" "$d"
    fi
  done
  [ "$QUIET" -eq 0 ] && printf "\n%d green | %d yellow | %d red\n" "$g" "$y" "$r"
fi

# Exit code
if [ "$r" -gt 0 ]; then
  exit 2
elif [ "$y" -gt 0 ]; then
  exit 1
else
  exit 0
fi
