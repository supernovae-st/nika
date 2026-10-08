#!/usr/bin/env bash
# COVERS: scripts/ci/check-crate-size.sh scripts/ci/crate-size-policy.sh
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$HERE/../../ci/test-crate-size-source.py"
