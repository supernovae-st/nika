#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
# COVERS: scripts/hygiene/tests/clone-armed.test.sh
# A lane Git wrapper may answer discovery, but must not enter isolated fixtures.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_GIT="$(git --exec-path)/git"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
mkdir -p "$SCRATCH/lane with spaces"
cat >"$SCRATCH/lane with spaces/git" <<'SHIM'
#!/usr/bin/env bash
set -euo pipefail
if [ "$#" -eq 1 ] && [ "$1" = --exec-path ]; then
  exec "$TEST_REAL_GIT" --exec-path
fi
# Fail immediately, so a regressed self-test never creates a recursive fork storm.
printf 'FAIL: isolated fixture re-entered the lane Git wrapper\n' >&2
exit 97
SHIM
chmod +x "$SCRATCH/lane with spaces/git"
TEST_REAL_GIT="$REAL_GIT" PATH="$SCRATCH/lane with spaces:$PATH" \
  bash "$HERE/clone-armed.test.sh"
echo 'clone-armed wrapper isolation: passed'
