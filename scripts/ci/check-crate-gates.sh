#!/usr/bin/env bash
# check-crate-gates.sh — emit 12-gate status JSON for a crate.
#
# Usage: scripts/ci/check-crate-gates.sh <crate-name>
#
# Output: JSON on stdout matching olympus `CrateGatesSchema`
# (see olympus/src/schemas/workspace.schema.ts):
#   { "crate": "<name>", "gates": { "1": "pass", ... , "12": "skip" },
#     "ran_at": "<iso8601>" }
#
# Gate status enum: "pass" | "fail" | "warn" | "skip"
# (maps to internal notions pass/fail/na/wip used in the Mintlify write-up).
#
# SCHEMA-REF: olympus CrateGatesSchema (workspace.schema.ts lines 100-104).
set -euo pipefail

if [ "$#" -ne 1 ]; then
  printf 'usage: %s <crate-name>\n' "$0" >&2
  exit 2
fi

crate="$1"
spec="docs/crate-specs/${crate}.md"
manifest="crates/${crate}/Cargo.toml"
src="crates/${crate}/src"
tests="crates/${crate}/tests"
benches="crates/${crate}/benches"
canary="tests/canary/${crate}.nika.yaml"
ran_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Hygiene: the crate directory must exist before we claim anything.
if [ ! -f "$manifest" ]; then
  printf 'error: no manifest at %s\n' "$manifest" >&2
  exit 2
fi

# Per-gate status (file-presence heuristics — full execution belongs to CI).
g1=$([ -f "$spec" ] && echo pass || echo fail)
g2=$([ -d "$tests" ] || grep -q '#\[cfg(test)\]' "$src"/*.rs 2>/dev/null && echo pass || echo warn)
g3=$(cargo check -p "$crate" --quiet 2>/dev/null && echo pass || echo fail)
g4=$(cargo clippy -p "$crate" --all-targets --quiet -- -D warnings 2>/dev/null && echo pass || echo fail)
# g5: cheap presence heuristic. The REAL executable Gate 5 (mutation ≥90%
# kill-floor) is scripts/ci/check-mutation-floor.sh <crate>, run at admission
# / in CI (cargo-mutants is minutes-slow — not for this fast JSON emitter).
g5=$([ -f "$spec" ] && grep -q 'Mutation' "$spec" 2>/dev/null && echo warn || echo skip)
g6=$(grep -rqE '\bproptest\!|use proptest' "$src" 2>/dev/null && echo pass || echo skip)
g7=$([ -d "$benches" ] && echo pass || echo skip)
g8=$(cargo doc -p "$crate" --no-deps --quiet 2>/dev/null && echo pass || echo fail)
g9=$([ -f "$canary" ] && echo pass || echo skip)
g10=$([ -f "$spec" ] && grep -q 'Parity' "$spec" 2>/dev/null && echo warn || echo skip)
g11=$([ -f "$spec" ] && grep -q 'Review' "$spec" 2>/dev/null && echo warn || echo skip)
g12=$(git log --format=%s -n 1 -- "$manifest" 2>/dev/null | grep -q "admit to workspace" && echo pass || echo skip)

printf '{"crate":"%s","gates":{"1":"%s","2":"%s","3":"%s","4":"%s","5":"%s","6":"%s","7":"%s","8":"%s","9":"%s","10":"%s","11":"%s","12":"%s"},"ran_at":"%s"}\n' \
  "$crate" "$g1" "$g2" "$g3" "$g4" "$g5" "$g6" "$g7" "$g8" "$g9" "$g10" "$g11" "$g12" "$ran_at"
