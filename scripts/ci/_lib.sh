#!/usr/bin/env bash
# Shared helpers for diamond CI ratchets.
# Source, do not execute.

set -euo pipefail

# List tracked .rs files under any */src/ directory (excluding tests/benches/examples).
rs_src_files() {
  git ls-files '*.rs' | grep -E '(^|/)src/' | grep -vE '(^|/)(tests|benches|examples)/' || true
}

# List every tracked .rs file (any location).
rs_all_files() {
  git ls-files '*.rs' || true
}

# List every tracked Cargo.toml that declares a [package] (i.e. not the workspace root).
package_manifests() {
  git ls-files '*Cargo.toml' 'Cargo.toml' \
    | while read -r m; do
        [ "$m" = "Cargo.toml" ] && continue
        grep -q '^\[package\]' "$m" 2>/dev/null && printf '%s\n' "$m"
      done || true
}
