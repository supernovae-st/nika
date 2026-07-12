#!/usr/bin/env bash
# check-on-edit — the plugin's seatbelt: after the agent edits a
# *.nika.yaml, run the audit so findings reach the loop immediately
# (the file is the contract; check is the oracle).
#
# Capability-honest: no nika binary means no verdict, never a failure
# (exit 0 keeps the edit flowing; the skill teaches the install line).
# Read-only: check never executes the workflow.
set -euo pipefail

file="${CURSOR_FILE_PATH:-${1:-}}"
if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi
if ! command -v nika >/dev/null 2>&1; then
  echo "nika binary not found; skipping check (brew install supernovae-st/tap/nika)"
  exit 0
fi

# Findings go to stdout for the agent loop; a red check must not block
# the edit itself (the repair loop is the NEXT step, not a veto).
nika check "$file" --color never || true
