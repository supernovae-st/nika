#!/usr/bin/env bash
# Vector 11: zero .unwrap()/.expect() in PRODUCTION src/ (non-test code).
#
# ONE RULE, ONE READER (#1207). This vector used to carry a private python
# re-implementation of the test-item scope — the third of four copies of one
# rule. It read `#[cfg(test)]` as an attribute waiting for a brace, so a
# declaration —
#
#     #[cfg(test)]
#     mod tests;
#
# — left the flag pending and adopted the NEXT block as the "test" region,
# hiding any production `.unwrap()` / `.expect(` inside it. The silent
# direction: no false finding, just real ones that stop being reported.
#
# It now delegates to the two `scripts/ci/` ratchets, which read through
# `_lib.sh::strip_test_items` — the one filter that ends a pending item at
# `;` AND blanks string, raw-string and char literals before counting braces.
# Both lessons were learned there and neither reached the copies. Delegating
# means this vector cannot drift from what CI enforces, because it is now the
# same code rather than a faithful-looking twin.
#
# Exit codes (the hygiene contract, NOT the ci one):
#   0 — green · 1 — yellow · 2 — RED
# The ci ratchets exit 1 on a finding. Passing that through would render a
# real violation as YELLOW on the board — a failure read as a warning. The
# mapping below is deliberate and load-bearing.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CI="$HERE/../ci"

red=0
detail=""

for gate in check-unwrap.sh check-expect.sh; do
  out="$(bash "$CI/$gate" 2>&1)"
  rc=$?
  case $rc in
    0) : ;; # this half is clean
    1)
      red=1
      detail="${detail}${out}"$'\n'
      ;; # a finding
    *)
      red=1
      detail="${detail}${gate} exited ${rc}: ${out}"$'\n'
      ;;
  esac
done

if [ "$red" -eq 0 ]; then
  echo "OK (0 production unwrap/expect — both ci ratchets green through the shared filter)"
  exit 0
fi

printf '%s' "$detail"
exit 2
