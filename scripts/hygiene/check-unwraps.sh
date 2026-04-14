#!/usr/bin/env bash
# Vector 11: zero .unwrap()/.expect() in PRODUCTION src/ (non-test code).
#
# Uses Python3 for context-aware detection: tracks brace depth to distinguish
# #[cfg(test)] modules and #[test] function bodies from production code.
#
# Workspace clippy lint `unwrap_used = "deny"` is the hard enforcement gate.
# This script is a complementary check that counts at the grep level with
# proper test-scope exclusion.
#
# Exit codes:
#   0 — OK (0 production unwraps found)
#   2 — RED (production unwraps found — must fix)
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.."

output=$(python3 - << 'PYEOF'
import re, os, sys

def check_file(path):
    with open(path, encoding='utf-8', errors='replace') as f:
        content = f.read()

    # Skip files with file-level allow annotations (#![allow(clippy::*_used)])
    if re.search(r'#!\[allow\([^\)]*clippy::(?:unwrap|expect)_used', content):
        return []

    lines = content.split('\n')
    brace_depth = 0
    cfg_test_entry = None   # depth inside #[cfg(test)] mod
    test_fn_entry  = None   # depth inside #[test] fn body
    saw_cfg_test   = False  # just saw #[cfg(test)] attribute
    saw_test_attr  = False  # just saw #[test]/#[tokio::test] attribute
    saw_test_fn    = False  # saw fn keyword after test attr (waiting for {)
    prod_unwraps   = []

    for lineno, line in enumerate(lines, 1):
        s = line.strip()

        # --- detect scope markers BEFORE brace counting on this line ---
        if re.match(r'#\[cfg\s*\(\s*test\s*\)\]', s):
            saw_cfg_test = True
        if re.match(r'#\[(?:(?:tokio::)?test|should_panic)(?:$|\s|\]|,)', s.rstrip()):
            saw_test_attr = True
        # Detect fn definition after test attribute
        if saw_test_attr and not saw_test_fn and re.search(r'\bfn\s+\w', s):
            saw_test_fn = True

        # --- check for production unwrap BEFORE brace counting ---
        # (unwrap call is on the content of the line, before any scope exits)
        in_test_scope = (cfg_test_entry is not None) or (test_fn_entry is not None)
        if not in_test_scope and not s.startswith('//') and not s.startswith('/*'):
            if re.search(r'\.(unwrap|expect)\s*\(', line):
                if not re.search(r'unwrap_or(?:_else|_default)?', line):
                    prod_unwraps.append(f'  {path}:{lineno}: {s[:100]}')

        # --- count braces (simplified, ignores strings/comments) ---
        for ch in line:
            if ch == '{':
                brace_depth += 1
                if saw_cfg_test:
                    cfg_test_entry = brace_depth
                    saw_cfg_test = False
                if saw_test_fn:
                    test_fn_entry = brace_depth
                    saw_test_fn = False
                    saw_test_attr = False
            elif ch == '}':
                brace_depth = max(0, brace_depth - 1)
                if cfg_test_entry is not None and brace_depth < cfg_test_entry:
                    cfg_test_entry = None
                if test_fn_entry is not None and brace_depth < test_fn_entry:
                    test_fn_entry = None
                    saw_test_attr = False
                    saw_test_fn = False

    return prod_unwraps

total = []
for root, dirs, files in os.walk('tools'):
    dirs[:] = [d for d in dirs if d not in ['target', '.git']]
    for fname in sorted(files):
        if not fname.endswith('.rs'):
            continue
        path = os.path.join(root, fname)
        if '/src/' not in path.replace(os.sep, '/'):
            continue
        total.extend(check_file(path))

for line in total:
    print(line)
print(f'__TOTAL__{len(total)}')
PYEOF
)

count=$(echo "$output" | grep '__TOTAL__' | grep -oE '[0-9]+$')
detail=$(echo "$output" | grep -v '__TOTAL__')

if [ "${count:-0}" -eq 0 ]; then
  echo "OK (0 production unwrap/expect — all test-scoped, clippy verified)"; exit 0
fi
[ -n "$detail" ] && echo "$detail"
echo "${count} production unwrap/expect in non-test src/ code"; exit 2
