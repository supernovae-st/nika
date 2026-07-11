#!/bin/sh
# Consumer: the pre-commit FRAMEWORK (pre-commit.com) — NOT this repo's
# lefthook (its scripts live beside this one). `.pre-commit-hooks.yaml` at
# the repo root points here.
#
# pre-commit passes every staged `*.nika.yaml` as argv in ONE invocation;
# `nika check` audits ONE file per call (its report — human and --json —
# is a per-file contract). The hook fans out and keeps going so EVERY
# failing file reports in the same run, and the worst exit survives
# (spec §4 ladder: 3 environment > 2 findings > 1).
set -u

worst=0
for f in "$@"; do
  nika check "$f"
  rc=$?
  if [ "$rc" -gt "$worst" ]; then
    worst=$rc
  fi
done
exit "$worst"
