#!/usr/bin/env bash
# Vector 49: the vectors that carry a self-test are RUN.
#
# Sister to vector 46, one level up. Vector 46 runs the kit's shell rails'
# tests; this one runs the tests some hygiene vectors ship for themselves,
# in `scripts/hygiene/tests/*.test.sh`.
#
# The reason is the same failure that wrote 46: a test file sitting green
# by never executing. A hygiene vector is the last thing that should be
# trusted unproven — it is the surface whose whole job is to say "this is
# fine", and a gate nobody has ever seen fail is decoration. The tests
# here are mutation proofs: they break the tree on purpose and assert the
# vector goes red, so a vector that stopped catching anything is caught.
#
# A missing directory or an empty one is YELLOW (say so — silence would
# read as passing), never green.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIR="$HERE/tests"

if [ ! -d "$DIR" ]; then
  echo "YELLOW: no hygiene self-test directory at scripts/hygiene/tests"
  exit 1
fi

tests=$(find "$DIR" -name '*.test.sh' -type f | sort)
if [ -z "$tests" ]; then
  echo "YELLOW: scripts/hygiene/tests holds no *.test.sh"
  exit 1
fi

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
  printf '\n%d/%d hygiene self-test(s) failed — a vector no longer catches what it claims.\n' \
    "$fails" "$count" >&2
  exit 2
fi

echo "OK: $count hygiene self-test(s) passed"
exit 0
