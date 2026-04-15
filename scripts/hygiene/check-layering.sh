#!/usr/bin/env bash
# check-layering.sh — hygiene wrapper around scripts/ci/check-layering.sh
#
# Exit codes (hygiene convention):
#   0 — green (all layers respected)
#   2 — red (at least one upward dep)
#
# The real enforcement logic lives in scripts/ci/check-layering.sh; this wrapper
# exists so the hygiene dashboard (check-all.sh) can run it alongside the other
# 20 vectors with a consistent summary line.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

OUTPUT=$(bash scripts/ci/check-layering.sh 2>&1)
STATUS=$?

case "$STATUS" in
  0)
    echo "OK: layer discipline respected"
    exit 0
    ;;
  *)
    echo "$OUTPUT" | head -3
    exit 2
    ;;
esac
