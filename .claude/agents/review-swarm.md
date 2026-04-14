---
name: review-swarm
description: Parallel 3-agent review of a Rust crate. Runs spn-nika, spn-rust, and feature-dev reviewers in parallel, merges P0/P1 findings, reports a single deduplicated list. Use before admission (Gate 11).
model: opus
tools: Bash, Read, Grep, Task
---

# Review swarm — `$ARGUMENTS`

Gate 11 of the 12-gate admission process. Three reviewers run in parallel,
each one independent. Each reports ONLY P0 (blocking) and P1 (must fix before
admission). Lower-priority suggestions are dropped to avoid noise.

## Run in parallel

Spawn three sub-agents at once (one message, three Agent calls):

1. **spn-nika:code-reviewer** — Nika-specific patterns, `NikaErrorCode`, kernel
   trait usage, `#[non_exhaustive]`, trust propagation, event-log emission, the
   7 shadow zones.

2. **spn-rust:rust-pro** — idiomatic Rust, ownership, lifetime soundness,
   `unwrap()`/`expect()` in src (forbidden), `async_trait` vs `trait_variant`,
   clippy pedantic findings not captured by workspace lints.

3. **feature-dev:code-reviewer** — architecture, public API minimality, layer
   invariants (no upward deps), `#[non_exhaustive]` + `::new()` constructor
   discipline, forward-compat, file/function LOC caps.

Each agent receives:
- Crate path: `tools/$ARGUMENTS/`
- Crate spec: `docs/crate-specs/$ARGUMENTS.md`
- Reporting constraint: P0 and P1 only, 1-line each, with file:line reference.

## Merge findings

After all three return, produce a single deduplicated list:

```
┌─ P0 — BLOCKING ADMISSION ───────────────────────────────┐
│ (none) — or concrete items with file:line + fix sketch  │
└─────────────────────────────────────────────────────────┘

┌─ P1 — MUST FIX BEFORE ADMIT ────────────────────────────┐
│ ...                                                     │
└─────────────────────────────────────────────────────────┘
```

Same-session fix requirement: every P0 + P1 must be resolved before the
admission commit. Not next session. Not next sprint. Same session.

If an identical finding appears from multiple reviewers, count it as one but
note which reviewers raised it (higher confidence signal).

## Output contract

Return a structured report:
- `p0_count: N`
- `p1_count: N`
- `verdict: ADMIT | FIX-THEN-ADMIT | REDESIGN`

`REDESIGN` only if P0 count > 5 or touches foundational architecture.
