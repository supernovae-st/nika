#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-all.sh
# Run the real dashboard with one synthetic vector; other absent vectors stay
# yellow. A leading success line must not hide the child that actually failed.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/scripts/hygiene"
cp "$ROOT/scripts/hygiene/check-all.sh" "$WORK/scripts/hygiene/check-all.sh"
cat >"$WORK/scripts/hygiene/check-memory-head.sh" <<'CHILD'
#!/usr/bin/env bash
printf '%s\n' 'first child passed' 'FAIL child | "quoted"' 'last \ path'
exit "${FIXTURE_EXIT:-2}"
CHILD
chmod +x "$WORK/scripts/hygiene/check-memory-head.sh"

status=0
bash "$WORK/scripts/hygiene/check-all.sh" --quiet >"$WORK/table" || status=$?
[ "$status" -eq 2 ]
grep -qF 'FAIL child | "quoted"' "$WORK/table"
grep -qF 'last \ path' "$WORK/table"
status=0
bash "$WORK/scripts/hygiene/check-all.sh" --format=json >"$WORK/red.json" || status=$?
[ "$status" -eq 2 ]
status=0
FIXTURE_EXIT=0 bash "$WORK/scripts/hygiene/check-all.sh" --format=json >"$WORK/green.json" || status=$?
[ "$status" -eq 1 ] # Missing sibling vectors are still yellow.
python3 - "$WORK/red.json" "$WORK/green.json" <<'PY'
import json
import sys

for path, status in zip(sys.argv[1:], ["red", "green"]):
    with open(path, encoding="utf-8") as source:
        rows = json.load(source)
    row = next(row for row in rows if row["vector"].startswith("1  memory-head-sha"))
    assert row["status"] == status, row
    assert row["detail"] == 'first child passed\nFAIL child | "quoted"\nlast \\ path', row
PY
printf 'OK: multiline diagnostics survive quiet tables and valid JSON without changing verdicts\n'
