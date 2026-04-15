#!/usr/bin/env bash
# Vector 16: Validate all ADR YAML frontmatter (required fields, format).
# Delegates to scripts/adr/validate.sh.
#
# Exit codes:
#   0 -- GREEN (all ADRs valid)
#   1 -- YELLOW (warnings only, e.g. bidirectional mismatches)
#   2 -- RED (validation errors)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit

output=$(scripts/adr/validate.sh 2>&1)
status=$?

echo "$output"
exit "$status"
