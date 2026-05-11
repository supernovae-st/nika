---
id: ADR-001
title: "Diamond orphan branch rewrite instead of iterative refactor"
status: accepted
date: "2026-04-13"
phase: "Phase 0 (bootstrap)"
deciders: ["@ThibautMelen"]
tags: ["architecture", "rewrite", "diamond", "orphan-branch"]
affects_crates: []
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4", "L5"]
supersedes: []
superseded_by: []
related: ["ADR-002", "ADR-003", "ADR-004", "ADR-037", "ADR-038"]
requires: []
enables: ["ADR-002", "ADR-003", "ADR-004"]
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 0"
follow_ups: []
---

# ADR-001: Diamond — orphan branch rewrite instead of iterative refactor

## Context

Legacy Nika (`main`, v0.79.3) had reached 322k LOC across 31 crates with 1,276 `.unwrap()` calls and 47 files above the 1,500-LOC cap. The single biggest correctness risk became **context loss**: no AI assistant could fit the whole system in a single context window. The math in `ROADMAP.md` is explicit — a 138k-LOC monolith (`nika-core`) ≈ 750k tokens ≈ 75% of a 1M-token context just to read one crate. Reviewing any non-trivial change devolved into pattern-matching against partial slices, which is how hallucinated refactors land.

Shadow-zone audit (16-agent Rust Council, 2026-04-13) confirmed 7 subsystems where documentation diverged from implementation. Incremental refactor would require untangling these in-place while maintaining compat.

## Decision

Create an **orphan branch `nika-diamond`** from scratch. No code inherited from `main`. Legacy `main` becomes a read-only reference via `git show brouillon:path`. Every line of the diamond is **rewritten**, not ported. 32 legacy crate directories are excluded from `Cargo.toml` members via the `exclude` key — they sit in the orphan working tree but do not participate in the workspace.

Legacy `main` stays frozen at v0.79.3. The diamond starts at **v0.80.0** to mark the boundary. Users on v0.79.x cannot benefit until v0.90 feature parity.

## Consequences

### Positive
- Each diamond crate fits entirely in an AI context window (≤15k LOC ≈ 70k tokens ≈ 7% of 1M-token context, 10× headroom).
- Zero accumulated legacy debt admitted — each crate enters via the 12-gate protocol (see ADR-003).
- Clean APIs enforced up-front; no breaking-change migrations mid-Diamond.

### Negative
- Approximately 11–12 months to feature parity (v0.90 milestone). Substantially longer than incremental refactor.
- No interim releasable state for Diamond users; the v0.79→v0.80 cliff is real.
- 5 admitted crates at v0.80.0-alpha.x means tiny installable surface today.

### Neutral
- Legacy bug fixes (CVE-class only) require separate back-port commits on `main`. None expected post-abandonment.

## Evidence

- `Cargo.toml` lines 1–17 — 5 admitted `members`, 32 excluded legacy crates
- `ROADMAP.md` lines 17–35 — "Why Diamond" context-window argument
- `CHANGELOG.md` lines 307–320 — v0.80.0-alpha.0 commit `42909b1c7`
- `DIAMOND.md` lines 40–53 — orphan-branch rationale
- memory: `POST_AUDIT_REVISIONS.md` — 10 locked decisions, 2026-04-13

## Alternatives considered

### Alt A — Iterative in-place refactor
Split each oversized file, add gates progressively, delete unwraps crate-by-crate on `main`. Rejected because the ~8k-LOC `nika-core/src/binding/` alone would need 4–5 ADR-scale decisions each and the audit proved human+AI context capacity would not hold across 322k LOC over 11+ months.

### Alt B — Workspace overlay (additive)
Keep `main`, add new crates alongside, migrate consumers lazily. Rejected because compat tax (dual APIs, feature flags, fallback logic) would re-create the 2,945-LOC `resolve.rs` class of file inside the new crates within months.

## Related

- ADR-003 — 12-gate admission (the quality floor every diamond crate must pass)
- ADR-004 — context-window-sized crates (the sizing rule the rewrite enables)
- `archive/nika-v0.79/adr/` (supernovae-hq monorepo) — 8 pre-Diamond ADRs, superseded by this rewrite

## Notes

Revisit if a v0.95 feature requires legacy behavior that was never documented. Otherwise this ADR is terminal — orphan rewrites are not reverted.
