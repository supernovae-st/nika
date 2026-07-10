#!/usr/bin/env bash
# activity-log.sh — Tier 3 post-commit (non-blocking)
#
# Appends a structured JSONL entry to memory/execution-roadmap/activity-log.jsonl
# for the commit that just completed.
#
# Entry format (one JSON object per line):
#   { "ts": "ISO8601", "sha": "short", "message": "header", "files": N, "author": "..." }
#
# Called from both root and engine contexts. Detects which repo it's in
# by looking for Cargo.toml at cwd (engine) vs absence (root monorepo).
#
# Non-blocking: any failure prints a warning but does not exit 1.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

# ---------------------------------------------------------------------------
# Detect context and log path (P1-9 Batch H+: use git toplevel, not cwd)
# ---------------------------------------------------------------------------
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf '[activity-log] not inside a git repo — skipping\n' >&2
  exit 0
}

if [[ -f "${REPO_ROOT}/Cargo.toml" ]] \
  && grep -q '^\[workspace\]' "${REPO_ROOT}/Cargo.toml" 2>/dev/null; then
  LOG_FILE="${REPO_ROOT}/memory/execution-roadmap/activity-log.jsonl"
else
  LOG_FILE="${REPO_ROOT}/dx/activity-log.jsonl"
fi

# Absent log dir = the log surface is retired here (memory/execution-roadmap
# left the engine tree) — the NORMAL state, not a failure. Skip silently:
# a warning on every commit is noise that trains people to ignore hooks.
LOG_DIR="$(dirname "$LOG_FILE")"
[[ -d "$LOG_DIR" ]] || exit 0

# ---------------------------------------------------------------------------
# Gather commit metadata
# ---------------------------------------------------------------------------
SHA="$(git rev-parse --short HEAD 2>/dev/null)" || {
  printf '[activity-log] git rev-parse failed\n' >&2
  exit 0
}
MSG="$(git log -1 --format='%s' 2>/dev/null)" || MSG='(unknown)'
AUTHOR="$(git log -1 --format='%an' 2>/dev/null)" || AUTHOR='(unknown)'
FILES_COUNT="$(git diff-tree --no-commit-id -r --name-only HEAD 2>/dev/null | wc -l | tr -d ' ')" || FILES_COUNT=0
TS="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null)" || TS='(unknown)'

# ---------------------------------------------------------------------------
# Escape string for JSON (minimal: quote and escape backslashes + double-quotes)
# ---------------------------------------------------------------------------
json_str() {
  local s="$1"
  # Escape backslash first, then double-quote
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  printf '"%s"' "$s"
}

ENTRY="{\"ts\":$(json_str "$TS"),\"sha\":$(json_str "$SHA"),\"message\":$(json_str "$MSG"),\"files\":${FILES_COUNT},\"author\":$(json_str "$AUTHOR")}"

# ---------------------------------------------------------------------------
# Append to log (append-safe, no read-modify-write race)
# ---------------------------------------------------------------------------
if printf '%s\n' "$ENTRY" >>"$LOG_FILE" 2>/dev/null; then
  printf '[activity-log] logged %s → %s\n' "$SHA" "$LOG_FILE" >&2
else
  printf '[activity-log] warning: could not write to %s\n' "$LOG_FILE" >&2
fi

exit 0
