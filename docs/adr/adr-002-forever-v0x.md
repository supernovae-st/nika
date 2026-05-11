---
id: ADR-002
title: "Forever v0.x -- no v1.0 target"
status: accepted
date: "2026-04-13"
phase: "Phase 0 (release model)"
deciders: ["@ThibautMelen"]
tags: ["versioning", "release-model", "semver"]
affects_crates: []
affects_layers: []
supersedes: []
superseded_by: []
related: ["ADR-001", "ADR-007", "ADR-028", "ADR-037"]
requires: ["ADR-001"]
enables: ["ADR-007"]
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 0"
follow_ups: []
---

# ADR-002: Forever v0.x — no v1.0 target

## Context

Nika is developed primarily by a solo author augmented by AI. Committing to a v1.0 target introduces two failure modes: (a) rushing breaking changes into a 1.0 window to "lock the API," producing a worse API than iteration would; (b) refusing to change load-bearing APIs post-1.0 because SemVer punishes breaking changes on major releases. Neither outcome serves correctness.

Context: legacy Nika had 17 releases in v0.x space (v0.1 → v0.79.3) across ~6 months of solo+AI work. Phase D (Diamond) is projected at ~10–11 months to v0.90 milestone. Applying strict SemVer 1.0 semantics over that horizon forces compat decisions that conflict with Diamond's "zero dead code" philosophy.

## Decision

**Nika remains at v0.x indefinitely.** No v1.0 target. Ever.

- Phase milestones (v0.90, v0.95, v0.100) are **markers**, not contracts. They signal accumulated crate admissions (40–42 @ v0.90, ~63 @ v0.95, ~75 @ v0.100, cap 100).
- Breaking changes ship on MINOR bumps (v0.90 → v0.91) without ceremony. `#[non_exhaustive]` + `pub fn new()` (ADR-007) preserve forward-compat *by construction* within minor ranges for downstream struct-literal use.
- The CHANGELOG distinguishes `Breaking`, `Added`, `Changed`, `Fixed`, `Deprecated` sections. Breaking changes are explicit but not gated by a SemVer major.
- Public documentation (`README.md`, `MANIFESTO.md`) states this explicitly so users adopt with correct expectations.

## Consequences

### Positive
- Freedom to refactor load-bearing APIs when evidence demands (post-audit findings, shadow-zone resolutions).
- No artificial "pre-1.0" vs "post-1.0" discipline gap. Every release held to the same quality floor.
- Quality-first signaling: a v0.95 release with 95% mutation coverage is tangibly better than a v1.0 with compat shims.

### Negative
- Some ecosystem tools (package registries, dependabot rules) treat pre-1.0 crates as "unstable" and apply stricter update policies. Acceptable — Nika distributes as a CLI + library, not a churned dependency.
- Requires explicit user education: `v0.x` here ≠ "pre-alpha quality." README must state this.

### Neutral
- AGPL license already signals long-term project commitment independent of version number.

## Evidence

- `ROADMAP.md` — "forever v0.x" in header, phased crate counts
- `Cargo.toml` line 19 — `version = "0.80.0"` at workspace root, inherited
- `CHANGELOG.md` header — explicit "Nika follows forever-v0.x" statement
- memory: `feedback_no_deadlines.md` — "never pressure with timelines"
- memory: `POST_AUDIT_REVISIONS.md` — "Version model: Forever v0.x"

## Alternatives considered

### Alt A — Target v1.0 at milestone v0.90
Ship v1.0.0 once the 40–42 admitted crates hit the quality bar. Rejected: v0.90 is an arbitrary crate count, not an API stabilization moment. Memory features (v0.95) and agent-v2 (v0.100) are forward-known to require kernel API evolution; committing v1.0 before those land is dishonest.

### Alt B — Calendar versioning (YYYY.MM)
`2026.04`, `2026.10`. Rejected: loses the "quality ratchets up with number" signal. Users infer from v0.80 → v0.90 that substantial progress happened.

## Related

- ADR-001 — orphan rewrite (why we're starting the v0.8x line)
- ADR-007 — forward-compat invariants (how we preserve struct-literal compat within minor ranges)

## Notes

If the ecosystem changes such that "forever v0.x" actively harms adoption (e.g., major packaging tool blocks < 1.0 by policy), revisit. Not expected within 24 months.
