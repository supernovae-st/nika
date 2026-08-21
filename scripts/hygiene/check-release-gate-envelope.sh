#!/usr/bin/env bash
# Vector 50: the release gates speak the LIVE envelope.
#
# The two scripts that gate a tag's artifacts before upload — the funnel
# e2e (scripts/ci/funnel-e2e.sh) and the trust battery
# (scripts/test/trust-battery.sh) — author workflows inline and run them
# against the freshly built binary. They run ONLY at tag time, so a
# language change on main can leave them teaching the previous envelope
# for weeks with nothing red on any push, and the release run is where it
# shows: v0.106.0 died on all four builders because the battery spent an
# exec without a `permits:` block (2026-07-27), and v0.109.0 died on all
# four because both gates still wrote `nika: v1` + `workflow:` — the
# fourteen-key envelope the nine-key engine refuses (2026-08-18 · run
# 32190274141 · `[guard-dirty] missing: NIKA-SEC-014` · `[consent-run]
# exit=2 want=4`). Twice the binaries were fine and the gate was the
# fossil.
#
# This vector fires on every push: any dead envelope form in a release
# gate is RED. It is a grep, so it is honest about its reach — it catches
# the envelope, not a semantic drift; the semantic proof stays the funnel
# itself, run against a main build (`bash scripts/ci/funnel-e2e.sh <bin>`).
set -u
GATES=(scripts/ci/funnel-e2e.sh scripts/test/trust-battery.sh)
# The dead forms of the envelope · one regex each · a match anywhere in a
# gate script is a fixture that the live engine refuses at parse.
DEAD=(
  'nika: v1'      # the version marker · the identity IS the key now (nika: <id>)
  '\\nworkflow:'  # the printf form of the retired block
  '^workflow:'    # the heredoc form
  'on_finally:'   # cleanup is a task on an unwind edge
  'depends_on:'   # with:/after: since W2
  '\$\{\{ vars\.' # the E-split
  '\$\{\{ env\.'  # the E-split
)
red=0
for gate in "${GATES[@]}"; do
  [ -f "$gate" ] || {
    echo "release gate missing: $gate"
    exit 2
  }
  for form in "${DEAD[@]}"; do
    if grep -nE -- "$form" "$gate" >/dev/null 2>&1; then
      echo "RED: $gate carries a dead envelope form ($form) — the release run will refuse its own artifacts:"
      grep -nE -- "$form" "$gate" | head -3 | cut -c1-140
      red=1
    fi
  done
done
if [ "$red" -eq 1 ]; then
  exit 2
fi
echo "OK · ${#GATES[@]} release gate(s) speak the live envelope (${#DEAD[@]} dead forms absent)"
exit 0
