#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-file-loc.sh scripts/ci/file-size-policy.sh
#
# Mutation proof for vector 12's three file tiers (ADR-023's tiers, ADR-154's
# values): a file above 1,500 lines warns and never blocks, a file above 2,500
# lines blocks unless a whitelisted LOC-EXEMPT marker carries it, and no marker
# carries a file above 5,000 lines. Each level is crossed only by MORE lines
# than it, so every exact boundary is a case too: a tier that moves by one line
# fails here.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-file-loc.sh"
POLICY="$ROOT/scripts/ci/file-size-policy.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# lines <n> [<marker>] prints n lines of Rust comments, the marker first.
lines() {
  local n="$1" marker="${2:-}"
  if [ -n "$marker" ]; then
    printf '%s\n' "$marker"
    n=$((n - 1))
  fi
  awk -v n="$n" 'BEGIN { for (i = 0; i < n; i++) print "// line" }'
}

# run_case <n> [<marker>] builds one crate with one n-line source file and runs
# the vector on it, leaving its output in $OUT and its exit code in $RC.
run_case() {
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  mkdir -p "$dir/scripts/hygiene" "$dir/scripts/ci" "$dir/crates/demo/src"
  cp "$VECTOR" "$dir/scripts/hygiene/"
  cp "$POLICY" "$dir/scripts/ci/"
  lines "$1" "${2:-}" >"$dir/crates/demo/src/lib.rs"
  OUT=$(bash "$dir/scripts/hygiene/check-file-loc.sh" 2>&1)
  RC=$?
}

# expect <want-rc> <label> <n> [<marker>]
expect() {
  local want="$1" label="$2"
  run_case "$3" "${4:-}"
  if [ "$RC" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$RC"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n%s\n' "$label" "$want" "$RC" "$OUT" >&2
  fi
}

# expect_says <needle> <label> <n> [<marker>] checks the FIRST line, the only
# one the hygiene dashboard renders.
expect_says() {
  local needle="$1" label="$2"
  run_case "$3" "${4:-}"
  case "$(printf '%s\n' "$OUT" | head -1)" in
    *"$needle"*) printf '  ok   %s\n' "$label" ;;
    *)
      fails=$((fails + 1))
      printf '  FAIL %s — first line lacks %s\n%s\n' "$label" "$needle" "$OUT" >&2
      ;;
  esac
}

echo "vector 12 · file-tier mutation proof"

expect 0 'a file of exactly 1,500 lines is green' 1500
expect 1 'a file above 1,500 lines warns and never blocks' 1501
expect 1 'a file of exactly 2,500 lines still only warns' 2500
expect_says 'YELLOW: file(s) in (1500, 2500]' 'the warning names its band' 1501
expect 2 'a file above 2,500 lines blocks' 2501
expect_says 'RED: file(s) > 2500 LOC' 'the block names the limit' 2501
expect 0 'a whitelisted LOC-EXEMPT marker carries a file above 2,500' 2501 \
  '// LOC-EXEMPT: codegen'
expect 0 'a marked file of exactly 5,000 lines passes' 5000 '// LOC-EXEMPT: lookup-table'
expect 2 'no marker carries a file above 5,000 lines' 5001 '// LOC-EXEMPT: codegen'
expect_says 'CRITICAL: file(s) > 5000 LOC' 'the ceiling names itself' 5001 \
  '// LOC-EXEMPT: codegen'
expect 2 'an unlisted LOC-EXEMPT reason blocks' 3000 '// LOC-EXEMPT: convenience'

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a file tier moved or stopped meaning what it says.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
