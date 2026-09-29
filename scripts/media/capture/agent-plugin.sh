#!/usr/bin/env bash
# agent-plugin.sh — capture what the agent-plugin clip shows: a coding
# agent's first draft of release-notes.nika, the audit that refuses it,
# the fixed file the audit passes, and a rehearsal run of that file.
#
# The draft guesses a tool name (`nika:read_file`); `nika check` must
# refuse it (exit 2, NIKA-BUILTIN-001 on that name). The fix names the
# real builtin (`nika:read`); `nika check` must pass it (exit 0, run
# ready). The run is `--model mock/echo`: a rehearsal that echoes the
# prompt, offline, never a real answer. Both checks use `--native-strict`,
# the flag the plugin's check command and check-on-edit hook pass.
#
# Everything runs in a scratch directory by relative names, so no path of
# this machine reaches a transcript. Re-running rewrites the same files;
# only the run's durations, trace name and chain differ between runs.
#
# Usage · bash scripts/media/capture/agent-plugin.sh   (nika on PATH)
# Input · scripts/media/fixtures/release-notes-draft.nika
#         scripts/media/fixtures/release-notes.nika
#         scripts/media/fixtures/release-notes-changelog.md
# Output · media/raw/agent-plugin-check-draft.txt   the refusal
#          media/raw/agent-plugin-check-fixed.txt   the clean re-check
#          media/raw/agent-plugin-run.txt           the rehearsal
#          media/raw/agent-plugin-files.txt         the project after it
#          media/raw/agent-plugin-exits.json        the three exit codes
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIX="scripts/media/fixtures"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}

fail() {
  echo "FATAL: $*" >&2
  exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp "$FIX/release-notes-changelog.md" "$WORK/CHANGELOG.md"

(
  cd "$WORK"

  # ── the draft: the audit must refuse the guessed tool ─────────────────
  cp "$ROOT/$FIX/release-notes-draft.nika" release-notes.nika
  draft_rc=0
  nika check --color never --native-strict release-notes.nika \
    >"$ROOT/$RAW/agent-plugin-check-draft.txt" 2>&1 || draft_rc=$?
  [ "$draft_rc" -eq 2 ] || fail "the draft must fail nika check with exit 2 (got $draft_rc)"
  # shellcheck disable=SC2016 # the diagnostic contains literal tool-name backticks
  grep -q '^ ✖ TOOLS .*NIKA-BUILTIN-001.*`nika:read_file`' "$ROOT/$RAW/agent-plugin-check-draft.txt" \
    || fail "the draft's refusal no longer names the guessed tool nika:read_file"

  # ── the fix: the same file name, the real builtin, a clean audit ──────
  cp "$ROOT/$FIX/release-notes.nika" release-notes.nika
  fixed_rc=0
  nika check --color never --native-strict release-notes.nika \
    >"$ROOT/$RAW/agent-plugin-check-fixed.txt" 2>&1 || fixed_rc=$?
  [ "$fixed_rc" -eq 0 ] || fail "the fixed file must pass nika check (got exit $fixed_rc)"
  grep -q '^ ✔ TOOLS ' "$ROOT/$RAW/agent-plugin-check-fixed.txt" \
    || fail "the re-check no longer passes the TOOLS vector"
  grep -q 'run ready ✔' "$ROOT/$RAW/agent-plugin-check-fixed.txt" \
    || fail "the fixed file is no longer run ready"

  # ── the rehearsal: mock/echo, offline ─────────────────────────────────
  run_rc=0
  nika run --no-progress --color never --model mock/echo release-notes.nika \
    >"$ROOT/$RAW/agent-plugin-run.txt" 2>&1 || run_rc=$?
  [ "$run_rc" -eq 0 ] || fail "the rehearsal must settle (got exit $run_rc)"
  grep -q '^rehearsal: mock/echo' "$ROOT/$RAW/agent-plugin-run.txt" \
    || fail "the run no longer says it is a rehearsal"
  grep -q '3/3 done' "$ROOT/$RAW/agent-plugin-run.txt" \
    || fail "the rehearsal no longer runs all three tasks"
  [ -f release-notes.md ] || fail "the rehearsal wrote no release-notes.md"

  # ── what the job leaves in the project ────────────────────────────────
  LC_ALL=C ls -1A >"$ROOT/$RAW/agent-plugin-files.txt"

  printf '{\n  "check_draft": %d,\n  "check_fixed": %d,\n  "run": %d\n}\n' \
    "$draft_rc" "$fixed_rc" "$run_rc" >"$ROOT/$RAW/agent-plugin-exits.json"
)

echo "agent-plugin captures refreshed."
