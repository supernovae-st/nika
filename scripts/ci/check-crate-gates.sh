#!/usr/bin/env bash
# check-crate-gates.sh — REPORT. Emits 12-gate status JSON for a crate.
#
# THIS IS NOT A GATE. It has no failing exit path: the only non-zero exits
# are the two argument/precondition guards below, and a gate reported as
# "fail" still leaves the process at 0. That is deliberate — it is a
# read-out for a human running `/admit` or CONTRIBUTING.md's checklist,
# and it has no automated caller by design.
#
# Saying so here because a 2026-08 audit counted it among the repo's
# guards and read "emits four fails, exits 0" as a broken gate. It is not
# broken; it was never a gate. The gates that actually block live
# elsewhere and each one owns its own exit code:
#
#   Gate 5 (mutation floor)  scripts/ci/check-mutation-floor.sh
#   layer discipline         scripts/ci/check-layering.sh
#   the nine CI ratchets     scripts/hooks/run-ci-ratchets.sh
#   the hygiene board        scripts/hygiene/check-all.sh
#
# If you want a blocking version of something below, wire that check into
# one of those — do not add an exit code here, because every consumer of
# this script reads its stdout as JSON.
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
# g12 asks whether this manifest ever arrived on an "admit to workspace"
# commit. It read `-n 1` — the LAST subject to touch the file — which any
# later commit overwrites. `chore(release): v0.108.0` rewrote 41 of the 63
# manifests, so the answer collapsed to 1 pass / 62 skip: the release train
# erased the admission record it was reading. Searching every commit that
# touched the manifest is the question actually being asked, and answers
# 18 / 63. Still a weak signal — most crates were admitted under other
# subjects — but it is no longer destroyed by unrelated commits.
g12=$(git log --format=%s -- "$manifest" 2>/dev/null | grep -q "admit to workspace" && echo pass || echo skip)

printf '{"crate":"%s","gates":{"1":"%s","2":"%s","3":"%s","4":"%s","5":"%s","6":"%s","7":"%s","8":"%s","9":"%s","10":"%s","11":"%s","12":"%s"},"ran_at":"%s"}\n' \
  "$crate" "$g1" "$g2" "$g3" "$g4" "$g5" "$g6" "$g7" "$g8" "$g9" "$g10" "$g11" "$g12" "$ran_at"
