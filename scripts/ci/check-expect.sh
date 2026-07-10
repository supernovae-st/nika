#!/usr/bin/env bash
# shellcheck disable=SC1091  # _lib.sh sourced at runtime
# Ratchet: zero `.expect(` calls in production src/.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. Mirrors clippy's
# `expect_used` lint scoping. This script promotes the workspace-level
# `expect_used = "warn"` to a hard block on the `nika-diamond` branch.
#
# Uses Python3 for context-aware detection: tracks brace depth to distinguish
# #[cfg(test)] modules and #[test] function bodies from production code.
# String and char literals are stripped before brace counting so braces
# inside string content do not unbalance the depth counter.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

# Load allowlist (P1-2 Batch H+: pre-existing violations surfaced by errexit fix)
ALLOWLIST_FILE="$HERE/allowlist-expect.conf"
_allowed_files=""
if [[ -f "$ALLOWLIST_FILE" ]]; then
  _allowed_files="$(grep -v '^#' "$ALLOWLIST_FILE" | grep -v '^$' || true)"
fi

# Python helper written to a temp script so it can be called per-file without
# bash here-doc-in-subshell limitations.
# No suffix after the Xs: BSD/macOS mktemp only randomizes TRAILING Xs, so
# `…_XXXXXX.py` degraded to a LITERAL name — two concurrent pre-push hooks
# (parallel worktree pushes) collided on it with `mkstemp failed: File
# exists`, blocking whichever push ran second (observed 2026-07-10). Python
# runs any filename; portability beats the extension.
_PY_CHECKER="$(mktemp /tmp/check_expect_XXXXXX)"
trap 'rm -f "$_PY_CHECKER"' EXIT

cat >"$_PY_CHECKER" <<'PYEOF'
import re, sys

def strip_str_lits(line):
    """Strip string/char literal CONTENTS for brace counting.
    Handles "...", b"...", r"...", br"..." and char literals b?'.'.
    Order matters: raw strings first (no backslash escaping, simpler pattern),
    then char literals (so a '"' char literal cannot start a fake string),
    then regular strings with negative lookbehind for remaining r/# prefixes.
    r#"..."# raw strings (hash-delimited) are not stripped here; their braces
    typically balance and the multi-hash terminator needs a more complex parser.
    """
    # Pass 0a: plain raw r"..." (word boundary ensures no mid-identifier match)
    s = re.sub(r'\br"[^"]*"', 'r""', line)
    # Pass 0b: byte raw br"..."
    s = re.sub(r'(?<!\w)br"[^"]*"', 'br""', s)
    # Pass 1: collapse char literals  b?'(\\.|[^'\\])' -> ''
    s = re.sub(r"b?'(\\.|[^'\\])'", "''", s)
    # Pass 2: collapse regular + byte string literals -> ""
    # Lookbehind (?<![r#]) skips r#"..."# and br#"..."# (char before " is r or #)
    # After pass 0a/0b, r"" and br"" are already collapsed so pass 2 won't re-match.
    s = re.sub(r'(?<![r#])b?"(?:\\.|[^"\\])*"', '""', s)
    # Pass 3: strip // line comments (after strings collapsed)
    m = s.find('//')
    if m >= 0:
        s = s[:m]
    return s

path = sys.argv[1]
with open(path, encoding='utf-8', errors='replace') as fh:
    lines = fh.readlines()

brace_depth = 0
cfg_test_entry = None   # brace_depth at entry of #[cfg(test)] mod
test_fn_entry  = None   # brace_depth at entry of #[test] fn body
saw_cfg_test   = False  # just saw #[cfg(test)] attribute
saw_test_attr  = False  # just saw #[test]/#[tokio::test] attribute
saw_test_fn    = False  # saw fn keyword after test attr (waiting for {)

for lineno, line in enumerate(lines, 1):
    s = line.strip()

    # --- detect scope markers BEFORE brace counting ---
    if re.match(r'#\[cfg\s*\(\s*(?!not\b)(?:(?:all|any)\s*\(\s*)?test\b', s):
        saw_cfg_test = True
    if re.match(r'#\[(?:(?:tokio::)?test|should_panic)(?:$|\s|\]|,)', s.rstrip()):
        saw_test_attr = True
    if saw_test_attr and not saw_test_fn and re.search(r'\bfn\s+\w', s):
        saw_test_fn = True

    # --- check for production expect BEFORE brace counting ---
    in_test_scope = (cfg_test_entry is not None) or (test_fn_entry is not None)
    if not in_test_scope and not s.startswith('//') and not s.startswith('/*'):
        if re.search(r'\.expect\s*\(', line):
            print(f'{lineno}:{s[:120]}')

    # --- count braces with literal stripping ---
    stripped = strip_str_lits(line)
    for ch in stripped:
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
PYEOF

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  # Skip allowlisted files
  if [ -n "$_allowed_files" ] && echo "$_allowed_files" | grep -qF "$f"; then
    continue
  fi
  hits=$(python3 "$_PY_CHECKER" "$f" || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_prod_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d .expect( call(s) in production src/\n' "$total" >&2
  exit 1
fi

echo "OK  zero .expect( in production src/"
