#!/usr/bin/env bash
# changelog-preview.sh — Tier 3 post-commit (non-blocking)
#
# Prints a preview of what the next changelog entry would look like,
# using git-cliff with the project's cliff.toml.
#
# Requires: git-cliff (https://git-cliff.org) — skips silently if not installed.
# Runs from inside nika/engine (cwd expected to have cliff.toml).
#
# Does NOT write to any file — print-only preview.
# Non-blocking: any error exits 0.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

# Require git-cliff
if ! command -v git-cliff >/dev/null 2>&1; then
  exit 0 # silently skip — not a hard dep
fi

# Require cliff.toml in cwd (engine-specific)
if [[ ! -f 'cliff.toml' ]]; then
  exit 0
fi

# Determine next tag from last tag + unreleased commits
LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || echo 'v0.80.0')"

printf '\n[changelog-preview] unreleased since %s:\n' "$LAST_TAG" >&2
git-cliff --unreleased --tag "$LAST_TAG" --strip all 2>/dev/null \
  | head -30 >&2 \
  || true

exit 0
