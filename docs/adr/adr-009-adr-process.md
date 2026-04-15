---
id: ADR-009
title: "ADR process and hook discipline (meta)"
status: accepted
date: "2026-04-14"
phase: "Phase 0 (process bootstrap)"
deciders: ["@ThibautMelen"]
tags: ["process", "adr", "documentation", "meta"]
affects_crates: []
affects_layers: []
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-011"]
requires: ["ADR-003"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.80.0, Phase 0"
follow_ups: ["Auto-generated INDEX.md at 50+ ADRs"]
---

# ADR-009: ADR process + hook discipline (meta)

## Context

Decisions rot. Architectural reasoning captured in Slack, commit messages, or agent memory degrades faster than the code it explains. Six months from a decision, the question *"why did we pick `trait_variant` over `async_trait`?"* has to be re-derived from diffs — or, more often, never answered.

Legacy Nika (v0.1–v0.27) had 8 ADRs, all `Accepted` status, no superseding chain, invisible to open-source contributors (they lived in the private monorepo at `dx/adr/`). When Diamond big-banged, those 8 ADRs were archived wholesale — not because they were wrong, but because the **link between decision and code had rotted**.

Diamond is a multi-year effort. Without a mechanical discipline for writing and maintaining ADRs, the same rot is inevitable.

## Decision

**Every architecturally-significant decision (ASD) lands with an ADR in the same commit.**

### Location

ADRs live in `nika/engine/docs/adr/` (this repo, public). Diamond ADRs **must** be public — the rationale is educational for open-source contributors and prevents the "invisible decisions" problem that killed the legacy ADRs.

### Numbering

Sequential: `adr-001`, `adr-002`, … Never reused, never reordered. Superseded ADRs keep their number and add `Superseded by ADR-NNN` to their status.

### Template

`docs/adr/TEMPLATE.md` is the mandatory shape:

- Frontmatter: `Status` (Proposed / Accepted / Superseded) + `Date` + `Phase` + `Deciders`
- Sections: Context → Decision → Consequences (Positive / Negative / Neutral) → Evidence → Alternatives → Related → Notes

### When to write one

An ASD matches ≥2 of these triggers:

- Crosses a crate or layer boundary (affects >1 crate, or defines a layer contract)
- Introduces a new invariant downstream code must respect
- Locks a public API (trait, struct, enum that other crates depend on)
- Makes a non-reversible choice (can't be undone without breaking users)
- Trades a quality attribute (perf vs correctness, simplicity vs flexibility)
- Replaces or supersedes a prior ADR

**Do not write an ADR for:** single-file refactors, bug fixes, dependency bumps, renames without behavior change, style decisions.

### Enforcement (the "hook")

`scripts/ci/check-adr-coverage.sh` — auxiliary gate script, warn-only initially:

1. Parses workspace `members` from `Cargo.toml`
2. For each admitted crate, greps `docs/adr/*.md` for `\bcrate_name\b`
3. Reports coverage; exits 0 (warn) unless `FAIL_ON_MISS=1` is set (CI strict mode)

Integration points:

- `crate-admit` skill invokes this script at Gate 12 (atomic commit) as a soft check
- CI `.github/workflows/diamond-ci.yml` runs `FAIL_ON_MISS=1` on the `main` branch
- PR template (`.github/pull_request_template.md`) asks: *"Does this change introduce an ASD? If yes, the ADR number: ADR-NNN. If no, explain why."*

### Supersession

When a decision is replaced:

1. Set old ADR's status to `Superseded by ADR-NNN`
2. Write the new ADR referencing `Supersedes: ADR-M` in its Related section
3. Commit both changes together (one atomic commit)

ADRs are **never deleted**.

## Consequences

### Positive
- Clarity for future-self and contributors: every hard decision is traceable to a durable document.
- Onboarding accelerant: new contributor reads `docs/adr/` to understand the reasoning, not just the shape.
- Supersession chain lets anyone answer *"what was the old approach, why did it change?"* in one git-log.
- Public ADRs signal project seriousness — mature OSS projects (Rust, Kubernetes, Helix) all ship them.

### Negative
- Writing an ADR is friction. A rushed session may skip it.
- Coverage-check is pattern-based (grep crate name); misses ADRs that discuss a crate without naming it.
- ADR count grows; at >50 ADRs, navigation becomes harder without tooling. Revisit with `docs/adr/INDEX.md` auto-generation then.

### Neutral
- The meta-ADR (this one) is self-referential: it mandates itself. That is acceptable — bootstrap is always a free variable.

## Evidence

- `docs/adr/README.md` — full process doc + template usage
- `docs/adr/TEMPLATE.md` — mandatory template
- `scripts/ci/check-adr-coverage.sh` — the coverage hook
- `docs/adr/adr-001-diamond-orphan-branch.md` through `adr-008-toml-driven-catalog.md` — 8 inaugural ADRs demonstrating the process
- memory: `POST_AUDIT_REVISIONS.md` — the process hole that motivated this meta-ADR
- Legacy reference: `archive/nika-v0.79/adr/LEGACY-NOTE.md` (supernovae-hq monorepo) — the rot that this ADR prevents

## Alternatives considered

### Alt A — Informal ADRs (blog posts, wiki pages)
Rejected. Not version-controlled with the code, lost on migrations, invisible to contributors.

### Alt B — ADRs in commit messages only
Rejected. git-log is lossy UI for decisions that reference each other (supersession is hard to trace).

### Alt C — Private ADRs (in `dx/adr/`, like legacy)
Rejected. Nika is AGPL open source; hiding architectural rationale from contributors is both technically counterproductive and philosophically inconsistent.

### Alt D — ADR-per-crate (nested in `crate-specs/`)
Rejected. Cross-cutting ADRs (layered architecture, forward-compat invariants) span multiple crates. Central `docs/adr/` preserves the sequential narrative.

## Related

- ADR-001 through ADR-008 — the inaugural batch
- `scripts/ci/check-adr-coverage.sh` — enforcement hook
- Michael Nygard, [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) (2011)
- `archive/nika-v0.79/adr/LEGACY-NOTE.md` (supernovae-hq) — prior-art, what happened when the habit wasn't enforced

## Notes

If after ~50 ADRs navigation becomes painful, add `docs/adr/INDEX.md` generated from ADR frontmatter (status, date, tags). Keep README.md as the human narrative; INDEX.md as the machine view.
