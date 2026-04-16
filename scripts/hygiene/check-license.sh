#!/usr/bin/env bash
# Vector 10: LICENSE file present and is AGPL-3.0-or-later.
#
# Renamed 2026-04-16 from check-citation.sh — the name was misleading.
# The script never checked CITATION.cff (which doesn't exist in this repo);
# it only checks LICENSE presence + content. Rename aligns name with
# behavior.
#
# If/when CITATION.cff lands, add a parity check between its `version:`
# field and the workspace version.
#
# Exit codes:
#   0 — GREEN
#   2 — RED (LICENSE missing or wrong license)

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit 2

[ -f LICENSE ] || {
  echo "RED: LICENSE missing"
  exit 2
}
head -1 LICENSE | grep -qi 'AFFERO' || {
  echo "RED: LICENSE is not AGPL (first line: $(head -1 LICENSE))"
  exit 2
}
echo "OK (AGPL-3.0-or-later confirmed)"
exit 0
