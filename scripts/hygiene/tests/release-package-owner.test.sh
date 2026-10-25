#!/usr/bin/env bash
# COVERS: .github/workflows/release.yml
# Behaviour proof for the release packaging step: the tarball it builds
# records root ownership (0:0, by number), whoever runs the build.
#
# The v0.123.1 archives recorded the runner's own uid (1001 runner:runner
# on the Linux legs · 501 runner:staff on macOS). `tar -x` run as root in a
# rootless container tries to restore that uid there, where it is not
# mapped, and the extraction fails. This test lifts the exact `run:` script
# of the "Package as `nika`" step out of release.yml, plays it against a
# stand-in binary with the tar this machine has (bsdtar on a Mac, GNU tar
# on Linux), and reads every member's owner. A negative control proves the
# reading has teeth: a plain `tar -czf` of the same stage records this
# user's own uid.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
WORKFLOW="$ROOT/.github/workflows/release.yml"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The step's own script, dedented out of its `run: |` block.
python3 - "$WORKFLOW" "$WORK/package.sh" <<'PY'
import sys

lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
start = next(i for i, line in enumerate(lines) if line.strip() == "- name: Package as `nika`")
run = next(i for i in range(start + 1, len(lines)) if lines[i].strip() == "run: |")
key_indent = len(lines[run]) - len(lines[run].lstrip())
body = []
for line in lines[run + 1:]:
    if line.strip() and len(line) - len(line.lstrip()) <= key_indent:
        break
    body.append(line)
indent = min(len(line) - len(line.lstrip()) for line in body if line.strip())
open(sys.argv[2], "w", encoding="utf-8").write("\n".join(line[indent:] for line in body) + "\n")
PY

# A stand-in `nika`: the step copies it and asks it for three completions.
mkdir -p "$WORK/build/target/x86_64-unknown-linux-gnu/release"
# shellcheck disable=SC2016 # the stand-in reads ITS OWN "$2", unexpanded here
printf '#!/bin/sh\nprintf "complete %%s\\n" "$2"\n' \
  >"$WORK/build/target/x86_64-unknown-linux-gnu/release/nika"
chmod +x "$WORK/build/target/x86_64-unknown-linux-gnu/release/nika"

if ! (cd "$WORK/build" && TMPDIR="$WORK" ASSET_NAME=probe TARGET=x86_64-unknown-linux-gnu \
  REF_NAME=v9.9.9 bash "$WORK/package.sh") >"$WORK/out" 2>&1; then
  echo "FAIL: the packaging step did not run: $(cat "$WORK/out")"
  exit 1
fi

# The negative control: the same stage, packed the way v0.123.1 was.
mkdir -p "$WORK/control"
cp "$WORK/build/target/x86_64-unknown-linux-gnu/release/nika" "$WORK/control/nika"
tar -czf "$WORK/control.tar.gz" -C "$WORK/control" nika

python3 - "$WORK/build/nika-probe-9.9.9.tar.gz" "$WORK/control.tar.gz" "$(id -u)" <<'PY'
import sys
import tarfile

archive, control, uid = sys.argv[1], sys.argv[2], int(sys.argv[3])
failures = []
with tarfile.open(archive) as packed:
    members = packed.getmembers()
    names = sorted(member.name for member in members)
    for want in ("nika", "completions/nika.bash", "completions/_nika", "completions/nika.fish"):
        if want not in names:
            failures.append(f"{want} missing from the archive: {names}")
    for member in members:
        if (member.uid, member.gid) != (0, 0) or member.uname not in ("", "root"):
            failures.append(
                f"{member.name}: {member.uid}:{member.gid} {member.uname!r}:{member.gname!r}"
            )
if uid != 0:
    with tarfile.open(control) as plain:
        if all(member.uid != uid for member in plain.getmembers()):
            failures.append("control: a plain tar did not record this user's uid")
for failure in failures:
    print(f"FAIL: {failure}")
sys.exit(1 if failures else 0)
PY
status=$?
[ "$status" -eq 0 ] || exit 1
echo "ok: the release tarball records 0:0 by number"
