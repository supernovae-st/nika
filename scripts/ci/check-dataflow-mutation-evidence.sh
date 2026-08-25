#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Recompute the exact source+harness closure named by nika-dataflow's Gate-5
# attestation. Unlike a commit SHA this digest is not circular when the
# attestation itself is committed beside the tested sources.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

EVIDENCE="docs/mutation/nika-dataflow-2026-08-25.txt"
inputs=(
  Cargo.toml
  Cargo.lock
  crates/nika-cap/Cargo.toml
  crates/nika-cap/src/expr.rs
  crates/nika-cap/src/lib.rs
  crates/nika-dataflow/Cargo.toml
  scripts/ci/check-mutation-floor.sh
)
while IFS= read -r path; do
  inputs+=("$path")
done < <(find crates/nika-dataflow/src -type f -name '*.rs' -print | LC_ALL=C sort)

digest="$({
  for path in "${inputs[@]}"; do
    printf '%s\0' "$path"
    shasum -a 256 "$path"
  done
} | shasum -a 256 | awk '{print $1}')"

if [[ "${1:-}" == "--print" ]]; then
  printf '%s\n' "$digest"
  exit 0
fi

recorded="$(sed -nE 's/^input_sha256=([0-9a-f]{64})$/\1/p' "$EVIDENCE")"
if [[ -z "$recorded" ]]; then
  echo "missing input_sha256 in $EVIDENCE" >&2
  exit 2
fi
if [[ "$recorded" != "$digest" ]]; then
  echo "nika-dataflow mutation evidence is stale" >&2
  echo "  recorded=$recorded" >&2
  echo "  current =$digest" >&2
  exit 2
fi
echo "OK: nika-dataflow mutation evidence matches input tree $digest"
