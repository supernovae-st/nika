#!/usr/bin/env bash
set -euo pipefail

# Bump version across all packages atomically.
# Usage: ./scripts/bump-version.sh 0.75.0
# Requires: jq, perl

NEW_VERSION="${1:?Usage: bump-version.sh <version>}"
echo "Bumping all packages to v${NEW_VERSION}"

# 1. Cargo workspace (portable sed via perl)
perl -i -pe "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" tools/Cargo.toml
echo "  Cargo.toml: ${NEW_VERSION}"

# 2. Sync npm + vscode
./scripts/sync-versions.sh

echo ""
echo "Done. Next steps:"
echo "  cargo check --workspace"
echo "  git add tools/Cargo.toml packages/ editors/vscode/package.json"
echo "  git commit -m 'chore: bump version to v${NEW_VERSION}'"
