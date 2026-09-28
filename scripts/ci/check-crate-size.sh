#!/usr/bin/env bash
# shellcheck disable=SC1091  # _lib.sh sourced at runtime
# Ratchet: every member crate must have <= MAX LOC of production Rust + jq.
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

# The counter proves itself before it guards the wall — fail CLOSED, the same
# discipline `_lib.sh` applies to the two shared filters. A budget nobody can
# demonstrate is a number people argue with, and here the argument gets settled
# by deleting doc comments (#1203): a comment is the cheapest line to remove,
# so it is the first one removed, and the budget is satisfied by discarding
# exactly the thing it exists to protect.
if ! python3 "$HERE/test-prod-loc.py" >/dev/null 2>&1; then
  printf 'FAIL  the prod-LOC counter is not honest — this ratchet cannot be trusted:\n' >&2
  python3 "$HERE/test-prod-loc.py" >&2 || true
  exit 2
fi

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
MANIFESTS="$(package_manifests)"
if [ -z "$MANIFESTS" ]; then
  echo 'FAIL  no tracked package manifests — cannot measure crate source' >&2
  exit 2
fi
PROD_FILES="$(rs_prod_files)
$(git ls-files '*.jq' | grep -E '(^|/)src/' | grep -vE '(^|/)(tests|benches|examples)/' || true)"

while IFS= read -r manifest; do
  [ -z "$manifest" ] && continue
  crate_dir=$(dirname "$manifest")
  CRATE_FILES=$(printf '%s\n' "$PROD_FILES" | grep -- "^$crate_dir/src/" || true)
  if [ -z "$CRATE_FILES" ]; then
    # cargo-fuzz packages contain test targets outside src/, not production.
    # Require their tracked targets; a missing inventory is still an error.
    if grep -q '^cargo-fuzz = true$' "$manifest" \
      && [ -n "$(git ls-files "$crate_dir/fuzz_targets/*.rs")" ]; then
      printf 'SKIP  %s: cargo-fuzz test targets outside production src/ scope\n' "$crate_dir"
      continue
    fi
    printf 'FAIL  %s has no tracked production source — cannot measure it\n' "$crate_dir" >&2
    exit 2
  fi
  # Prod scope: src/ only, minus in-file #[cfg(test)] regions (counted by the
  # python block below) — the FILE-level exclusions are already applied by
  # rs_prod_files (basename `tests.rs` + `#[cfg(test)] mod` declarations).
  # An absent inventory is a refusal above, never a zero-LOC success. Embedded
  # jq uses raw lines; only Rust receives the cfg(test) item filter.
  # The counter lives in ONE proven file (prod-loc.py), not inline here.
  # It used to be inline, and it was blind twice over — braces inside string
  # literals ended a test module early (412 lines of `mod tests` charged to
  # production in nika-runtime/src/expr.rs alone), and `#[cfg(test)] mod foo;`
  # swallowed whichever block came next. One rule, one reader, one self-test.
  total=$(
    printf '%s\n' "$CRATE_FILES" \
      | python3 "$HERE/prod-loc.py" \
      | awk -F'\t' '{ sum += $1 } END { print sum + 0 }'
  ) || exit 2
  if [ "$total" -gt "$MAX" ]; then
    printf 'FAIL  %s  %d LOC (max %d)\n' "$crate_dir" "$total" "$MAX"
    violations=$((violations + 1))
  fi
done <<<"$MANIFESTS"

if [ "$violations" -gt 0 ]; then
  printf '\n%d crate(s) over the %d-LOC limit.\n' "$violations" "$MAX" >&2
  exit 1
fi

echo "OK  all crates <= ${MAX} LOC"
