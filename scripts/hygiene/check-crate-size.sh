#!/usr/bin/env bash
# Vector 24: per-crate LOC ratchet (≤ 15,000 LOC of tracked .rs sources).
#
# Wrapper around scripts/ci/check-crate-size.sh — same logic, translated
# to hygiene exit-code convention (0/1/2).
#
# This invariant is in nika-invariants.md ("Max LOC per crate: 15,000")
# but until now lived only in CI (`scripts/ci/`), not in the pre-push
# hygiene dashboard. Wired as vector 24 per Audit-2 P0 (2026-04-16).
#
# Exit codes:
#   0 — GREEN (all crates ≤ 15k LOC)
#   2 — RED   (at least one crate over budget — split required)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

OUTPUT=$(bash scripts/ci/check-crate-size.sh 2>&1)
STATUS=$?

case "$STATUS" in
  0)
    echo "OK: all admitted crates within 15,000 LOC budget"
    exit 0
    ;;
  *)
    # The CI script exits 1 on violation; promote to RED (2) for hygiene.
    echo "$OUTPUT" | head -10
    echo ""
    echo "Hint: split the over-budget crate into focused sub-crates per"
    echo "ADR-022 / ADR-024. The 15k LOC cap is a Diamond invariant"
    echo "(.claude/rules/nika-invariants.md)."
    exit 2
    ;;
esac
