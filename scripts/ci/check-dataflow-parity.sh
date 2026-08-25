#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Deterministic pre/post differential proof for the nika-dataflow descent.
# The immutable pre-split main SHA and the candidate commit receive the exact
# same crate-private probe; CEL, jq and RuntimeError wrapper observations must
# then compare byte-for-byte.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PRE_SPLIT_SHA="25784444bfa84eab809a378580139cae32cd698b"
POST_SPLIT_REF="${1:-HEAD}"
PROBE="$ROOT/scripts/ci/dataflow-parity-probe.rs"
PROBE_FILENAME="dataflow_parity_probe.rs"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/nika-dataflow-parity.XXXXXX")"
PRE="$TMP/pre"
POST="$TMP/post"
TARGET="$ROOT/target/dataflow-parity"

cleanup() {
  git -C "$ROOT" worktree remove --force "$PRE" >/dev/null 2>&1 || true
  git -C "$ROOT" worktree remove --force "$POST" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT

git -C "$ROOT" cat-file -e "${PRE_SPLIT_SHA}^{commit}"
git -C "$ROOT" cat-file -e "${POST_SPLIT_REF}^{commit}"
git -C "$ROOT" worktree add --detach "$PRE" "$PRE_SPLIT_SHA" >/dev/null
git -C "$ROOT" worktree add --detach "$POST" "$POST_SPLIT_REF" >/dev/null

inject_probe() {
  local tree="$1"
  cp "$PROBE" "$tree/crates/nika-runtime/src/$PROBE_FILENAME"
  printf '\n#[cfg(test)]\nmod dataflow_parity_probe;\n' >>"$tree/crates/nika-runtime/src/lib.rs"
}

run_probe() {
  local tree="$1"
  local side="$2"
  if [[ "$side" == post ]]; then
    RUSTFLAGS="${RUSTFLAGS:-} --cfg dataflow_post_split" \
      CARGO_TARGET_DIR="$TARGET" cargo test \
      --manifest-path "$tree/Cargo.toml" \
      -p nika-runtime --lib \
      dataflow_parity_probe::pre_post_corpus -- --exact --nocapture 2>&1 \
      | grep '^PARITY|'
  else
    CARGO_TARGET_DIR="$TARGET" cargo test \
      --manifest-path "$tree/Cargo.toml" \
      -p nika-runtime --lib \
      dataflow_parity_probe::pre_post_corpus -- --exact --nocapture 2>&1 \
      | grep '^PARITY|'
  fi
}

inject_probe "$PRE"
inject_probe "$POST"
run_probe "$PRE" pre >"$TMP/pre.out"
run_probe "$POST" post >"$TMP/post.out"

if ! diff -u "$TMP/pre.out" "$TMP/post.out"; then
  echo "DATAFLOW PARITY FAILED: ${PRE_SPLIT_SHA} != ${POST_SPLIT_REF}" >&2
  exit 2
fi

lines="$(wc -l <"$TMP/post.out" | tr -d ' ')"
echo "OK: dataflow parity · ${lines} record/CEL/jq/error observations byte-identical"
echo "    pre=${PRE_SPLIT_SHA}"
echo "    post=$(git -C "$ROOT" rev-parse "${POST_SPLIT_REF}^{commit}")"
