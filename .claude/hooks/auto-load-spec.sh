#!/bin/bash
# Hook: UserPromptSubmit
# Purpose: Auto-load spec context when user mentions spec-related keywords
# Exit 0 + stdout = context injected

set -e

input=$(cat)
prompt=$(echo "$input" | jq -r '.prompt // ""' | tr '[:upper:]' '[:lower:]')

spec_path="$CLAUDE_PROJECT_DIR/spec/SPEC.md"

# Keywords that trigger spec injection
keywords=("spec" "schema" "action" "infer:" "exec:" "fetch:" "use:" "flow" "dag" "nika-" "workflow@")

should_inject=false
for keyword in "${keywords[@]}"; do
  if [[ "$prompt" == *"$keyword"* ]]; then
    should_inject=true
    break
  fi
done

if $should_inject && [[ -f "$spec_path" ]]; then
  echo "<spec-context source=\"spec/SPEC.md\" version=\"0.1\">"
  cat "$spec_path"
  echo "</spec-context>"
fi

exit 0
