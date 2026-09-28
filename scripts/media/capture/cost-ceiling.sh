#!/usr/bin/env bash
# cost-ceiling.sh — capture what the cost-ceiling clip shows, from the
# real binary: `nika check` pricing a cloud workflow before any call (the
# COST rung, UNBOUNDED until one line declares `max_tokens`, then a hard
# worst-case ceiling), and `nika run --max-cost-usd` refusing to start
# because the workflow's unavoidable floor already exceeds the budget.
#
# Offline and keyless by construction. Every nika call runs in a scratch
# directory under `env -i`: a scratch HOME, no provider key, no harness
# login, and a dead proxy, so no request could leave the machine even if
# a gate regressed. The budget sits below the floor, so the run must
# refuse (exit 2 · NIKA-1709) before any provider is touched. The script
# fails if it stops refusing, if the refusal is not the budget's, if the
# run leaves a journal behind, or if the rung stops pricing.
#
# The fixtures are copied under their workflow id, weekly-update.nika:
# the file a user keeps and edits in place, so every transcript names it.
#
# Usage · bash scripts/media/capture/cost-ceiling.sh   (nika on PATH)
# Output · media/raw/cost-ceiling-*
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIX="scripts/media/fixtures"
NAME="weekly-update.nika"
BUDGET="0.05"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}
BIN="$(dirname "$(command -v nika)")"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/home" "$TMP/work"

# The clean room: only these variables exist, none of them a key.
DEAD="http://127.0.0.1:9"
ROOM=(HOME="$TMP/home" PATH="$BIN:/usr/bin:/bin" TERM=dumb
  HTTPS_PROXY="$DEAD" HTTP_PROXY="$DEAD" ALL_PROXY="$DEAD"
  https_proxy="$DEAD" http_proxy="$DEAD" all_proxy="$DEAD")
room() { (cd "$TMP/work" && env -i "${ROOM[@]}" "$@"); }
fail() {
  echo "FATAL (cost-ceiling capture): $*" >&2
  exit 1
}
has() { grep -qF -- "$2" "$1" || fail "$1 lost: $2"; }

# ── 1 · as first written: priced, but no output limit ───────────────────
cp "$FIX/cost-unbounded.nika" "$TMP/work/$NAME"
room nika check --color never "$NAME" >"$RAW/cost-ceiling-check-unbounded.txt" 2>&1
has "$RAW/cost-ceiling-check-unbounded.txt" "no total ceiling"
has "$RAW/cost-ceiling-check-unbounded.txt" "UNBOUNDED — no max_tokens declared"
has "$RAW/cost-ceiling-check-unbounded.txt" "[cost] declare \`max_tokens\` on \`update\`"

# ── 2 · the one-line fix: max_tokens turns it into a hard ceiling ───────
cp "$FIX/cost-ceiling.nika" "$TMP/work/$NAME"
room nika check --color never "$NAME" >"$RAW/cost-ceiling-check.txt" 2>&1
has "$RAW/cost-ceiling-check.txt" "worst-case output ceiling"
has "$RAW/cost-ceiling-check.txt" "≤4096 tk"
# The same report as data: the envelope, the price it used and the
# catalog snapshot it came from (the rest of the report stays out).
room nika check --json "$NAME" >"$TMP/check.json"
node - "$TMP/check.json" "$RAW/cost-ceiling-check.json" <<'NODE'
const fs = require('fs');
const [src, dst] = process.argv.slice(2);
const r = JSON.parse(fs.readFileSync(src, 'utf8'));
const keep = { engine_version: r.engine_version, build_sha: r.build_sha, cost: r.cost, pricing: r.pricing };
if (!keep.cost || keep.cost.has_unbounded || !(keep.cost.bounded_total_usd > 0)) {
  console.error('the capped workflow no longer prices a bounded ceiling');
  process.exit(1);
}
fs.writeFileSync(dst, `${JSON.stringify(keep, null, 2)}\n`);
NODE

# ── 3 · the budget gate: a cap below the floor refuses to start ─────────
set +e
room nika run --no-progress --color never "$NAME" --max-cost-usd "$BUDGET" \
  >"$RAW/cost-ceiling-run.txt" 2>&1
code=$?
set -e
[ "$code" -eq 2 ] || fail "nika run --max-cost-usd $BUDGET exited $code, not 2 (the refusal)"
grep -q '^NIKA-1709 · refusing to start: ' "$RAW/cost-ceiling-run.txt" ||
  fail "the run did not refuse on its budget (NIKA-1709)"
if grep -q 'NIKA-1800' "$RAW/cost-ceiling-run.txt"; then
  fail "the run reached the access check: the budget no longer refuses first"
fi
# Every run writes a flight-recorder journal by default; a refusal before
# the start writes none.
[ ! -e "$TMP/work/.nika" ] || fail "the refused run left .nika/ behind: it started"
# What the run was given, what it answered, and the room it ran in: the
# names of every variable it could read (so: no key).
# shellcheck disable=SC2016  # a JavaScript template literal; bash must not expand it
room env | cut -d= -f1 | sort | node -e '
const names = require("fs").readFileSync(0, "utf8").trim().split("\n");
const [cmd, exit] = process.argv.slice(1);
process.stdout.write(`${JSON.stringify({ command: cmd, exit: Number(exit), env: names, journal_written: false }, null, 2)}\n`);
' "nika run $NAME --max-cost-usd $BUDGET" "$code" >"$RAW/cost-ceiling-run.json"

# ── 4 · the same numbers in beginner words, read after the refusal ──────
room nika explain --color never "$NAME" >"$RAW/cost-ceiling-explain.txt" 2>&1
has "$RAW/cost-ceiling-explain.txt" "cost before a token is spent"
has "$RAW/cost-ceiling-explain.txt" "no runs recorded here yet"

written=("$RAW"/cost-ceiling-*)
echo "cost-ceiling captured: ${#written[@]} files → $RAW/cost-ceiling-*"
