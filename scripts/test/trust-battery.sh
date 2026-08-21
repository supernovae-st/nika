#!/usr/bin/env bash
# trust-battery · the flight-recorder TRUST loop, end to end.
#
# The funnel plays the stranger's first path; THIS plays the operator's
# trust path — the claims the site/docs make about runs-as-evidence,
# each proven against a BUILT binary:
#
#   T1 run→resume     a second run over the trace skips on identity
#                     (visible cache hits) · --from re-runs the subtree
#   T2 reproduce      two runs of the same workflow classify REPRODUCED
#   T3 verify         both chains intact (exit 0)
#   T4 tamper         one edited byte → BROKEN + exit 2
#   T5 drop           one dropped line → BROKEN + exit 2
#   T6 export         OTLP projection lands beside the journal
#
# Offline · mock-only · throwaway workspace. Exit ≠0 iff any ✖.
# Usage: trust-battery.sh <nika-binary>
#
# Probe discipline (learned the hard way, 2026-07-10): every tamper
# MUST verify its bytes actually changed (a no-op edit proves nothing),
# and every exit code is read UNPIPED.
set -uo pipefail
BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
[ -x "$BIN" ] || {
  echo "no binary at $1"
  exit 2
}
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
cd "$ROOT" || exit 2
FAILS=0
fail() {
  echo "✖ $*"
  FAILS=$((FAILS + 1))
}
say() { printf '%s\n' "$*"; }

cat >wf.nika.yaml <<'YAML'
nika: trust
model: mock/echo
# F-O8 · absent permits is the EMPTY boundary since 0.106, and this fixture
# spends an exec. Without this block the battery's very first run refuses
# NIKA-AUTH-006, no trace exists, and every downstream probe cascades into
# empty-variable noise — exactly how the v0.106.0 release run died on all
# four builders (2026-07-27, run 30299180129) while the binaries themselves
# were fine. The infer task needs nothing: infer is not an effect
# (permits_fit.rs · RawAction::Infer(_) => {}).
permits:
  exec: ["echo"]
tasks:
  fetch:
    exec: { command: ["echo", "data-v1"] }
  think:
    with:
      fetch_out: ${{ tasks.fetch.output }}
    infer: { prompt: "sum ${{ with.fetch_out }}", max_tokens: 30 }
YAML

say "── trust battery · $("$BIN" --version)"

# T1 · run → resume (skips) → --from (subtree re-runs)
"$BIN" run wf.nika.yaml --json >run1.ndjson 2>/dev/null || fail "[T1] first run"
TRACE=$(/bin/ls -t .nika/traces/*.ndjson 2>/dev/null | head -1)
[ -n "$TRACE" ] || fail "[T1] no trace recorded"
OUT=$("$BIN" run wf.nika.yaml --resume "$TRACE" --color never 2>&1)
printf '%s' "$OUT" | grep -q "cache hit" || fail "[T1] resume shows no visible cache hit"
OUT=$("$BIN" run wf.nika.yaml --resume "$TRACE" --from think --color never 2>&1)
printf '%s' "$OUT" | grep -qE "1 skipped.*1 ran live|1 skipped.*2 ran live|ran live" \
  || fail "[T1] --from did not re-run the subtree: $OUT"

# T2 · reproduce: two runs of the same workflow → REPRODUCED
"$BIN" run wf.nika.yaml --json >run2.ndjson 2>/dev/null || fail "[T2] second run"
T_OLD=$(/bin/ls -t .nika/traces/*.ndjson | tail -1)
T_NEW=$(/bin/ls -t .nika/traces/*.ndjson | head -1)
OUT=$("$BIN" trace reproduce "$T_OLD" "$T_NEW" 2>&1)
RC=$?
printf '%s' "$OUT" | grep -q "REPRODUCED" || fail "[T2] not REPRODUCED: $OUT"
[ "$RC" -eq 0 ] || fail "[T2] reproduce exit $RC"

# T3 · both chains verify (UNPIPED exits)
"$BIN" trace verify "$T_OLD" >/dev/null 2>&1 || fail "[T3] old chain broken"
"$BIN" trace verify "$T_NEW" >/dev/null 2>&1 || fail "[T3] new chain broken"

# T4 · tamper ONE byte in a content-bearing line → BROKEN, exit 2.
# The probe asserts its own honesty: the bytes MUST differ.
python3 - "$T_NEW" <<'PY'
import sys
lines = open(sys.argv[1]).read().splitlines(True)
i, l = next((i, l) for i, l in enumerate(lines) if "mock" in l)
lines[i] = l.replace("mock", "hack", 1)
open("tampered.ndjson", "w").writelines(lines)
PY
cmp -s "$T_NEW" tampered.ndjson && fail "[T4] tamper was a NO-OP (probe bug)"
RC=0
"$BIN" trace verify tampered.ndjson >/dev/null 2>&1 || RC=$?
[ "$RC" -eq 2 ] || fail "[T4] tampered chain exited $RC, want 2"

# T5 · drop one line → BROKEN, exit 2
sed '4d' "$T_NEW" >dropped.ndjson
RC=0
"$BIN" trace verify dropped.ndjson >/dev/null 2>&1 || RC=$?
[ "$RC" -eq 2 ] || fail "[T5] dropped-line chain exited $RC, want 2"

# T6 · OTLP export lands
"$BIN" trace export "$T_NEW" >/dev/null 2>&1 || fail "[T6] export refused"
/bin/ls .nika/traces/*.otlp.jsonl >/dev/null 2>&1 || fail "[T6] no .otlp.jsonl beside the journal"

say "── trust battery: FAILS=$FAILS"
exit $((FAILS > 0))
