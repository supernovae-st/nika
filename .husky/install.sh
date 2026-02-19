#!/usr/bin/env sh

# Install git hooks for Nika
# Run this once after cloning: sh .husky/install.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "📦 Installing git hooks for Nika..."

# Configure git to use .husky as hooks directory
cd "$REPO_ROOT"
git config core.hooksPath .husky

echo "✅ Git hooks installed!"
echo ""
echo "Hooks enabled:"
echo "  - pre-commit: cargo fmt, clippy, .nika.yaml validation"
echo ""
echo "To uninstall: git config --unset core.hooksPath"
