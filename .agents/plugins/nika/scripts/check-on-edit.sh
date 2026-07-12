#!/usr/bin/env bash
# check-on-edit — the plugin's seatbelt: after the agent edits a
# *.nika.yaml, run the audit so findings land in the hook log
# immediately (the file is the contract; check is the oracle).
#
# Cursor hook contract (docs/agent/hooks): input is JSON on STDIN
# (afterFileEdit carries file_path + edits); stdout is the JSON
# response; the afterFileEdit matcher filters by TOOL type, not by
# filename — so the extension filter lives HERE. Exit 0 always: an
# audit finding is the repair loop's next step, never an edit veto.
#
# Capability-honest: no nika binary means no verdict, never a failure.
set -euo pipefail

input="$(cat)"

# file_path from the stdin JSON — python3 when present, sed fallback.
if command -v python3 >/dev/null 2>&1; then
  file="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("file_path", ""))
except Exception:
    print("")' 2>/dev/null || true)"
else
  file="$(printf '%s' "$input" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi

case "$file" in
  *.nika.yaml | *.nika.yml) ;;
  *)
    printf '{}\n'
    exit 0
    ;;
esac

if [ ! -f "$file" ] || ! command -v nika >/dev/null 2>&1; then
  # Missing file or binary: nothing to audit (the skill teaches the
  # install line) — stay silent and let the edit flow.
  printf '{}\n'
  exit 0
fi

# Findings go to STDERR (the hook log); stdout stays valid JSON for
# the hook protocol. A red check never blocks the edit.
nika check "$file" --color never >&2 || true
printf '{}\n'
