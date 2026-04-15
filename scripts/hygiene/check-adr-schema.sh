#!/usr/bin/env bash
# Vector 16: Validate all ADR YAML frontmatter (required fields, format).
# Delegates to scripts/adr/validate.sh.
#
# Exit codes:
#   0 -- GREEN (all ADRs valid)
#   2 -- RED (validation errors)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

output=$(scripts/adr/validate.sh 2>&1)
status=$?

if [ $status -eq 0 ]; then
  echo "$output"
  exit 0
else
  echo "$output" | tail -1
  exit 2
fi
