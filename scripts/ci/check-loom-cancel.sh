#!/usr/bin/env bash
# Prove CancelCtx publication with Loom; an absent model is a failed gate.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd -- "$REPO_ROOT"

export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cfg loom"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target}/loom-cancel"
# A developer's debugging bounds/checkpoint must not shorten the CI proof.
unset LOOM_MAX_PERMUTATIONS LOOM_MAX_DURATION LOOM_MAX_PREEMPTIONS LOOM_CHECKPOINT_FILE
listing="$(mktemp)"
trap 'rm -f -- "$listing"' EXIT

cargo test --locked -p nika-types --lib loom_cancel -- --list >"$listing"
if ! grep -qx 'cancel::loom_cancel::cancellation_publishes_preceding_payload: test' "$listing"; then
  cat "$listing"
  echo 'FAIL: the CancelCtx payload publication model was not discovered' >&2
  exit 1
fi
# Even an accidentally ignored model must execute in the proof gate.
cargo test --locked -p nika-types --lib loom_cancel -- --include-ignored
