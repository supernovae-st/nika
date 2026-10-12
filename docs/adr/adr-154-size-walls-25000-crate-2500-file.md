---
id: ADR-154
title: "Size walls: 25,000 production lines per crate and 2,500 lines per file, warnings at 15,000 and 1,500"
status: accepted
date: "2026-10-12"
phase: "pre-1.0 · engine gates"
deciders: ["@ThibautMelen"]
tags: ["architecture", "sizing", "crate-structure", "size-cap", "gates"]
affects_crates: []
affects_layers: ["L0", "L0.5", "L1", "L2", "L3", "L4"]
supersedes: []
superseded_by: []
related: []
requires: []
enables: []
amends: ["ADR-004", "ADR-023", "ADR-143"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["the crate members already split for the 15,000 wall stay as they are; merging one back is its own decision, made for cohesion, never for the counter"]
---

# ADR-154: Size walls — 25,000 production lines per crate and 2,500 lines per file

## Context

ADR-004 sized every crate to 15,000 lines so that a whole crate fit in an AI
context window (15k LOC ≈ 70k tokens ≈ 7% of a 1M-token window), and ADR-023 set
the file tiers at 800 (warning), 1,500 (block) and 3,000 (the `LOC-EXEMPT`
ceiling). Both walls are enforced by the CI ratchets and the hygiene dashboard.

On main `8fa5c9929`, measured with the production-line counter
(`scripts/ci/prod-loc.py` through `scripts/ci/check-crate-size.sh`), six crates sat
within twelve lines of the 15,000 wall: `nika-check` 14,999, `nika-serve` 14,999,
`nika-cli` 14,998, `nika-compile-cognition` 14,993, `nika-dap` 14,993 and
`nika-compile` 14,992. Reaching the wall had become the most common reason to
split a crate, not a cohesion argument: member splits ADR-110 (`nika-cli-host`),
ADR-137 (`nika-onboard`), ADR-143 (`nika-tui-view`), ADR-150
(`nika-session-intelligence`) and ADR-152 (`nika-compile-seats`), and three more
members in one integration (`nika-providers-access`, `nika-cli-host-typesafe`,
`nika-onboard-knowledge`). Fixes waited behind descents
(`docs/crate-specs/nika-dap.md`: 14,966/15,000 with two open pull requests blocked
at the push gate), and the native renderer needed its own ceiling, raised twice in
two days (ADR-143's amendments: 18,000 on 8 October, 19,000 on 9 October).

The context-window premise is now judged differently. ADR-004 computed its fit
against a 1M-token window, and the engine's agents read a crate through search and
targeted reads rather than loading it whole. A 25,000-line crate is about 117k
tokens by ADR-004's own ratio, about 12% of that window: the budget still bounds a
crate, but it no longer has to be the 7% ADR-004 computed.

## Decision

On 2026-10-12 the owner (@ThibautMelen) raised the walls, from 15,000 to 25,000
lines per crate and from 1,500 to 2,500 lines per file, through the gates and
everything they govern, and confirmed two levels for each.

| Level | Crate (production lines) | File (lines) | Consequence |
|---|---|---|---|
| Warning | above 15,000 | above 1,500 | hygiene dashboard YELLOW; never blocks |
| Limit | above 25,000 | above 2,500 | CI and the pre-push gate fail; blocks |
| `LOC-EXEMPT` ceiling | — | above 5,000 | blocks even with a whitelisted marker |

- **One ceiling for every crate.** `scripts/ci/crate-size-policy.sh` holds a single
  25,000 ceiling. The native renderer's scoped ceiling (19,000 for `crates/nika-tui`,
  ADR-143's 9 October amendment) is absorbed by it and removed: no crate has an
  exception. A probe may still only lower the ceiling, never raise it.
- **Each level is crossed only by more lines than it.** A crate of exactly 25,000
  production lines passes and 25,001 blocks; a file of exactly 2,500 lines passes
  and 2,501 blocks; a crate above 15,000 or a file above 1,500 warns.
- **The warnings keep the former walls visible.** Hygiene vector 24 reports every
  crate in the window above 15,000 and names the tightest one with its remaining
  headroom to 25,000; vector 12 reports every file above 1,500.
- **Unchanged.** What the counter measures (`src/` without `#[cfg(test)]` items,
  plus embedded jq), the `LOC-EXEMPT` whitelist (codegen, lookup-table,
  enum-mega), the 100-line function warning, and the rule that no code or
  documentation is deleted to satisfy a counter.

Rejected: keeping 15,000 and 1,500 with per-crate exceptions (exceptions
accumulate, as the renderer's two raises in two days showed); raising only the
crate wall (the file limit applied the same pressure one level down); removing the
walls (an unbounded crate loses the planned descent the warning provides); a
warning at 80% of the new ceiling (20,000), which would hide the band where crates
sat until now.

## Consequences

### Positive
- A fix or feature no longer waits for a member split forced by the counter; a
  split is decided for cohesion, with an ADR, inside the warning window.
- One crate ceiling and no exceptions: the policy file has no per-crate branch to
  review, and the dashboard measures every crate against the same number.
- The warnings keep 15,000 and 1,500 visible, so a growing crate or file is seen
  ten thousand (or a thousand) lines before it blocks.

### Negative
- A crate may now reach about 117k tokens and a file 2,500 lines: reading either
  whole costs more context and more review time.
- The crate warning window is 10,000 lines wide, so the dashboard's headroom
  figure, not the YELLOW bit, is what says how close a crate is.
- The members split for the old wall stay; their boundaries are history, not a
  measure of the new budget.

### Neutral
- ADR-004, ADR-023 and ADR-143 are amended by reference: their texts, the
  CHANGELOG and every dated measurement keep the numbers of their time.
- Hygiene vector 12 now reads every level as "more lines than"; it used to fail a
  file of exactly 1,500 lines that the CI ratchet accepted.

## Evidence / Affected code

- `scripts/ci/crate-size-policy.sh` — the single 25,000 ceiling; the TUI branch removed
- `scripts/ci/check-crate-size.sh`, `scripts/ci/prod-loc.py` — the CI crate wall and its counter
- `scripts/ci/check-loc-limits.sh` — the CI file wall, `MAX=2500`
- `scripts/hygiene/check-crate-size.sh` — vector 24: YELLOW above 15,000, RED above 25,000
- `scripts/hygiene/check-file-loc.sh` — vector 12: YELLOW above 1,500, RED above 2,500,
  CRITICAL above 5,000
- `scripts/ci/test-crate-size-source.py`, `scripts/hygiene/tests/crate-size.test.sh`,
  `scripts/hygiene/tests/file-loc.test.sh`, `scripts/hygiene/tests/loc-limits.test.sh` — the
  mutation proofs: each warning exits non-blocking, each limit blocks one line above it, and
  each test fails against the former gates
- `AGENTS.md`, `CONTRIBUTING.md`, `DIAMOND.md`, `ROADMAP.md`, `.claude/rules/nika-invariants.md`,
  `docs/architecture/ai-velocity.md`, the crate specs' cap rows — the active rules restated

## Alternatives considered

Covered under Decision: per-crate exceptions at 15,000, a crate-only raise, no walls,
and a warning at 20,000. Each was weighed against the measured pressure above.
