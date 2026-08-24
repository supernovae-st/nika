#!/usr/bin/env bash
# check-changelog-fragments.sh — the ratchet entry for the changelog mechanism.
#
# `scripts/hooks/run-ci-ratchets.sh` and `diamond-ci.yml` both address a
# ratchet as `scripts/ci/check-<name>.sh`. The gate itself lives beside the
# thing it gates (`scripts/release/changelog-assemble.sh --check`), so there
# is ONE implementation and this file only routes to it — a second copy of
# the rules here is the hand-typed-mirror class, and it always drifts.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
exec bash "$SCRIPT_DIR/../release/changelog-assemble.sh" --check
