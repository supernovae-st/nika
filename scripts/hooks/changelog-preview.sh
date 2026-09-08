#!/usr/bin/env bash
# changelog-preview.sh — Tier 3 post-commit (optional, bounded)
#
# Print-only preview using cliff.toml in the current working directory.
# Requires git-cliff and Python 3 on POSIX; missing tools/config skip silently.
# Any error still exits 0. No release command or file is changed.
#
# git describe + git-cliff + head share a 3-second wall-clock budget,
# followed by at most 0.5 seconds to reap the worker. Process startup and OS
# scheduling are additional; this is a hang guard, not a real-time guarantee.
# Children remaining in the worker's process group are killed on completion,
# timeout or a caught interruption. Deliberately detached sessions and SIGKILL
# of this supervisor are outside that cleanup guarantee.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

if ! command -v git-cliff >/dev/null 2>&1 \
  || ! command -v python3 >/dev/null 2>&1 \
  || [[ ! -f 'cliff.toml' ]]; then
  exit 0
fi

# Python is already used by the hook suite. Do not fall back to an unbounded
# pipeline when GNU timeout/gtimeout is unavailable (notably on macOS).
python3 - "$BASH" <<'PY'
import os
import select
import signal
import subprocess
import sys
import time

if os.name != "posix":
    sys.exit(0)

# Inherited SIGCHLD=SIG_IGN would auto-reap the leader and defeat PID pinning.
signal.signal(signal.SIGCHLD, signal.SIG_DFL)
cancelled = False
timed_out = False


def cancel(_signum, _frame):
    global cancelled
    cancelled = True


for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
    signal.signal(sig, cancel)

worker = r'''
set -uo pipefail
LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || echo 'v0.80.0')"
printf '\n[changelog-preview] unreleased since %s:\n' "$LAST_TAG" >&2
git-cliff --unreleased --tag "$LAST_TAG" --strip all 2>/dev/null | head -30 >&2 || true
'''
process = None
deadline = time.monotonic() + 3.0
try:
    process = subprocess.Popen(
        [sys.argv[1], "-c", worker], stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, start_new_session=True,
    )
    # Preview output stays on stderr. This otherwise-unused stdout pipe is
    # the completion channel; an inherited open fd can only consume the same
    # deadline. Never poll/wait/communicate before killpg: even an exited
    # leader must remain unreaped so its PID/group ID cannot be recycled.
    while not cancelled:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            timed_out = True
            break
        readable, _, _ = select.select([process.stdout], [], [], min(remaining, 0.05))
        if readable and not os.read(process.stdout.fileno(), 8192):
            break
except (OSError, ValueError):
    pass  # Optional preview: spawn/read failures do not reject the commit.
finally:
    if process is not None:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except OSError:
            # ESRCH if empty; Darwin may give EPERM for a zombie-only group.
            # An OS refusal is not acknowledged cleanup; never retry by name
            # or broaden the target to another process group.
            pass
        process.stdout.close()
        try:
            process.wait(timeout=0.5)
        except subprocess.TimeoutExpired:
            pass  # Do not turn an optional preview into an unbounded reap.
    if cancelled:
        print("[changelog-preview] interrupted; preview stopped", file=sys.stderr)
    elif timed_out:
        print("[changelog-preview] 3s budget reached; preview stopped", file=sys.stderr)
PY

exit 0
