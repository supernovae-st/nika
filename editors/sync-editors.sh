#!/usr/bin/env bash
# sync-editors.sh — Keep the shared editor keyword cache in sync with Rust source.
#
# Runs editors/shared/extract-keywords.py to regenerate or verify the canonical
# nika-keywords.json consumed by the VS Code extension.
#
# Usage:
#   ./editors/sync-editors.sh          # Check drift (CI-friendly, exits 1 on drift)
#   ./editors/sync-editors.sh --fix    # Regenerate nika-keywords.json
#   ./editors/sync-editors.sh --help   # Show this help
#
# Exit codes:
#   0  In sync (or --fix applied successfully)
#   1  Drift detected
#   2  Fatal: missing dependency or source file
#
# AGPL-3.0-or-later — SuperNovae Studio

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
EXTRACT="$SCRIPT_DIR/shared/extract-keywords.py"
OUTPUT="$SCRIPT_DIR/shared/nika-keywords.json"

if ! command -v python3 &>/dev/null; then
  printf 'ERROR: python3 is required but not found in PATH\n' >&2
  exit 2
fi

if [[ ! -f "$EXTRACT" ]]; then
  printf 'ERROR: missing %s\n' "$EXTRACT" >&2
  exit 2
fi

case "${1:-check}" in
  --fix|-f|fix)
    python3 "$EXTRACT" > "$OUTPUT"
    printf '[ok] regenerated %s\n' "$OUTPUT"
    ;;
  --help|-h|help)
    sed -n '1,15p' "$0"
    ;;
  --check|-c|check|'')
    python3 "$EXTRACT" --check
    ;;
  *)
    printf 'Unknown argument: %s\n' "$1" >&2
    exit 2
    ;;
esac
