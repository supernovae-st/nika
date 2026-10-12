#!/usr/bin/env bash
# The reviewed production-LOC ceiling, shared by the gate and its dashboard.
# ADR-154 sets ONE ceiling for every crate, 25,000 production lines. It absorbs
# the scoped native-renderer ceiling of ADR-143's amendments (19,000 for
# crates/nika-tui), so no crate has an exception any more.
# Sourcing this file does not change the caller's shell options.

# crate_size_limit <crate-dir> [<probe>] prints the ceiling for <crate-dir>.
# Every crate shares the one ceiling; the directory argument stays so callers
# keep naming the crate they measure.
crate_size_limit() {
  local probe="${2:-}" limit=25000
  if [ -n "$probe" ]; then
    if [[ ! "$probe" =~ ^[1-9][0-9]*$ ]]; then
      echo 'FAIL  CRATE_SIZE_MAX must be a positive integer' >&2
      return 2
    fi
    # A probe may tighten the reviewed ceiling, never raise it.
    if [ "$probe" -lt "$limit" ]; then
      limit="$probe"
    fi
  fi
  printf '%s\n' "$limit"
}
