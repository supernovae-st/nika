#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-source-naming.sh scripts/hygiene/check-source-naming.py
#
# Mutation proof: injection into an excepted production file, a new
# retired path under a former prefix directory, and escaped/brace/glob
# aliases are red. The live tree stays green.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

if python3 "$ROOT/scripts/hygiene/check-source-naming.py" --selftest; then
  echo "  ok   source-naming selftest"
  exit 0
fi
echo "  FAIL source-naming selftest" >&2
exit 2
