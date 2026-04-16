---
id: ADR-021
title: "YAML envelope convention: apiVersion + kind + metadata + spec, multi-doc, 9 doc kinds"
status: accepted
date: "2026-04-16"
phase: "Phase C — foundation lock v0.81"
deciders: ["@ThibautMelen"]
tags: ["schema", "yaml", "envelope", "k8s", "multi-doc", "kind-discriminator"]
affects_crates: ["nika-envelope", "nika-schema-ast", "nika-schema", "nika-binding", "nika-catalog", "nika-event"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-022", "ADR-023", "ADR-024"]
requires: []
enables: ["ADR-022"]
amends: []
fci: ["FCI-009"]
inv: ["INV-019"]
shadow_zones: ["GATE-1"]
nika_codes: []
timeline: "v0.81"
follow_ups: ["nuke nika-schema parser, rewrite to envelope"]
---

# ADR-021: YAML envelope convention

## Context

Phase D Round 2c-2e shipped a parser for top-level workflow scalars (`name`,
`description`, `goal`, `provider`, `model`, `schema`) and `tasks:` sequence.
Background research (4 web-research agents, 2026-04-16) converged unanimously
on adopting the K8s envelope pattern (`apiVersion + kind + metadata + spec`)
with multi-document YAML support. Evidence: every workflow DSL with >2 doc
types (Argo, Tekton, Flux, Drone-1.x, Helm) ended up with this envelope after
flat-form regret migrations.

Nika has 9 user-facing doc kinds at v0.90: Workflow, Skill, Agent, Provider,
MCP, Eval, Recipe, Shield, Lints (+ Memory v0.95, Plugin v0.100 reserved).
Without an envelope, every kind reimplements identity + version semantics.

## Decision

**Adopt the K8s envelope as the canonical shape for every Nika user-facing
structured config (YAML and TOML)**:

```yaml
apiVersion: nika.sh/v1
kind: Workflow                # closed enum: Workflow|Skill|Agent|Provider|
                              #              Mcp|Eval|Recipe|Shield|Lints|
                              #              Memory(reserved)|Plugin(reserved)
metadata:
  name: my-workflow
  description: ...
  labels: { ... }             # optional
  annotations: { ... }        # optional
spec:
  defaults: { model: openai/gpt-4o }
  tasks: [...]                # for kind: Workflow
```

**Multi-document YAML** (--- separator) supported: one file may declare
multiple kinds (Workflow + Skill + Agent), parsed into `Vec<RawDocument>`.

**Model identifier**: single string `provider/model` (LiteLLM format), e.g.
`openai/gpt-4o`, `anthropic/claude-sonnet-4.5`. Drop separate `provider:` +
`model:` fields.

**Drop workflow-level `goal:`** — CrewAI agent-specific concept, not a
workflow concept (0 of 8 surveyed workflow DSLs have it). If goal-driven
agentic workflows need it, place in `spec.orchestrate.goal:`.

**Format support**:
- YAML: workflows (DSL, user-authored)
- TOML: catalog data, agent presets, lint rules (config-as-data)
- Markdown + YAML frontmatter: skills/agents (prose-heavy, Cursor/Claude
  convention)

All three formats use the same envelope keys (apiVersion + kind + metadata +
spec).

## Consequences

- ✅ Single envelope shape across 9 doc kinds = LLM-friendly (Nika-brain
  generates correct shape on first try)
- ✅ Forward-compat: adding a kind (e.g., Memory at v0.95) requires only
  adding a variant to the closed `Kind` enum
- ✅ K8s envelope is in 100% of LLM training data — Nika-brain pattern-matches
  without prompting tax
- ❌ Diverges from typical "Rust workspace" convention (TOML flat) — but
  workflow engines diverge from compilers (the relevant comparison set)
- ❌ Existing nika-schema parser (Round 2c-2e, 38 tests) needs full rewrite
  in Phase D — explicit cost accepted (v0.1, 0 users, break freely)
- ⚠️ Hygiene vector 23 (check-workflow-envelope.sh) MUST land in Phase B
  before any envelope code is written — guards against regression

## Alternatives considered

- **Flat top-level** (current Round 2c shape) — dies at ~8 fields per
  GitLab/Drone/Dify post-mortems
- **Hybrid (Dify-style: version + kind + nested app block)** — half-step that
  loses metadata uniformity
- **URL-path discrimination (apiVersion: nika.sh/v1/Workflow)** — breaks YAML
  editor schema hints
- **Filename convention only** — moves truth out of doc, hurts multi-doc

See `~/.claude/.../memory/project_foundation_v081_constellation.md` for the
4-agent research synthesis.
