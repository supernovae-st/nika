#!/usr/bin/env bash
# Vector 49: the vectors that carry a self-test are RUN — and the board says
# how many DON'T carry one.
#
# Sister to vector 46, one level up. Vector 46 runs the kit's shell rails'
# tests; this one runs the tests some guards ship for themselves, in
# `scripts/hygiene/tests/*.test.sh`.
#
# The reason is the same failure that wrote 46: a test file sitting green by
# never executing. A guard is the last thing that should be trusted unproven
# — it is the surface whose whole job is to say "this is fine", and a gate
# nobody has ever seen fail is decoration. The tests here are mutation
# proofs: they break the tree on purpose and assert the guard goes red, so a
# guard that stopped catching anything is caught.
#
# 2026-08-15 — this used to end at `OK: N hygiene self-test(s) passed`. That
# is a count of tests that EXIST, not a coverage figure: it read the same
# whether 9 guards were proven or 45 were. The board carried 47 registered
# vectors and 9 proofs, and said nothing about the other 38. Counting what
# ran while the denominator stays offscreen is the shape this whole family
# of bugs takes, so the meta-vector had it too.
#
# Coverage is DECLARED, not guessed from filenames: one test can prove two
# guards (kernel-scope covers cancel-safety AND kernel-no-spawn) and its name
# matches neither. Each test carries `# COVERS: <path> [<path>...]`.
#
# Exit: 0 green (every registered vector proven) · 1 yellow (some unproven,
# listed — the ratchet target) · 2 red (a test failed, or a declaration is
# missing or names a guard that is not there).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
DIR="$HERE/tests"
BOARD="$HERE/check-all.sh"

if [ ! -d "$DIR" ]; then
  echo "YELLOW: no hygiene self-test directory at scripts/hygiene/tests"
  exit 1
fi

tests=$(find "$DIR" -name '*.test.sh' -type f | sort)
if [ -z "$tests" ]; then
  echo "YELLOW: scripts/hygiene/tests holds no *.test.sh"
  exit 1
fi

# --- run them ---------------------------------------------------------------
fails=0
count=0
for t in $tests; do
  count=$((count + 1))
  if out=$(bash "$t" 2>&1); then
    printf '  ok   %s\n' "$(basename "$t")"
  else
    fails=$((fails + 1))
    printf '  FAIL %s\n%s\n' "$(basename "$t")" "$out" >&2
  fi
done

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d hygiene self-test(s) failed — a guard no longer catches what it claims.\n' \
    "$fails" "$count" >&2
  exit 2
fi

# --- derive coverage --------------------------------------------------------
# Every guard a test DECLARES it proves. A test with no declaration cannot be
# counted, and an uncountable proof is how the denominator goes quiet — so
# that is RED, not a shrug.
undeclared=""
covered=""
for t in $tests; do
  line="$(grep -m1 '^# COVERS:' "$t" 2>/dev/null || true)"
  if [ -z "$line" ]; then
    undeclared="$undeclared $(basename "$t")"
    continue
  fi
  covered="$covered ${line#\# COVERS:}"
done

if [ -n "${undeclared# }" ]; then
  echo "RED: self-test(s) with no '# COVERS:' declaration — they run, but" >&2
  echo "     nothing can tell which guard they prove:" >&2
  for t in $undeclared; do echo "  · $t" >&2; done
  exit 2
fi

# A declaration that outlived its guard is the bug this whole board exists to
# catch. Name it here rather than let the coverage number quietly inflate.
stale=""
for g in $covered; do
  [ -f "$ROOT/$g" ] || stale="$stale $g"
done
if [ -n "${stale# }" ]; then
  echo "RED: a self-test declares it covers a guard that is not there:" >&2
  for g in $stale; do echo "  · $g" >&2; done
  echo "     (the guard moved or died — re-aim the COVERS line or drop the test)" >&2
  exit 2
fi

# --- the gap ----------------------------------------------------------------
# The denominator is the vectors this board actually runs. Tests that prove a
# hook or a CI ratchet still RUN above; they are named separately rather than
# silently dropped, because a scope that vanishes without a word reads as a
# scope that was cleared.
registered="$(grep -oE 'check-[a-z0-9-]+\.sh' "$BOARD" 2>/dev/null | sort -u)"
proven_vectors=""
off_board=0
for g in $covered; do
  base="$(basename "$g")"
  if printf '%s\n' "$registered" | grep -qxF "$base"; then
    proven_vectors="$proven_vectors $base"
  else
    off_board=$((off_board + 1))
  fi
done
proven_vectors="$(printf '%s' "$proven_vectors" | tr ' ' '\n' | grep -v '^$' | sort -u)"

total="$(printf '%s\n' "$registered" | grep -c . || true)"
proven="$(printf '%s\n' "$proven_vectors" | grep -c . || true)"

uncovered=""
for v in $registered; do
  printf '%s\n' "$proven_vectors" | grep -qxF "$v" || uncovered="$uncovered $v"
done

suffix=""
[ "$off_board" -gt 0 ] && suffix=" · plus $off_board proof(s) of hooks/CI guards off this board"

if [ -n "${uncovered# }" ]; then
  printf 'RATCHET (%d/%d board vectors carry a mutation proof%s) · unproven:\n' \
    "$proven" "$total" "$suffix"
  for v in $uncovered; do echo "  · $v"; done
  echo "  (a vector nobody has seen fail is decoration — add scripts/hygiene/tests/<name>.test.sh"
  echo "   with a '# COVERS:' line, both directions, and a negative control)"
  exit 1
fi

echo "OK: $count self-test(s) passed · all $total board vectors proven$suffix"
exit 0
