#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 36: unused-dependency hygiene via cargo-machete.
#
# A dependency declared in Cargo.toml but never referenced is dep-rot: it
# inflates the supply-chain surface (more crates to audit), slows builds, and
# masks intent. For a sovereign forever-v0.x engine, every dep must earn its
# place. cargo-machete statically detects unused [dependencies].
#
# Scope: machete walks the workspace. The `exclude`d legacy main-leftover
# crates are not workspace members; findings confined to those are ignored
# (they are not part of the diamond workspace — see Cargo.toml exclude list).
#
# Exit: 0 green (no unused deps in admitted crates) · 2 red (unused dep found).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

# shellcheck source=scripts/ci/_lib.sh
. "$ENGINE_ROOT/scripts/ci/_lib.sh"

if ! command -v cargo-machete >/dev/null 2>&1; then
  echo "cargo-machete not installed (run: cargo install cargo-machete)"
  exit 2
fi

# Workspace members only (basenames), to filter out excluded legacy crates.
members="$(workspace_members)"

out="$(cargo machete 2>&1)"
status=$?

# machete exits 1 when it finds unused deps. Filter findings to workspace
# members (lines like "crates/nika-foo -- bar is unused" / a crate header).
if [ "$status" -ne 0 ]; then
  offenders="$(printf '%s\n' "$out" | grep -oE 'nika-[a-z0-9-]+' | sort -u \
    | while read -r c; do printf '%s\n' "$members" | grep -qx "$c" && echo "$c"; done)"
  if [ -n "$offenders" ]; then
    echo "unused dependencies in admitted crate(s):"
    printf '%s\n' "$out" | grep -iE 'unused|is unused|--' | head -20
    exit 2
  fi
  # Findings only in excluded/legacy crates → not our concern.
fi

echo "OK (no unused dependencies in admitted crates)"
exit 0
