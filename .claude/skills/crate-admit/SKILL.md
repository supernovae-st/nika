---
name: crate-admit
description: Guided 12-gate crate admission workflow. Runs each gate sequentially and stops on first failure. Use when you believe a crate is ready to join the workspace.
argument-hint: [crate-name]
allowed-tools: Bash, Read, Edit, Grep
---

# Crate admission — `$ARGUMENTS`

Authority: `.claude/rules/diamond-discipline.md` §12 gates, `RUST_ENFORCEMENT.md`.
**No exception.** If a gate genuinely does not apply, document the exemption in
`docs/crate-specs/$ARGUMENTS.md` with a 1-paragraph rationale BEFORE running
this skill.

## Pre-flight (run first)

```bash
# Grep-verify current state
git log -1 --oneline
ls crates/$ARGUMENTS/Cargo.toml 2>/dev/null || echo "NOT SCAFFOLDED"
test -f docs/crate-specs/$ARGUMENTS.md || echo "SPEC MISSING — create it first"
```

If pre-flight fails → STOP. Scaffold + spec first.

## The 12 gates, sequential

Run `/gate-check $ARGUMENTS` first to see current state. Then for any FAIL:

### Gate 1 — SPEC
File `docs/crate-specs/$ARGUMENTS.md` exists with: Purpose, Layer, LOC budget,
Public API, Dependencies, Exemptions (if any).

### Gate 2 — TDD
Verify test commits precede impl commits via `git log --oneline crates/$ARGUMENTS/`.
Count tests in `crates/$ARGUMENTS/src/` and `crates/$ARGUMENTS/tests/`.

### Gate 3 — IMPL
`cargo test -p $ARGUMENTS --lib` must be all-green.

### Gate 4 — CLIPPY 0
`cargo clippy -p $ARGUMENTS --all-targets -- -D warnings` must produce 0 warnings.

### Gate 5 — MUTATION ≥ 90 %
`cargo mutants -p $ARGUMENTS --timeout 30`. Record the score in the crate spec.

### Gate 6 — PROPERTY
If the crate parses input, handles encoding, or touches security boundaries,
it MUST have proptest tests. Otherwise mark N/A in the spec.

### Gate 7 — BENCHMARKS
If it's on a hot path (per `DIAMOND.md` performance section), add `benches/`
with criterion. Otherwise N/A.

### Gate 8 — DOCS
`cargo doc --no-deps -p $ARGUMENTS` must emit 0 warnings. Every `pub` item
needs a doc comment starting with `///`.

### Gate 9 — CANARY E2E
Add `tests/canary-$ARGUMENTS.nika.yaml` that exercises the crate end-to-end.
If the crate is infra-only (kernel traits, mocks), mark N/A.

### Gate 10 — PARITY LEGACY
Create a golden test comparing diamond output to `git show main:...` output
for the same input. Diff must be empty or annotated.

### Gate 11 — REVIEW SWARM (parallel)
Launch `.claude/agents/review-swarm.md` with `$ARGUMENTS`. Address all P0/P1
findings in the SAME session before proceeding.

### Gate 12 — ATOMIC COMMIT
When all 11 prior gates PASS, stage ONLY the files for this crate:

```bash
git add crates/$ARGUMENTS/ docs/crate-specs/$ARGUMENTS.md
# NEVER `git add -A` or `git add .`
```

Commit message (exact form):
```
feat($ARGUMENTS): admit to workspace — all 12 gates passed

Layer: L{0,0.5,1,2,3,4,5}
LOC: N
Tests: N
Mutation killed: N%

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## After commit — mandatory

1. Do NOT push yet — user GO required.
2. Run `./scripts/hygiene/check-all.sh` — all green expected.
3. Update `MEMORY.md` Quick State: HEAD, crate count, LOC, tests.
4. Report to user: commit SHA, metrics, any exemptions taken.
