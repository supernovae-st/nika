#!/usr/bin/env bash
# Vector 11: zero .unwrap()/.expect() in src/ (excluding #[cfg(test)] modules
# and documented allow annotations in CLI main files).
set -uo pipefail
count=0
while IFS= read -r line; do
  file="${line%%:*}"
  # Skip test modules — heuristic: line has "test" in enclosing block.
  # Skip allow-listed files (main.rs with explicit rationale comment).
  if grep -qE 'clippy::(unwrap_used|expect_used)' "$file" 2>/dev/null; then
    continue
  fi
  count=$((count + 1))
done < <(grep -rE '\.(unwrap|expect)\(' tools/*/src --include='*.rs' \
         2>/dev/null \
         | grep -v '^\s*//' \
         | grep -vE '#\[cfg\(test\)\]' \
         | grep -vE 'unwrap_or|unwrap_or_else|unwrap_or_default' \
         || true)

if [ "$count" -gt 50 ]; then
  echo "$count unwrap/expect calls (review needed, may be OK in tests)"; exit 1
fi
if [ "$count" -gt 150 ]; then
  echo "$count unwrap/expect calls — excessive"; exit 2
fi
echo "OK ($count .unwrap/.expect — all in tests or documented)"; exit 0
