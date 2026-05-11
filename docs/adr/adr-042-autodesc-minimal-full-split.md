---
id: ADR-042
title: "Autodesc MINIMAL/FULL split · forcing-function moat at W4"
status: proposed
date: "2026-05-12"
phase: "Phase 1.5 (W4 autodesc-MINIMAL · Phase 2 autodesc-FULL)"
deciders: ["@ThibautMelen"]
tags: ["memory", "autodesc", "moat", "split", "lock-031-spirit"]
affects_crates: ["nika-autodesc"]
affects_layers: ["L1"]
supersedes: []
superseded_by: []
related: ["ADR-004", "ADR-040", "ADR-078"]
requires: ["ADR-004", "ADR-040"]
enables: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "W4 autodesc-MINIMAL · Phase 2 autodesc-FULL (post 2026-08-30)"
follow_ups: []
---

# ADR-042 — `nika-autodesc` MINIMAL/FULL split

## Context

ADR-004 lists `nika-autodesc` (1 crate · ~600 LOC) as the moat differentiator.
Per internal cognitive audit 2026-05-11 + user lock G3=C 2026-05-12 · the 600 LOC
spans heterogeneous concerns :

- **Provenance ingest** (`vocab.rs` + `schema.rs` + `ingest.rs` · ~200 LOC) — RDF-star
  reification for fact provenance
- **Query** (`query.rs` · ~100 LOC) — SPARQL-star with provenance chain
- **OWL 2 punning** (`punning.rs` · ~100 LOC) — schema-evolution-via-punning
- **Schema evolution** (`evolve.rs` · ~100 LOC) — discovered → user realm promotion
- **Graph summarization** (~100 LOC · the « automatic graph summarization » headline)

Shipping all 600 LOC together at W9 (post 7 commodity satellites · per ADR-040 admission
DAG) means the MOAT claim lands LAST · violates LOCK-031 spirit (« no infra behind locked
gate »). Per Agent 4 cognitive audit recommendation · ship a forcing-function subset early.

## Decision

Split `nika-autodesc` into two admission waves :

### `nika-autodesc-minimal` (W4 · ~300 LOC)

- `vocab.rs` · RDF-star vocabulary + namespace constants
- `schema.rs` · schema-types + datatype layer
- `ingest.rs` · `<< s p o >> nm:provenance [...]` annotation pattern
- `query.rs` · SPARQL-star with provenance chain
- `punning.rs` · OWL 2 punning subset (deterministic rules · no LLM)

Validates moat-CLAIM-1 (« provenance-attached + schema-evolution-via-punning ·
no OSS Rust-embedded production competitor with our 4-axis combo » per G1=B lock)
at W4 · BEFORE 5 commodity satellites land. Forcing-function for credibility.

### `nika-autodesc-full` (Phase 2 · ~300 LOC delta)

- `evolve.rs` · discovered → user realm promotion ceremony
- `summarize.rs` · graph summarization via `nika-graph-algos` (Louvain · centrality · communities)
- Re-exports + façade pattern over `nika-autodesc-minimal`

**Admission trigger** (explicit · per LOCK-031 spirit + `steal-pattern.md` Step 0
consumer-signal gate + `time-architecture.md` Layer 3 quarterly review) · admit
`nika-autodesc-full` WHEN ·
1. `nika-graph-algos` admitted (W8 satellite · per ADR-040 dependency · structural blocker)
2. AND ≥1 consumer signal explicitly cites graph-summarization use case (issue · PR
   citation · scheduled launch · OR locked decision)
3. AND quarterly review (Q3 2026 close · 2026-10-15) confirms scope still load-bearing
   (Keeper Test · « lost autodesc-FULL today · re-invent? » must answer YES)

Without all three · `autodesc-full` stays parked. Prevents indefinite Phase 2 deferral
masquerading as « roadmap ».

### Cargo features (per ADR-040)

```toml
autodesc-minimal = ["dep:nika-autodesc-minimal", "rdfs-reasoner", "temporal"]
autodesc-full = ["dep:nika-autodesc-full", "autodesc-minimal", "graph-algos"]
```

`autodesc-full` includes `autodesc-minimal` · linear dep · no diamond.

## Consequences

### Positive
- **Moat lands W4** · 5 commodity satellites later validate the foundation but don't gate the differentiator claim.
- **Public credibility cascade** · « autodesc shipped W4 · others 4-5x competitors do this · we do it earlier and in 300 LOC » becomes citable in strategy docs (per G4=A unified-runtime reframe).
- **Forcing-function avoids drift** · if autodesc-minimal can't ship clean at W4 · we learn early · re-plan · NOT 4 months later at W9.
- **2-crate split per ADR-004 + ADR-040** · each independently publishable on crates.io · `nika-autodesc-minimal` MIT-friendly subset of the AGPL umbrella.

### Negative
- **Adds 1 crate to admission DAG** · 9 satellites instead of 8. Per ADR-006 amendment 40-42 crate cap · we have headroom (current 7 admitted · target 40-42 · cap 100).
- **Split discipline required** · `summarize.rs` MUST NOT leak into minimal · enforced by `cargo-deny` layer rule (see ADR-006 strict-downward-deps).
- **Documentation cost** · 2 README + 2 CHANGELOG. Acceptable ceremony.

### Neutral
- Forward-compat per ADR-007 · minimal API stays additive · full re-exports minimal.

## Evidence
- ADR-004 — 1+8 architecture (this ADR refines to 1+9)
- ADR-040 — Cargo feature matrix (autodesc-minimal / autodesc-full bundles)
- Internal cognitive audit 2026-05-11 — 600 LOC heterogeneous concerns + LOCK-031 spirit application

## Alternatives considered

### Alt A — Ship monolithic at W9 (status quo per ADR-004)
Rejected per G3=C lock · moat lands last · forcing-function lost · audit pressure on
600 LOC heterogeneous scope at admission gate.

### Alt B — Split into 5 crates (one per concern)
Rejected · over-fragmentation · LOC budget per crate would be ~120 each · violates
the 5,900-LOC scope discipline + 40-42 cap discipline.

### Alt C — Ship summarization as separate crate `nika-graph-summarize` not under autodesc umbrella
Considered · graph-summarization is the « autodesc » HEADLINE per ADR-004 public framing ·
keeping under autodesc-full umbrella preserves the naming + marketing narrative.

## Related
- ADR-004 — 1+8 architecture (this ADR amends to 1+9)
- ADR-006 — kernel ISP + layer discipline
- ADR-040 — Cargo features (consumer of split)

## Notes
ADR-004 reference count « 8 satellites » in `dx/.claude/rules/naming-memory-subsystem.md`
v2.2 will read « 8 satellites + autodesc-FULL companion » post v2.3 reframe. Naming
clarification : the « 8 » canon stays semantically (8 algorithmic concerns) but
implementation count is 9 crates · per ADR-006 amendment crate-count discipline allows
this split as fine-grained concern separation NOT category expansion.
