#!/usr/bin/env bash
# Vector 52: live on-disk Nika programs are `*.nika`. Retired
# `.nika.yaml` / `.nika.yml` must not reappear in tracked pathnames or
# live text. Exceptions are exact files with pinned hit-count/hash or
# frozen whole-file digest — never a directory prefix.
#
# Exit: 0 green · 2 red.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2
exec python3 "$REPO_ROOT/scripts/hygiene/check-source-naming.py" "$@"
