#!/usr/bin/env bash
# count-unwraps.sh — Wrapper for the Python unwrap ratchet counter
#
# Usage:
#   ./scripts/count-unwraps.sh          # Print current counts
#   ./scripts/count-unwraps.sh --check  # Compare against baseline, fail if increased
#   ./scripts/count-unwraps.sh --list   # Print all locations

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/count-unwraps.py" "$@"
