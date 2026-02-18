#!/bin/bash
# Hook: PostToolUse (Edit|Write on src/ or spec/)
# Purpose: Warn if spec-code alignment might be affected
# Exit 0 = success, stdout shown in verbose mode

set -e

input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // ""')
file_path=$(echo "$input" | jq -r '.tool_input.file_path // ""')

# Only check Edit/Write on relevant files
if [[ "$tool_name" != "Edit" && "$tool_name" != "Write" ]]; then
  exit 0
fi

# Check if file is in src/ or spec/
if [[ "$file_path" != *"/src/"* && "$file_path" != *"/spec/"* ]]; then
  exit 0
fi

# Output alignment reminder (shown in verbose mode)
if [[ "$file_path" == *"/spec/"* ]]; then
  echo "[nika-alignment] Spec modified. Ensure code matches."
elif [[ "$file_path" == *"/src/ast/"* ]]; then
  echo "[nika-alignment] AST modified. Check spec Section 3-4."
elif [[ "$file_path" == *"/src/binding/"* ]]; then
  echo "[nika-alignment] Binding modified. Check spec Section 6-7."
elif [[ "$file_path" == *"/src/error"* ]]; then
  echo "[nika-alignment] Errors modified. Check spec Section 11."
fi

exit 0
