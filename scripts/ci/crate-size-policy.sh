#!/usr/bin/env bash
# Reviewed production-LOC ceilings, shared by the gate and its dashboard.
# ADR-143's 2026-10-08 amendment scopes the native composition exception.
# Sourcing this file does not change the caller's shell options.

crate_size_limit() {
  local crate="$1" probe="${2:-}" limit=15000
  case "$crate" in
    crates/nika-tui) limit=18000 ;;
  esac
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
