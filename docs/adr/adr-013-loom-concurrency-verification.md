---
id: ADR-013
title: "Loom-based concurrency verification for L0.5 and L3"
status: accepted
date: "2026-04-14"
phase: "Phase 3 (runtime admission)"
deciders: ["@ThibautMelen"]
tags: ["testing", "concurrency", "loom", "verification"]
affects_crates: ["nika-kernel-mock"]
affects_layers: ["L0.5", "L3"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-018"]
requires: ["ADR-006"]
enables: []
amends: []
fci: []
inv: ["INV-029"]
shadow_zones: [4]
nika_codes: []
timeline: "v0.80.0, Phase 3 spec -- harness lands with first concurrent primitive"
follow_ups: ["Evaluate Shuttle in Phase 4 for >3-thread state spaces"]
---

# ADR-013: Loom-based concurrency verification for L0.5 + L3

## Context

Diamond has `unsafe_code = "forbid"` workspace-wide — no raw `unsafe` blocks. This catches one class of UB. It does **not** catch concurrency bugs in safe-Rust primitives: a deadlock between two `parking_lot::RwLock`s, a missed wakeup on a `tokio::sync::Notify`, a lost message on an `mpsc::channel` under selective receive.

Tokio itself uses Loom (model-checks all possible interleavings of `Atomic*`, `Mutex`, `mpsc`, etc.) as the **only** reliable way to catch these bugs in its scheduler. Nika-runtime (Phase 3) will have channels, cancellation tokens, and a task graph — the exact territory where Loom has caught real bugs at tokio, crossbeam, parking_lot.

The SOTA audit recommends landing the harness pattern **now** in `nika-kernel-mock` (which already uses tokio channels for test mocks) so that when `nika-runtime` ships, Loom is muscle memory.

## Decision

- `nika-kernel-mock` gains a `tests/loom/` directory with at least one demo test proving the pattern (a 2-thread mock channel drain).
- `[dev-dependencies]` gains `loom = "0.7"` behind a `loom` cfg flag.
- CI workflow `diamond-ci.yml` adds a weekly (not per-PR — too slow) Loom job:
  ```yaml
  loom:
    schedule: cron: '0 4 * * 1'
    run: RUSTFLAGS="--cfg loom" cargo test --lib --test 'loom_*' -p nika-kernel-mock
  ```
- **Every** future crate with concurrent primitives (`nika-runtime` with scheduler, `nika-daemon` with IPC, `nika-mcp` with stdio sessions) MUST ship Loom tests for the primitive at admission. Codified as invariant #29.
- Loom failures block release tags (`git tag v0.X.0` gated on Loom green).

## Consequences

### Positive
- Catches concurrency bugs proptest cannot reach (interleaving enumeration, not property generation).
- Aligns Diamond with tokio-scheduler-grade quality.
- Forces concurrent primitive authors to design for testability (Loom hates over-coupled primitives).

### Negative
- Loom is slow. 2-thread tests take ~10s; 3-thread ~10 minutes. Hence weekly, not per-PR.
- Requires a `--cfg loom` parallel build of the crate — some dep tree duplication.
- Learning curve for Loom semantics (atomic orderings, spuriousness).

### Neutral
- Shuttle (AWS, PCT-based, faster than Loom) is complementary. Revisit in Phase 4 when `nika-runtime` has >3-thread state spaces that exceed Loom's reach.

## Evidence

- tokio's `mpsc` Loom tests: `tokio-stream/tests/`
- crossbeam's `loom_*` modules
- `nika-kernel-mock/src/` — already uses tokio channels for test fixtures (direct beneficiary)

## Alternatives considered

### Alt A — Proptest only
Rejected — proptest cannot enumerate interleavings.

### Alt B — Shuttle only
Rejected for now — Loom is more mature (AWS Shuttle is production but smaller community).

### Alt C — No concurrency verification
Rejected — it is 2026 and we are building a workflow engine. Inexcusable.

## Related

- ADR-006 — kernel ISP traits (Loom validates trait *impls*, not traits themselves)
- Shadow zone #4 — L1 taint runtime (will need Loom when resolved)
- Invariant #29 — "every crate with concurrent primitives ships Loom tests" (added to `nika-invariants.md`)

## Notes

The `nika-types` cancellation model runs in Diamond CI's existing tests leg
through `scripts/ci/check-loom-cancel.sh`. Its small state space is suitable
for each PR; broader models still follow the scheduling budget above. The
gate refuses a missing payload test, and uses library targets locally and
in CI. The model checks a separate relaxed payload before joining its
writer; independently weakening Acquire or Release makes the assertion fail.
It covers that publication edge only, not the runtime scheduler or shutdown.

If weekly Loom run takes > 30 minutes, switch the Loom job to nightly-only and Shuttle per-PR. Both tools pay their keep at this tier of quality.
