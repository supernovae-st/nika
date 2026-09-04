#!/usr/bin/env bash
# COVERS: scripts/release/prepare-draft-release.sh
# The draft-release lookup's proof rides the hygiene self-test loop (vector
# 49) so it runs at every gate: the script it covers ran ONLY at tag time
# and died on its first release (v0.118.0 · 2026-09-04) — a gate that runs
# only at tag time is a fossil.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$HERE/../../release/tests/draft-release.test.sh"
