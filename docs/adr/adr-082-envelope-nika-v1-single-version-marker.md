---
id: ADR-082
title: "Workflow envelope: single version marker `nika: v1` (supersedes K8s apiVersion form)"
status: superseded
date: 2026-05-25
superseded_date: "2026-08-13"
phase: "Phase C — foundation lock v0.81"
deciders: ["@ThibautMelen"]
tags: ["schema", "yaml", "envelope", "version-marker", "spec-alignment"]
affects_crates: ["nika-schema", "nika-schema-ast", "nika-binding", "nika-catalog", "nika-event"]
affects_layers: ["L0", "L0.5"]
supersedes: ["ADR-021"]
superseded_by: ["ADR-113"]
related: ["ADR-021", "ADR-084", "ADR-085", "ADR-086", "ADR-087", "ADR-088", "ADR-113"]
requires: []
enables: []
amends: []
fci: ["FCI-009"]
inv: ["INV-019"]
shadow_zones: ["GATE-1"]
nika_codes: []
timeline: "v0.81 · aligns engine to public nika-spec"
follow_ups: ["nuke nika-schema parser envelope branch · parse `nika: v1` not `apiVersion:`/`schema:`"]
---

# ADR-082: Workflow envelope — single version marker `nika: v1`

> **⚠️ SUPERSEDED 2026-08-13** · the version slot decided here is gone,
> losslessly: the envelope is nine keys and the file's identity rides ON
> `nika:` (`nika: <kebab-id>` · never `v1` · `workflow:` is not an
> envelope key any more) · see [ADR-113](adr-113-envelope-identity-on-nika.md)
> and the nine-key envelope decision of 2026-08-11 (D-2026-08-11 · `nika-spec`
> 01-envelope). The body below is history and is not rewritten.

## Context

The public **`nika-spec`** (Apache-2.0 · created 2026-05-22 · 5 pillars
locked D-2026-05-22-N1..N8) defines the canonical workflow envelope as a
**single version marker**:

```yaml
nika: v1                 # required · language name (key) + contract version (value)
workflow: my-workflow-id # required · kebab-case · unique within file
tasks:
  <task-id>: ...
```

This **supersedes** the Kubernetes-style envelope adopted in **ADR-021**
(`apiVersion: nika.sh/v1` + `kind` + `metadata` + `spec`). The spec
explicitly rejects the two-field `apiVersion:`/`schema:` ceremony
(cf `nika-spec spec/01-envelope.md` · "Why one field, not apiVersion +
schema") on the grounds that modern specs converge on a single version
marker — OpenAPI writes `openapi: 3.1.0`, Docker Compose dropped its
`version:` field entirely.

Two contradictory legacy forms were also still scattered across engine
docs and surfaced by a verification swarm 2026-05-25:

- `apiVersion: nika.sh/v1` (the ADR-021 K8s form)
- `schema: "nika/workflow@0.12"` (the pre-Diamond brouillon form)

A phantom citation `ADR-044` ("adr-044-schema-envelope.md") was referenced
in several docs but the file never existed.

## Decision

**Adopt `nika: v1` as the single canonical envelope version marker for
every Nika user-facing workflow YAML.** The contract version is `v1`,
locked forever — a `v2` would require a breaking LANGUAGE contract change
· effectively never. This LANGUAGE-envelope axis is **orthogonal to the
engine version**: the engine ships real semver toward a 1.0 launch (ADR-002
· amended D-2026-06-20-N1), but `nika: v1` stays frozen across every engine
major (it is the real "v1" signal adopters always saw).

- The **`kind` discriminator** + multi-document rationale from ADR-021
  **survives** (9 doc kinds: Workflow · Skill · Agent · Provider · Mcp ·
  Eval · Recipe · Shield · Lints). Only the **version-field shape**
  changed: `apiVersion: nika.sh/v1` → `nika: v1`.
- The engine's internal canonical URI stays `https://nika.sh/spec/v1` for
  RDF / conformance tooling — but the **author never types a URL**.
- The serde discriminator becomes `#[serde(tag = "nika")]` with variant
  `#[serde(rename = "v1")]` (was `tag = "schema"` / `nika/workflow@0.12`).

## Consequences

- `nika-schema` parser rewrite (Phase D) targets `nika: v1`, not the
  `apiVersion:`/`schema:` forms.
- ADR-021 marked `superseded`; its `kind`/multi-doc analysis stays valid
  reference material.
- The phantom `ADR-044` references are removed; this ADR is the canonical
  engine-side anchor for the envelope decision (mirrors the public spec).
- Public canon source of truth: `nika-spec spec/01-envelope.md`.

## Related

- `docs/adr/adr-021-yaml-envelope-convention.md` (superseded · K8s form)
- `nika-spec` spec/01-envelope.md — the public `nika: v1` contract
- `docs/architecture/forward-compat-invariants.md` — FCI-009 envelope versioning
