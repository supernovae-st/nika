#!/usr/bin/env bash
# pre-push gate — the four heavy legs behind ONE stdin-aware door.
#
# git hands pre-push a line per ref on stdin:
#   <local ref> <local sha> <remote ref> <remote sha>
# A DELETION carries the zero sha as <local sha>: nothing new reaches the
# remote, so there is nothing to test — the gate skips instantly (the
# >90s hang on `git push origin --delete` was the full workspace suite
# running for a ref removal).
#
# FAIL-SAFE: if stdin carries no refs (lefthook not forwarding · manual
# invocation), the gate RUNS — a silent skip must be impossible.
set -uo pipefail

ZERO=0000000000000000000000000000000000000000
saw_ref=false
deletion_only=true
while read -r _local_ref local_sha _remote_ref _remote_sha; do
  [ -z "${local_sha:-}" ] && continue
  saw_ref=true
  if [ "$local_sha" != "$ZERO" ]; then
    deletion_only=false
  fi
done

if [ "$saw_ref" = true ] && [ "$deletion_only" = true ]; then
  echo "pre-push gate: deletion-only push — nothing new to test, skipping."
  exit 0
fi

if [ "${NIKA_GATE_DRYRUN:-}" = "1" ]; then
  echo "pre-push gate: WOULD RUN (saw_ref=$saw_ref deletion_only=$deletion_only)"
  exit 0
fi

set -e
cargo test --workspace --lib --quiet
cargo clippy --workspace --all-targets -- -D warnings
# hygiene: YELLOW (rc=1) passes with its stdout · only RED (rc=2) blocks —
# the exact contract the old inline leg carried.
rc=0
bash scripts/hygiene/check-all.sh --quiet || rc=$?
if [ "$rc" -eq 2 ]; then
  echo "engine hygiene RED — push blocked." >&2
  exit 1
fi
bash scripts/hooks/run-ci-ratchets.sh
