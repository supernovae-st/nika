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

> **⚠️ AMENDED 2026-06-20 (D-2026-06-20-N1) — this doctrine is RETIRED.**
> The "forever-v0.x · no v1.0 target" model described below is **superseded**.
> Nika now follows **real semver toward a 1.0.0 public launch** (the engine
> is at 0.90.0 RC-grade; 1.0.0 ships when the 7 shadow zones are green; the
> remaining crates land additively across the 1.x minors). The `nika: v1`
> LANGUAGE envelope is frozen forever and is unaffected (a different axis).
> The original text is preserved below per `cross-source-validation.md` §2.7.

> **⚠️ AMENDED 2026-06-20 (D-2026-06-20-N1) · "forever-v0.x · no v1.0 ever" is RETIRED → real semver toward a 1.0 launch.**
> The decision below is SUPERSEDED at the version-policy layer. Canon courant ·
>
> - The engine is now **0.90.0** — release-candidate grade. The usable vertical
>   slice works today (the 4 verbs `infer`/`exec`/`invoke`/`agent` · 14 providers ·
>   fs/http/blob effects · the static-check conformance pass · MCP + LSP in-binary).
>   The remaining gap to "a person installs it and runs a workflow" is the
>   operator CLI front-door (`nika-cli`) + `nika-extract` + the `nika` binary
>   composition root.
> - Design-partner hardening ships as **`1.0.0-rc.N`**.
> - **First public launch = `1.0.0`** — a real major, justified by design-partner
>   validation + a frozen language spec + a real installable binary. (The original
>   Alt-A objection — "agent-v2 + memory need kernel evolution before 1.0" — is now
>   largely resolved · `nika-verb-agent` is admitted and the memory kernel hooks
>   are `#[non_exhaustive]`-reserved per ADR-007, so a 1.0 surface no longer
>   forward-blocks the Connectome.)
> - **Post-1.0 = real semver.** The adopter contract is (1) the `nika: v1` LANGUAGE
>   envelope (frozen forever · unaffected by engine majors · it is the real "v1"
>   signal adopters always saw) + (2) the CLI surface. **1.x MINORS are additive**
>   (remaining crates toward the 42-crate architecture · new builtins · new
>   providers · polish). **`2.0` is reserved for the Connectome era** (memory +
>   cognition · ADR-004) — the next architectural epoch.
> - The original rationale (freedom to break load-bearing APIs without 1.0
>   ceremony) is PRESERVED for the pre-1.0 window (now) and continues to apply to
>   INTERNAL Rust crate APIs post-1.0 — external adopters consume the language +
>   CLI, not the crate API. What changes · the public version number now CLIMBS to
>   communicate maturity instead of sitting at 0.x forever.
>
> The `v0.8X.Y` layer-phase tag scheme and the "42 crates @ v0.90 milestone /
> no v1.0" framing are SUPERSEDED · 42 crates is now a post-1.0 target reached
> across 1.x minors. The original body is preserved below per
> cross-source-validation §2.7 (zero silent rewrite · this ADR's filename + title
> stay frozen as historical anchors).

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
