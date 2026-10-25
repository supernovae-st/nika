#!/usr/bin/env bash
# COVERS: scripts/adr/generate-index.sh
# Mutation proof for the ADR index projection: every relation edge the
# frontmatter carries reaches docs/adr/index.json.
#
# docs/adr/README.md says the index carries the full relation graph
# (supersedes · requires · enables · amends), and the generator emitted
# every edge but `amends` (NIK-262): ADR-154 amends three ADRs, and no
# reader of the index could tell. The case below runs the generator over
# two seeded ADRs and asserts the edge, its empty form, and that the
# other edges keep their values. A generator that drops `amends` again
# turns it red.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
GENERATOR="$ROOT/scripts/adr/generate-index.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/scripts/adr" "$WORK/docs/adr"
cp "$GENERATOR" "$WORK/scripts/adr/generate-index.sh"

# seed_adr <file> <id> <amends> <enables>
seed_adr() {
  printf -- '---\nid: %s\ntitle: "%s"\nstatus: accepted\ndate: 2026-10-25\ntags: []\naffects_crates: []\naffects_layers: []\nsupersedes: []\nsuperseded_by: []\nrelated: []\nrequires: []\nenables: %s\namends: %s\n---\n\n# %s\n' \
    "$2" "$2" "$4" "$3" "$2" >"$WORK/docs/adr/$1"
}
seed_adr adr-001-base.md ADR-001 '[]' '["ADR-002"]'
seed_adr adr-002-amender.md ADR-002 '["ADR-001"]' '[]'

if ! bash "$WORK/scripts/adr/generate-index.sh" >/dev/null 2>"$WORK/err"; then
  echo "FAIL: the generator did not run: $(cat "$WORK/err")"
  exit 1
fi

python3 - "$WORK/docs/adr/index.json" <<'PY'
import json
import sys

index = json.load(open(sys.argv[1], encoding="utf-8"))
by_id = {entry["id"]: entry for entry in index["adrs"]}
failures = []
if by_id.get("ADR-002", {}).get("amends") != ["ADR-001"]:
    failures.append(f"ADR-002 amends: want ['ADR-001'], got {by_id.get('ADR-002')}")
if by_id.get("ADR-001", {}).get("amends") != []:
    failures.append(f"ADR-001 amends: want [], got {by_id.get('ADR-001')}")
if by_id.get("ADR-001", {}).get("enables") != ["ADR-002"]:
    failures.append(f"ADR-001 enables lost its value: {by_id.get('ADR-001')}")
for failure in failures:
    print(f"FAIL: {failure}")
sys.exit(1 if failures else 0)
PY
status=$?
[ "$status" -eq 0 ] || exit 1
echo "ok: the ADR index carries amends (and keeps enables)"
