#!/usr/bin/env bash
# shellcheck disable=SC1091  # _lib.sh sourced at runtime
# Ratchet: every member crate must have <= MAX LOC of PRODUCTION .rs source.
# Scope = src/ minus in-file #[cfg(test)] regions; tests/ + benches/ are
# excluded entirely. Rationale: the 15k invariant is a prod-code
# maintainability budget (CONSTELLATION_PLAN §7 criterion 3) — the mutation
# ratchet (>=90% killed) grows test mass by design, and a counter that
# charged tests against the prod budget made the two gates fight (empirical:
# nika-schema 2026-06-11 · 16.2k total = 10.1k prod + 6.1k cfg(test) — the
# tree was un-pushable while the prod code sat comfortably under cap).
# Same cfg(test)-scope parsing as check-unwraps.sh (vector 11).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

# Overridable ONLY so the hygiene dashboard's yellow band (vector 24 ·
# scripts/hygiene/check-crate-size.sh) can reuse THIS ONE counter with a
# lowered ceiling — two policies, one measure, zero duplication. CI never
# sets the var: the default keeps the ratchet strictly binary at 15k.
MAX="${CRATE_SIZE_MAX:-15000}"
violations=0

# The prod-file set is the SHARED one, computed ONCE for the whole workspace.
#
# It used to be re-implemented inline here — `git ls-files … | grep -v
# '/tests\.rs$'` — under a comment claiming to follow "the rs_prod_files
# convention in _lib.sh". One rule, three readers (this, _lib.sh,
# check-seam-discipline), and the copies drifted the moment the shared one
# learned something: on 2026-08-18 `rs_prod_files` started honouring modules
# declared under `#[cfg(test)]`, and this copy kept charging 951 lines of
# test-only code to five crates' production budget. The copy is gone; the
# measure is read from its single source.
PROD_FILES="$(rs_prod_files)"

while IFS= read -r manifest; do
  [ -z "$manifest" ] && continue
  crate_dir=$(dirname "$manifest")
  # Prod scope: src/ only, minus in-file #[cfg(test)] regions (counted by the
  # python block below) — the FILE-level exclusions are already applied by
  # rs_prod_files (basename `tests.rs` + `#[cfg(test)] mod` declarations).
  # `|| true` keeps an src-less crate from killing the loop under pipefail
  # (grep exits 1 on zero matches); python prints 0 on empty stdin then.
  total=$(
    { printf '%s\n' "$PROD_FILES" | grep -- "^$crate_dir/src/" || true; } \
      | python3 -c '
import re, sys
total = 0
for path in sys.stdin.read().split():
    try:
        lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
    except OSError:
        continue
    depth = 0
    test_entry = None   # brace depth at which a #[cfg(test)] item opened
    saw_cfg_test = False
    for line in lines:
        stripped = line.strip()
        if re.match(r"#\[cfg\(\s*(?:(?:all|any)\s*\(\s*)?test\b", stripped):
            saw_cfg_test = True
        in_test = test_entry is not None or saw_cfg_test
        if not in_test:
            total += 1
        row = re.sub(r"b?\x27(\\.|[^\x27\\])\x27", "", line)  # strip char literals
        for c in row:
            if c == "{":
                depth += 1
                if saw_cfg_test:
                    test_entry = depth
                    saw_cfg_test = False
            elif c == "}":
                depth -= 1
                if test_entry is not None and depth < test_entry:
                    test_entry = None
print(total)
'
  )
  if [ "$total" -gt "$MAX" ]; then
    printf 'FAIL  %s  %d LOC (max %d)\n' "$crate_dir" "$total" "$MAX"
    violations=$((violations + 1))
  fi
done < <(package_manifests)

if [ "$violations" -gt 0 ]; then
  printf '\n%d crate(s) over the %d-LOC limit.\n' "$violations" "$MAX" >&2
  exit 1
fi

echo "OK  all crates <= ${MAX} LOC"
