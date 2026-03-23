#!/usr/bin/env bash
# validate-nika.sh — PostToolUse hook for auto-validating .nika.yaml files
#
# Called by Claude Code after Write or Edit operations.
# Reads tool output from stdin (JSON), extracts file_path,
# and runs `nika check` if the file is a .nika.yaml workflow.
#
# Exit codes:
#   0 — Not a .nika.yaml file, or validation passed
#   2 — Validation failed (blocks Claude from continuing)
#
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (c) 2026 SuperNovae Studio

set -euo pipefail

# Read tool output from stdin
INPUT=$(cat)

# Extract file_path from JSON
FILE=$(printf '%s' "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('tool_input', {}).get('file_path', ''))
except Exception:
    print('')
" 2>/dev/null)

# Only validate .nika.yaml files
case "$FILE" in
  *.nika.yaml)
    # Check if nika binary exists
    if ! command -v nika &>/dev/null; then
      echo "Warning: nika binary not found. Skipping validation of $FILE" >&2
      exit 0
    fi

    # Check if the file exists (might have been a failed write)
    if [ ! -f "$FILE" ]; then
      echo "Warning: $FILE does not exist. Skipping validation." >&2
      exit 0
    fi

    # Run validation
    RESULT=$(nika check "$FILE" 2>&1)
    RC=$?

    if [ $RC -ne 0 ]; then
      echo "" >&2
      echo "=== NIKA VALIDATION FAILED ===" >&2
      echo "File: $FILE" >&2
      echo "" >&2
      echo "$RESULT" >&2
      echo "" >&2
      echo "Fix the issues above before continuing." >&2
      echo "Run 'nika check $FILE' for details." >&2
      echo "===============================" >&2
      exit 2
    else
      echo "Nika: $FILE validated successfully"
    fi
    ;;
  *)
    # Not a .nika.yaml file, skip
    exit 0
    ;;
esac
