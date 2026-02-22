#!/bin/bash
# Hook: PreToolUse (Bash)
# Purpose: Intercept git commit and run validation first
# Exit 0 = allow, Exit 1 = block with message

set -e

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$PWD}"

# Read the tool input
input=$(cat)
command=$(echo "$input" | jq -r '.tool_input.command // ""' 2>/dev/null || echo "")

# Only intercept git commit commands
if [[ ! "$command" =~ git[[:space:]]+commit ]]; then
  # Not a commit, allow through
  exit 0
fi

# This is a git commit - run pre-commit validation
echo "🔍 Nika Pre-Commit Validation triggered..."
echo ""

# Source or run the validation
if [[ -x "$PROJECT_DIR/.claude/hooks/nika-pre-commit.sh" ]]; then
  # Run validation, capture exit code
  if "$PROJECT_DIR/.claude/hooks/nika-pre-commit.sh"; then
    echo ""
    echo "✅ Validation passed. Proceeding with commit..."
    exit 0
  else
    echo ""
    echo "❌ Validation failed. Commit blocked."
    echo "Fix the issues above before committing."
    exit 1
  fi
else
  echo "⚠️ Pre-commit script not found, allowing commit"
  exit 0
fi
