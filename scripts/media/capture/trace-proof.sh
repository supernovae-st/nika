#!/usr/bin/env bash
# trace-proof.sh — capture what the trace-proof clip shows: a run's
# hash-chained trace, `nika trace verify` reading it back, and the same
# verify on a copy of that trace with one byte changed.
#
# Every line of the journal carries a `chain` field, the sha256 of the line
# before it (the first line's, of a fixed genesis tag), so an edit to a line
# breaks the link the next line carries. The copy changes one
# byte of the `task_completed` line, the first numeral of its `tokens`
# value, and nothing else: `cmp -l` proves it and the byte is recorded.
#
# Offline, idempotent, and run in a scratch directory with a scratch HOME:
# no signing key or user config of this machine reaches the run, so it is
# unsealed wherever it is captured (the verify output says so), and no path
# of this machine reaches a transcript (everything runs by relative name).
#
# Usage · bash scripts/media/capture/trace-proof.sh   (nika on PATH)
# Output · media/raw/trace-proof-*
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RAW="$ROOT/media/raw"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/home" "$TMP/work"
export HOME="$TMP/home"
cd "$TMP/work"

# ── the run and the trace it writes ─────────────────────────────────────
# The offline `hello` skeleton, run on mock/echo: a rehearsal, and the run
# output says so on its first line.
nika compile --color never hello hello.nika >/dev/null
run_exit=0
nika run --no-progress --color never hello.nika >"$RAW/trace-proof-run.txt" 2>&1 || run_exit=$?
shopt -s nullglob
traces=(.nika/traces/*.ndjson)
shopt -u nullglob
[[ ${#traces[@]} -eq 1 ]] || {
  echo "FATAL: expected one trace, found ${#traces[@]}" >&2
  exit 1
}
TRACE="${traces[0]}"
cat "$TRACE" >"$RAW/trace-proof-trace.ndjson"

# ── verify the trace the run wrote (the workspace latest) ───────────────
verify_exit=0
nika trace verify --color never >"$RAW/trace-proof-verify.txt" 2>&1 || verify_exit=$?
grep -q '^OK — .* · chain intact · head [0-9a-f]\{64\}$' "$RAW/trace-proof-verify.txt" || {
  echo "FATAL: nika trace verify did not report the chain intact" >&2
  exit 1
}

# ── one byte changed, in a copy ─────────────────────────────────────────
# The first numeral of the task_completed line's `tokens` value becomes 9
# (or 1, were it already 9): one byte, found by its field, not by a fixed
# offset, and recorded with its line, column and file offset.
cp "$TRACE" tampered.ndjson
node - tampered.ndjson "$RAW/trace-proof-tamper.json" <<'NODE'
const fs = require('fs');
const [file, record] = process.argv.slice(2);
const buf = fs.readFileSync(file);
let start = 0, line = 0, found = null;
while (start < buf.length) {
  let end = buf.indexOf(0x0a, start);
  if (end < 0) end = buf.length;
  line++;
  const text = buf.subarray(start, end).toString('utf8');
  if (text.trim() && JSON.parse(text).kind === 'task_completed') { found = { start, end, line }; break; }
  start = end + 1;
}
if (!found) throw new Error('no task_completed line in the trace');
const key = Buffer.from('{"key":"tokens","value":');
const at = buf.indexOf(key, found.start);
if (at < 0 || at >= found.end) throw new Error('the task_completed line carries no tokens value');
const offset = at + key.length;
const from = String.fromCharCode(buf[offset]);
if (!/[0-9]/.test(from)) throw new Error(`the tokens value does not start with a numeral: ${from}`);
const to = from === '9' ? '1' : '9';
const number = buf.subarray(offset, found.end).toString('latin1').match(/^\d+/)[0];
buf[offset] = to.charCodeAt(0);
fs.writeFileSync(file, buf);
fs.writeFileSync(record, `${JSON.stringify({
  file: 'tampered.ndjson',
  line: found.line,
  kind: 'task_completed',
  key: 'tokens',
  byte: offset + 1,
  column: offset - found.start + 1,
  from,
  to,
  value_from: number,
  value_to: to + number.slice(1),
}, null, 2)}\n`);
NODE
cat tampered.ndjson >"$RAW/trace-proof-tampered.ndjson"

# The copy differs from the trace in exactly one byte (cmp exits 1 when
# files differ; its -l listing is byte number, then both bytes in octal).
cmp -l "$TRACE" tampered.ndjson >"$RAW/trace-proof-cmp.txt" || true
[[ $(wc -l <"$RAW/trace-proof-cmp.txt") -eq 1 ]] || {
  echo "FATAL: the copy differs in more than one byte" >&2
  exit 1
}

# ── verify the copy ─────────────────────────────────────────────────────
tampered_exit=0
nika trace verify --color never tampered.ndjson >"$RAW/trace-proof-verify-tampered.txt" 2>&1 || tampered_exit=$?
grep -q '^BROKEN at line [0-9]* — recorded chain [0-9a-f]\{16\} · computed [0-9a-f]\{16\}$' \
  "$RAW/trace-proof-verify-tampered.txt" || {
  echo "FATAL: nika trace verify did not refuse the tampered copy" >&2
  exit 1
}

printf '{\n  "run": %d,\n  "verify": %d,\n  "verify_tampered": %d\n}\n' \
  "$run_exit" "$verify_exit" "$tampered_exit" >"$RAW/trace-proof-exits.json"
[[ $run_exit -eq 0 && $verify_exit -eq 0 && $tampered_exit -eq 2 ]] || {
  echo "FATAL: exit codes run $run_exit · verify $verify_exit · tampered $tampered_exit (want 0 · 0 · 2)" >&2
  exit 1
}

echo "trace-proof captured: $(sed -n 1p "$RAW/trace-proof-verify-tampered.txt")"
