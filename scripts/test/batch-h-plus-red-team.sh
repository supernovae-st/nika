#!/usr/bin/env bash
# shellcheck disable=SC2329  # many test_* fns dispatched by name via $fn var
# batch-h-plus-red-team.sh — self-test suite for Batch H+ hygiene gates
#
# Each fixture creates a throw-away git repo, simulates a scenario that
# SHOULD be blocked by a Batch H+ gate, runs the gate, and asserts the
# gate actually blocked (non-zero exit). Fixtures are added incrementally
# as P0/P1 fixes land — this file is always runnable and always green
# against the current tree.
#
# Usage:
#   cd nika/engine && bash scripts/test/batch-h-plus-red-team.sh
#
# Exit:
#   0  all fixtures behaved as expected
#   1  at least one fixture regressed (a gate stopped blocking)
#
# Prereqs: bash 4+, git, sed, awk. No brew-specific tools.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

# ──────────────────────────────────────────────────────────────────────────
# Framework
# ──────────────────────────────────────────────────────────────────────────

ENGINE_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
PASS=0
FAIL=0
SKIP=0
FAILURES=()

if [[ -t 1 ]]; then
  C_G=$'\033[32m'
  C_R=$'\033[31m'
  C_Y=$'\033[33m'
  C_D=$'\033[2m'
  C_0=$'\033[0m'
else
  C_G=''
  C_R=''
  C_Y=''
  C_D=''
  C_0=''
fi

mk_tmp_repo() {
  local dir
  dir="$(mktemp -d -t batch-h-plus-rt.XXXXXX)"
  (
    cd "$dir" || exit 1
    git init -q -b main
    git config user.email "redteam@batch-h-plus.test"
    git config user.name "redteam"
    git commit --allow-empty -q -m "baseline"
    git checkout -q -b nika-diamond
  )
  echo "$dir"
}

rm_tmp_repo() {
  if [[ -n "${1:-}" && -d "$1" && "$1" == /tmp/* ]]; then
    rm -rf "$1"
  fi
}

# Assert that invoking $fn returns 0 (meaning: the gate under test blocked
# the red-team scenario as expected).
assert_must_block() {
  local label="$1" fn="$2" rc=0
  "$fn" || rc=$?
  if [[ $rc -eq 0 ]]; then
    PASS=$((PASS + 1))
    printf '  %s✓%s %s\n' "$C_G" "$C_0" "$label"
  elif [[ $rc -eq 77 ]]; then
    SKIP=$((SKIP + 1))
    printf '  %s·%s %s %s(skip — fixture not yet implemented)%s\n' \
      "$C_Y" "$C_0" "$label" "$C_D" "$C_0"
  else
    FAIL=$((FAIL + 1))
    FAILURES+=("$label")
    printf '  %s✗%s %s\n' "$C_R" "$C_0" "$label"
  fi
}

# Assert that invoking $fn returns 0 (meaning: the gate ALLOWED a scenario
# that should legitimately pass — e.g., a .md file on the allowlist).
assert_must_allow() {
  assert_must_block "$1" "$2"
}

hook_must_block() {
  local rc=0
  "$@" >/dev/null 2>&1 || rc=$?
  [[ $rc -ne 0 ]]
}

hook_must_allow() {
  "$@" >/dev/null 2>&1
}

# ──────────────────────────────────────────────────────────────────────────
# P0 fixtures — added incrementally as each P0 fix lands
# ──────────────────────────────────────────────────────────────────────────

test_p0_1_basic_claude_trailer() { return 77; }
test_p0_1_claudia_human_contributor() { return 77; }
test_p0_1_claude_via_nika_relay() { return 77; }
test_p0_2_substring_only_not_trailer() { return 77; }
test_p0_3_dot_claude_projects_ref() { return 77; }
test_p0_4_monorepo_nika_hq_ref_in_dx() { return 77; }
test_p0_5_squash_drops_trailer() { return 77; }
test_p0_6_layer_registry_is_six_tier() { return 77; }

# Scenarios (fixtures flesh out as each P0 fix lands):
#   P0-1  vector 13 must block Claude trailer (basic + Claudia no-FP +
#         Claude-via-Nika relay cannot bypass)
#   P0-2  validate-conventional must reject substring-only "Nika" prose
#   P0-3  block-private-paths must catch .claude/projects/ reference
#         (grep -F + escaped regex = silent bypass today)
#   P0-4  monorepo variant must scan beyond engine-staged tree
#   P0-5  coauthor-squash-detect must warn when squash drops trailer
#   P0-6  crate-layer-registry.md must declare 6-tier L0..L5 canonical

# ──────────────────────────────────────────────────────────────────────────
# Runner
# ──────────────────────────────────────────────────────────────────────────

printf '\n%s🦋 Batch H+ red-team suite%s — %s\n\n' \
  "$C_G" "$C_0" "$(git -C "$ENGINE_DIR" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"

printf '  %sP0 · silently-broken gates%s\n' "$C_D" "$C_0"
assert_must_block "P0-1 · Claude trailer detected" test_p0_1_basic_claude_trailer
assert_must_allow "P0-1 · Claudia human contributor allowed" test_p0_1_claudia_human_contributor
assert_must_block "P0-1 · Claude-via-Nika-relay detected" test_p0_1_claude_via_nika_relay
assert_must_block "P0-2 · substring-only Nika ref rejected" test_p0_2_substring_only_not_trailer
assert_must_block "P0-3 · .claude/projects/ ref in engine .md" test_p0_3_dot_claude_projects_ref
assert_must_block "P0-4 · monorepo nika/hq/ ref in dx/" test_p0_4_monorepo_nika_hq_ref_in_dx
assert_must_block "P0-5 · squash drops trailer → warn" test_p0_5_squash_drops_trailer
assert_must_allow "P0-6 · crate-layer-registry 6-tier" test_p0_6_layer_registry_is_six_tier

printf '\n'
printf '  %sresult%s  %s%d passed%s   %s%d failed%s   %s%d skip%s\n\n' \
  "$C_D" "$C_0" "$C_G" "$PASS" "$C_0" "$C_R" "$FAIL" "$C_0" "$C_Y" "$SKIP" "$C_0"

if [[ $FAIL -gt 0 ]]; then
  printf '%sfailures:%s\n' "$C_R" "$C_0"
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
fi

exit 0
