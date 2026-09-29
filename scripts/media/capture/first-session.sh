#!/usr/bin/env bash
# first-session.sh — capture what the first-session clip shows: the
# Session's plain view (`nika --plain`) taking the README's orders request
# from a sentence to a checked file, a run and its proof.
#
# The story, typed into one Session in a scratch `orders-demo/` folder:
#   1. the README's request   → a proposed `compiled-workflow.nika`, checked
#   2. `yes`                  → the exact bytes saved and checked, nothing run
#   3. `run it`               → one run under an announced ceiling
#   4. `/proof`               → what the run's journal records, and its limits
#   5. `/quit`
# The deterministic compiler settles this request: no AI model is chosen,
# called or needed, and the capture refuses if the Session asks for one.
#
# The Session opens only on a terminal, so a small python3 driver gives it a
# pseudo-terminal for stdin and stdout, types each line and waits for the
# next prompt (`nika ›` or `apply? ›`) before typing the next. Its stderr
# goes to a file: the Session writes nothing there, and bare `nika` prints
# its HOME-isolation note only when stderr is a terminal (that note would
# carry this machine's scratch path into the transcript).
#
# Offline and idempotent: a scratch directory, a scratch HOME and a clean
# environment (`env -i`). The scratch HOME gets its own run-signing key
# first (`nika key init`), so the run is sealed and `/proof` shows the
# signature it verifies; no key or config of this machine is used, and
# NIKA_KEYCHAIN=off keeps an OS keychain out of it. Run durations, the
# chain head, the trace name and the key fingerprint differ on every
# capture; the clip reads them from here and never types them.
#
# Usage · bash scripts/media/capture/first-session.sh   (nika on PATH)
# Output · media/raw/first-session-{transcript.txt,workflow.nika,orders.csv,
#          paid.csv,facts.json}
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RAW="$ROOT/media/raw"
mkdir -p "$RAW"

command -v nika >/dev/null || {
  echo "nika binary not found on PATH" >&2
  exit 1
}
command -v python3 >/dev/null || {
  echo "python3 not found on PATH (it drives the Session's terminal)" >&2
  exit 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/home" "$TMP/orders-demo/data"
# the README's first workflow, byte for byte (README "Start here", step 2)
cat >"$TMP/orders-demo/data/orders.csv" <<'EOF'
order_id,customer,status,amount
1001,Juniper Books,paid,18.00
1002,Kestrel Cafe,pending,9.50
1003,Linden Studio,paid,13.50
1004,Moss Supply,refunded,6.00
EOF
cd "$TMP/orders-demo"

# The clean environment every nika call below runs in.
clean() {
  env -i HOME="$TMP/home" PATH="$PATH" NIKA_KEYCHAIN=off TERM=dumb NO_COLOR=1 "$@"
}

clean nika key init >"$TMP/key-init.txt" 2>&1 || {
  cat "$TMP/key-init.txt" >&2
  echo "FATAL: nika key init failed in the scratch HOME" >&2
  exit 1
}

# ── the Session, driven through a pseudo-terminal ───────────────────────
clean COLUMNS=100 LINES=50 python3 - "$TMP/session.txt" "$TMP/session-stderr.txt" <<'PY'
import fcntl
import os
import pty
import re
import select
import struct
import sys
import termios
import time

transcript, errfile = sys.argv[1:]
TYPED = [
    "Read ./data/orders.csv, keep only the rows whose status is paid, "
    "and write them to ./out/paid.csv.",
    "yes",
    "run it",
    "/proof",
]
PROMPT = re.compile(r"(nika ›|apply\? ›) ?$")

pid, fd = pty.fork()
if pid == 0:
    err = os.open(errfile, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    os.dup2(err, 2)
    os.execvp("nika", ["nika", "--plain"])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 100, 0, 0))
out = bytearray()


def pump(timeout):
    """Read what the Session printed: bytes read, 0 on a quiet wait, -1 at its end."""
    ready, _, _ = select.select([fd], [], [], timeout)
    if fd not in ready:
        return 0
    try:
        chunk = os.read(fd, 65536)
    except OSError:
        return -1
    if not chunk:
        return -1
    out.extend(chunk)
    return len(chunk)


def since(start):
    return out[start:].decode("utf-8", "replace").replace("\r", "")


def until_prompt(what, timeout=180):
    """Wait until the Session prints its prompt last and then stays quiet."""
    start, end = len(out), time.time() + timeout
    while time.time() < end:
        if pump(0.2) < 0:
            break
        if PROMPT.search(since(start)):
            quiet = time.time()
            while time.time() - quiet < 0.6:
                if pump(0.1) > 0:
                    quiet = time.time()
            if PROMPT.search(since(start)):
                return
    sys.stdout.write(out.decode("utf-8", "replace"))
    sys.exit(f"FATAL: no prompt after {what}")


until_prompt("opening the Session")
for line in TYPED:
    os.write(fd, (line + "\r").encode())
    until_prompt(repr(line))
os.write(fd, b"/quit\r")
end = time.time() + 30
while time.time() < end and pump(0.2) >= 0:
    pass
_, status = os.waitpid(pid, 0)
code = os.waitstatus_to_exitcode(status)
with open(transcript, "w", encoding="utf-8") as f:
    f.write(out.decode("utf-8").replace("\r\n", "\n"))
if code != 0:
    sys.exit(f"FATAL: the Session exited {code}")
PY

if [[ -s "$TMP/session-stderr.txt" ]]; then
  echo "note: the Session wrote to stderr (kept out of the transcript):" >&2
  cat "$TMP/session-stderr.txt" >&2
fi

# ── the story must still hold ───────────────────────────────────────────
S="$TMP/session.txt"
need() {
  grep -qF -- "$1" "$S" || {
    cat "$S" >&2
    echo "FATAL: the Session no longer prints: $1" >&2
    exit 1
  }
}
# the backticks are the Session's own code marks, matched literally
# shellcheck disable=SC2016
{
  need 'Nika proposes `compiled-workflow.nika`:'
  need 'check of these bytes · `compiled-workflow.nika` · clean ✔'
  need 'Nothing has run yet · `yes` saves these exact bytes and checks them'
  need 'applied · wrote `compiled-workflow.nika`'
  need 'Saved · checked · not active · nothing has run'
  need 'running `compiled-workflow.nika` once · ceiling $'
  need 'produced · ./out/paid.csv ('
  need 'cost · no model usage recorded'
  need 'run observed · exit 0 · succeeded'
  need 'chain · OK — '
  need 'does not prove · that the content is right (read it)'
}
if grep -qF 'Choose which AI' "$S"; then
  echo "FATAL: the Session asked which AI to use; this story needs none" >&2
  exit 1
fi

# out/paid.csv is exactly the header and the rows whose status is paid
awk -F, 'NR == 1 || $3 == "paid"' data/orders.csv >"$TMP/expected.csv"
[[ "$(grep -c ',paid,' "$TMP/expected.csv")" -eq 2 ]] || {
  echo "FATAL: the orders fixture no longer has two paid rows" >&2
  exit 1
}
cmp -s out/paid.csv "$TMP/expected.csv" || {
  diff "$TMP/expected.csv" out/paid.csv >&2 || true
  echo "FATAL: out/paid.csv is not the header and the two paid rows" >&2
  exit 1
}

# ── facts the clip checks the transcript against ────────────────────────
python3 - "$S" "$TMP/facts.json" "$(clean nika --version)" \
  "$(wc -c <out/paid.csv | tr -d ' ')" <<'PY'
import json
import re
import sys

transcript, facts, version, size = sys.argv[1:]
text = open(transcript, encoding="utf-8").read()
done = re.search(r"^Done · `[^`]+` · \d+ ms · (\d+) tasks ran$", text, re.M)
run = re.search(r"run observed · exit (\d+) · (\w+)", text)
tasks = len(re.findall(r"^  ok \w+ +invoke · nika:\w+ +\d+ms$", text, re.M))
if not done or not run or int(done.group(1)) != tasks:
    sys.exit("FATAL: the run's task lines and its Done line disagree")
record = {
    "nika": version,
    "session_exit": 0,
    "run_exit": int(run.group(1)),
    "run": run.group(2),
    "tasks": tasks,
    "written": {"path": "./out/paid.csv", "bytes": int(size)},
    "sealed": "seal · SEALED — " in text,
    "model_usage_recorded": False,
}
with open(facts, "w", encoding="utf-8") as f:
    f.write(json.dumps(record, indent=2) + "\n")
print(f"first-session: {text.count(chr(10))} lines · {tasks} tasks · "
      f"{size} B written · sealed {record['sealed']}")
PY

# only a capture that told the whole story replaces the committed one
cp "$S" "$RAW/first-session-transcript.txt"
cp compiled-workflow.nika "$RAW/first-session-workflow.nika"
cp data/orders.csv "$RAW/first-session-orders.csv"
cp out/paid.csv "$RAW/first-session-paid.csv"
cp "$TMP/facts.json" "$RAW/first-session-facts.json"
