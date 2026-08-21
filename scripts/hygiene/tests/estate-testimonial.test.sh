#!/usr/bin/env bash
# COVERS: scripts/estate_rules.py
# The testimonial lane must win before the broad authored-docs rule, while
# ordinary documentation must remain authored.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="${SPN_REPO_ROOT:-$(cd -- "$SCRIPT_DIR/../../.." && pwd)}"

python3 - "$REPO_ROOT" <<'PY'
import importlib.util
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("estate", root / "scripts/estate.py")
estate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(estate)

def classification(path):
    for rule in estate.PATTERNS:
        if estate.glob_to_re(rule["glob"]).match(path):
            return rule["class"]
    raise AssertionError(f"unclassified test path: {path}")

assert classification("docs/testimonials/run/outcomes.json") == "testimonial"
assert classification("docs/crate-specs/nika-cadence.md") == "authored"
print("OK: testimonial evidence is first-match classified without widening docs")
PY
