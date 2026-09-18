#!/usr/bin/env bash
# check-source-naming.sh — Diamond PR ratchet for the live `*.nika` suffix.
#
# Live programs are lowercase `*.nika`. Retired dual-suffix names must
# not reappear in tracked paths or text except as exact pinned
# exceptions. `--selftest` plants alias / extra-file mutations in overlay
# only (no writes to the tree).
#
# diamond-ci.yml addresses a ratchet as `scripts/ci/check-<name>.sh`.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2
python3 "$REPO_ROOT/scripts/hygiene/check-source-naming.py"
python3 "$REPO_ROOT/scripts/hygiene/check-source-naming.py" --selftest
