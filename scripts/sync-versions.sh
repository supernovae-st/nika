#!/usr/bin/env bash
set -euo pipefail

# Sync all package versions to match tools/Cargo.toml workspace version.
# Usage: ./scripts/sync-versions.sh
# Requires: jq

if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is required. Install with: brew install jq" >&2
  exit 1
fi

VERSION=$(grep '^version' tools/Cargo.toml | head -1 | cut -d'"' -f2)
echo "Syncing all packages to v${VERSION}"

# npm platform packages
for pkg in packages/*/package.json; do
  jq --arg v "$VERSION" '.version = $v' "$pkg" > "${pkg}.tmp" && mv "${pkg}.tmp" "$pkg"
done

# npm main package optional dependencies
jq --arg v "$VERSION" '
  .optionalDependencies |= with_entries(.value = $v)
' packages/npm/package.json > packages/npm/package.json.tmp \
  && mv packages/npm/package.json.tmp packages/npm/package.json

# VS Code extension
jq --arg v "$VERSION" '.version = $v' editors/vscode/package.json \
  > editors/vscode/package.json.tmp \
  && mv editors/vscode/package.json.tmp editors/vscode/package.json

echo "Synced:"
echo "  Cargo:  ${VERSION}"
echo "  npm:    $(node -p "require('./packages/npm/package.json').version")"
echo "  vscode: $(node -p "require('./editors/vscode/package.json').version")"
