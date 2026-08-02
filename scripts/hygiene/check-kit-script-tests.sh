#!/usr/bin/env bash
# Vector 46: the kit's shell rails self-test — and are RUN.
#
# `.agents/plugins/nika/scripts/` ships the hooks that land in a user's
# editor: the run guard, the session context, the on-edit check. They
# come with `tests/*.test.sh` beside them.
#
# Nothing ran them. `session-context.test.sh` had been sitting there
# green-by-never-executing, which is the same failure as a check that
# cannot see: a verdict nobody asked for is not a verdict (2026-08-02).
#
# This vector runs every `tests/*.test.sh` and fails on the first red.
# RED, not yellow: these scripts gate a `nika run` in somebody else's
# editor, and the guard's own contract says an unjudged run is denied —
# the rails that enforce that cannot ship unproven.
#
# A directory with no tests is YELLOW (say so — silence would read as
# passing), never green.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
DIR="$ROOT/.agents/plugins/nika/scripts/tests"

if [ ! -d "$DIR" ]; then
  echo "YELLOW: no kit script test directory at .agents/plugins/nika/scripts/tests"
  exit 1
fi

tests=$(find "$DIR" -name '*.test.sh' -type f | sort)
if [ -z "$tests" ]; then
  echo "YELLOW: .agents/plugins/nika/scripts/tests holds no *.test.sh"
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
  printf 'RED: %d of %d kit script test(s) failing — these rails ship into user editors\n' \
    "$fails" "$count" >&2
  exit 2
fi

printf 'OK: %d kit script test(s) green (the hooks that land in a user editor)\n' "$count"
