#!/usr/bin/env bash
# approval-gate.sh — capture the durable human gate for the approval-gate
# clip (scripts/media/motion/intent-to-proof/clips/approval-gate.mjs).
#
# gated-ship.nika is the example of the docs' Resume page, byte for byte:
# build, ask a human through `nika:prompt`, ship only on yes. The runs are
# real and offline, each in its own scratch directory under a scratch HOME:
#
#   1. at a terminal (a pseudo-terminal): the gate asks `[y/N]`; answered
#      `y`, then `N`, then a bare Enter
#   2. where no human can answer (stdin is not a terminal, as in CI): the
#      run pauses durably, exits 4 and prints the exact resume line
#   3. that printed line, run exactly as printed: build is reused (cache
#      hit), the answer is given, ship runs
#   4. the trace store and the gate record after the resume
#
# Every run is by relative name, so no path of this machine is printed; the
# script fails if one leaks, if the gate stops asking, if the pause stops
# exiting 4 or if the printed resume line stops finishing the run.
#
# Usage · bash scripts/media/capture/approval-gate.sh   (nika on PATH)
# Output · media/raw/approval-gate-*
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIX="scripts/media/fixtures/gated-ship.nika"
mkdir -p "$RAW"

NIKA="$(command -v nika)" || {
  echo "nika binary not found on PATH" >&2
  exit 1
}
[[ -f "$FIX" ]] || {
  echo "FATAL: fixture missing: $FIX" >&2
  exit 1
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/home"
# A clean environment: a scratch HOME, and a PATH that holds the binary and
# the system tools only. A run of this model-free file still probes the
# harness CLIs it finds on PATH (one writes into HOME); with none on PATH
# the capture stays offline and the rendered output is the same.
SAFE_PATH="$(dirname "$NIKA"):/usr/bin:/bin"
nk() { env -i HOME="$WORK/home" PATH="$SAFE_PATH" "$@"; }
fresh() {
  mkdir -p "$WORK/$1"
  cp "$FIX" "$WORK/$1/gated-ship.nika"
}
fail() {
  echo "FATAL: $*" >&2
  exit 1
}

# ── the file · the audit passes ─────────────────────────────────────────
fresh check
(cd "$WORK/check" && nk nika check --color never gated-ship.nika) \
  >"$RAW/approval-gate-check.txt" 2>&1 || fail "nika check refuses gated-ship.nika"
grep -q 'run ready ✔' "$RAW/approval-gate-check.txt" || fail "gated-ship.nika is not run ready"

# ── 1 · at a terminal: the gate asks ────────────────────────────────────
# A pseudo-terminal driver: it reads with select, waits for the gate's
# `[y/N]` (never a fixed sleep), types the answer and a carriage return,
# then reads to the end. The live render redraws in place, so the bytes
# are settled into the screen a person reads: carriage return, line feed,
# cursor up, erase below and erase line move the cursor; colour and
# hyperlink escapes are not text (a hyperlink's text stays). Any other
# control sequence fails the capture rather than being misread. The one
# stderr line naming the scratch HOME (the home-isolation notice a
# terminal gets) stays out, as the LSP capture keeps its session out.
tty_run() { # <dir> <answer> <out>
  fresh "$1"
  python3 - "$WORK/$1" "$2" "$WORK/home" "$SAFE_PATH" "$WORK" >"$3" <<'PY'
import fcntl, os, pty, re, select, struct, sys, termios, time

cwd, answer, home, path, work = sys.argv[1:6]
env = {'HOME': home, 'PATH': path, 'TERM': 'xterm-256color'}
pid, fd = pty.fork()
if pid == 0:
    fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
    os.chdir(cwd)
    os.execvpe('nika', ['nika', 'run', 'gated-ship.nika'], env)

buf, asked = b'', False
deadline = time.monotonic() + 60
while True:
    if time.monotonic() > deadline:
        os.kill(pid, 9)
        sys.exit('the terminal run did not finish within 60 s')
    ready, _, _ = select.select([fd], [], [], 0.25)
    if fd not in ready:
        continue
    try:
        chunk = os.read(fd, 65536)
    except OSError:  # EIO: the run closed its terminal
        break
    if not chunk:
        break
    buf += chunk
    if not asked and b'[y/N]' in buf:
        os.write(fd, answer.encode() + b'\r')
        asked = True
_, status = os.waitpid(pid, 0)
if not asked:
    sys.exit('the gate never asked [y/N] at the terminal')
code = os.waitstatus_to_exitcode(status)

data = buf.decode('utf-8')
lines, row, col, i = [''], 0, 0, 0
CSI = re.compile(r'\x1b\[([?0-9;]*)([A-Za-z])')
OSC8 = re.compile(r'\x1b\]8;[^;\x1b\x07]*;[^\x1b\x07]*(?:\x1b\\|\x07)')
while i < len(data):
    ch = data[i]
    if ch == '\x1b':
        m = CSI.match(data, i)
        if m:
            p, c = m.groups()
            if c == 'm' or (p == '?2026' and c in 'hl'):
                pass  # colour · synchronized update
            elif c == 'A':
                row = max(0, row - int(p or 1))
            elif c == 'J' and p in ('', '0'):
                lines[row] = lines[row][:col]
                del lines[row + 1:]
            elif c == 'K' and p in ('', '0'):
                lines[row] = lines[row][:col]
            else:
                sys.exit(f'unhandled escape {m.group(0)!r}')
            i = m.end()
            continue
        m = OSC8.match(data, i)
        if m:
            i = m.end()
            continue
        sys.exit(f'unhandled escape {data[i:i + 12]!r}')
    if ch == '\r':
        col = 0
    elif ch == '\n':
        row += 1
        while len(lines) <= row:
            lines.append('')
    elif ch < ' ':
        sys.exit(f'unhandled control character {ch!r}')
    else:
        cur = lines[row].ljust(col)
        lines[row] = cur[:col] + ch + cur[col + 1:]
        col += 1
    i += 1
shown = [l.rstrip() for l in lines if not l.startswith('nika: home isolation:')]
while shown and not shown[-1]:
    shown.pop()
text = '\n'.join(shown)
if work in text or home in text:
    sys.exit('a scratch path reached the terminal transcript')
print(text)
print(f'exit {code}', file=sys.stderr)
PY
}

for a in yes:y no:N enter:; do
  name="${a%%:*}"
  tty_run "tty-$name" "${a#*:}" "$RAW/approval-gate-tty-$name.txt" 2>"$WORK/tty-$name.exit" \
    || fail "the terminal run ($name): $(cat "$WORK/tty-$name.exit")"
  grep -qx 'exit 0' "$WORK/tty-$name.exit" \
    || fail "the terminal run answered '$name' did not exit 0: $(cat "$WORK/tty-$name.exit")"
done
grep -q '◇ approve · Ship this build to production?  \[y/N\] y$' "$RAW/approval-gate-tty-yes.txt" \
  || fail "the terminal no longer asks the gate's question"
grep -q 'resumed · 1 skipped (cache hit) · 2 ran live' "$RAW/approval-gate-tty-yes.txt" \
  || fail "answered y at the terminal, ship no longer runs"
for a in no enter; do
  grep -q 'ship     when: closed (post-gate)' "$RAW/approval-gate-tty-$a.txt" \
    || fail "answered '$a', ship is no longer held back"
done

# ── 2 · nobody to ask: the run pauses durably ───────────────────────────
# stdin is not a terminal and stdout goes to a log, as in CI. The run must
# settle by itself (no hang) with exit 4, printing the resume line.
fresh ci
set +e
(cd "$WORK/ci" && nk timeout 60 nika run gated-ship.nika) \
  </dev/null >"$RAW/approval-gate-ci.txt" 2>"$WORK/ci.stderr"
ci_exit=$?
set -e
[[ "$ci_exit" -eq 4 ]] || fail "unattended, the run exited $ci_exit, not 4 (paused)"
[[ ! -s "$WORK/ci.stderr" ]] || fail "the paused run wrote to stderr: $(cat "$WORK/ci.stderr")"
# shellcheck disable=SC2016  # the backticks are the frame's own, literal
grep -q '◇ paused · awaiting an answer for `approve`' "$RAW/approval-gate-ci.txt" \
  || fail "the paused frame no longer says what it awaits"

# The printed resume line, and the command it carries (before its note).
resume_line="$(grep -m1 '^    resume: ' "$RAW/approval-gate-ci.txt")" \
  || fail "the paused run printed no resume line"
resume_cmd="${resume_line#    resume: }"
resume_cmd="${resume_cmd% · or false}"
[[ "$resume_cmd" =~ ^nika\ run\ gated-ship\.nika\ --resume\ \.nika/traces/[A-Za-z0-9._-]+\.ndjson\ --answer\ approve=true$ ]] \
  || fail "the resume line changed shape: $resume_line"
printf '%s\n' "$resume_cmd" >"$RAW/approval-gate-resume-cmd.txt"
trace="$(awk '{print $5}' <<<"$resume_cmd")"
[[ -f "$WORK/ci/$trace" ]] || fail "the resume line names no trace on disk: $trace"

# What the pause left in that trace: the workflow_paused event, its stable
# fields (its ids, times and digests differ on every run and stay out).
# shellcheck disable=SC2016  # a JavaScript program, not shell expansions
node -e '
const fs = require("fs");
const keep = new Set(["workflow", "task", "mode", "note", "message", "status", "cause"]);
const paused = fs.readFileSync(process.argv[1], "utf8").split("\n").filter(l => l.trim())
  .map(l => JSON.parse(l)).filter(e => e.kind === "workflow_paused");
if (paused.length !== 1) { console.error(`expected one workflow_paused, found ${paused.length}`); process.exit(1); }
const e = paused[0];
console.log(JSON.stringify({ kind: e.kind, fields: e.fields.filter(f => keep.has(f.key)) }));
' "$WORK/ci/$trace" >"$RAW/approval-gate-paused-event.json"

# ── 3 · the printed line, run as printed ────────────────────────────────
read -r -a resume_argv <<<"$resume_cmd"
set +e
(cd "$WORK/ci" && nk timeout 60 "${resume_argv[@]}") \
  </dev/null >"$RAW/approval-gate-resume.txt" 2>"$WORK/resume.stderr"
resume_exit=$?
set -e
[[ "$resume_exit" -eq 0 ]] || fail "the printed resume line exited $resume_exit, not 0"
[[ ! -s "$WORK/resume.stderr" ]] || fail "the resume wrote to stderr: $(cat "$WORK/resume.stderr")"
grep -q '↷  build    cache hit (resume)' "$RAW/approval-gate-resume.txt" || fail "the resume no longer reuses build"
grep -q '✔  ship ' "$RAW/approval-gate-resume.txt" || fail "the resume no longer ships"
grep -q 'resumed · 1 skipped (cache hit) · 2 ran live' "$RAW/approval-gate-resume.txt" \
  || fail "the resume summary changed"

# ── 4 · the record: paused, then succeeded · the gate's decision ────────
(cd "$WORK/ci" && nk nika trace ls --color never) </dev/null >"$RAW/approval-gate-trace-ls.txt" 2>&1
grep -q ' paused$' "$RAW/approval-gate-trace-ls.txt" || fail "the trace store lost the paused run"
grep -q ' succeeded ★$' "$RAW/approval-gate-trace-ls.txt" || fail "the trace store lost the resumed run"
(cd "$WORK/ci" && nk nika trace show --color never) </dev/null >"$RAW/approval-gate-trace-show.txt" 2>&1
grep -q 'answer: "true"' "$RAW/approval-gate-trace-show.txt" || fail "the gate record lost its answer"
# What each task of the resumed run holds (build's is the reused one).
(cd "$WORK/ci" && nk nika trace outputs --color never) </dev/null >"$RAW/approval-gate-resume-outputs.txt" 2>&1
grep -q '^  ship .*"shipped"' "$RAW/approval-gate-resume-outputs.txt" || fail "the resumed run did not ship"

printf '{"check":0,"tty_yes":0,"tty_no":0,"tty_enter":0,"ci":%d,"resume":%d}\n' "$ci_exit" "$resume_exit" \
  >"$RAW/approval-gate-exits.json"

for f in "$RAW"/approval-gate-*; do
  if grep -qF -e "$WORK" -e "$ROOT" "$f"; then fail "a machine path reached $f"; fi
done
captured=("$RAW"/approval-gate-*)
echo "approval-gate captured: ${#captured[@]} files in $RAW"
