#!/usr/bin/env bash
# COVERS: scripts/release/wave-sweep.sh scripts/ci/check-version-uniform.sh
# Keep the release regression in the hygiene self-test loop so the test cannot
# sit green only because nobody invokes it.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$HERE/../../release/tests/release-tooling.test.sh"
