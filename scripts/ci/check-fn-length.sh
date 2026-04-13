#!/usr/bin/env bash
# Ratchet: every fn body must be <= MAX lines.
# Phase 0 heuristic (regex + brace-counting via Python). Phase 2 xtask will
# replace this with a proper `syn` AST walk.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

MAX=100
violations=0
had_files=0

while IFS= read -r f; do
  [ -z "$f" ] && continue
  had_files=1
  python3 - "$f" "$MAX" <<'PY' || violations=$((violations + 1))
import re, sys
path, max_lines = sys.argv[1], int(sys.argv[2])
with open(path, 'r', encoding='utf-8', errors='replace') as fh:
    lines = fh.read().splitlines()
fn_re = re.compile(
    r'^\s*(pub(\([^)]*\))?\s+)?(default\s+)?(async\s+)?(unsafe\s+)?(extern\s+"[^"]*"\s+)?(const\s+)?fn\s+([A-Za-z_]\w*)'
)
bad = 0
i = 0
n = len(lines)
while i < n:
    m = fn_re.match(lines[i])
    if not m:
        i += 1
        continue
    depth = 0
    started = False
    j = i
    while j < n:
        # crude brace count (ignores strings/comments — good enough for a ratchet)
        for c in lines[j]:
            if c == '{':
                depth += 1
                started = True
            elif c == '}':
                depth -= 1
        if started and depth <= 0:
            length = j - i + 1
            if length > max_lines:
                print(f"FAIL  {path}:{i+1}  fn '{m.group(8)}' is {length} lines (max {max_lines})")
                bad += 1
            break
        j += 1
    i = (j + 1) if j > i else (i + 1)
sys.exit(1 if bad else 0)
PY
done < <(rs_src_files)

if [ "$violations" -gt 0 ]; then
  printf '\n%d fn(s) over the %d-line limit.\n' "$violations" "$MAX" >&2
  exit 1
fi

if [ "$had_files" -eq 0 ]; then
  echo "OK  (no tracked src/ .rs files yet)"
else
  echo "OK  all fns <= ${MAX} lines"
fi
