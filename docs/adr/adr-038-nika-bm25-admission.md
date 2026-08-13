---
id: ADR-038
title: "Admit nika-bm25 as the first L1 memory satellite (CRAFT-from-scratch BM25 scorer)"
status: accepted
date: "2026-05-12"
phase: "Phase 1.5 memory · Wave 3 (pre-admission gate-readiness)"
deciders: ["@ThibautMelen"]
tags: ["memory", "L1", "bm25", "ir", "satellite", "CRAFT", "diamond"]
affects_crates: ["nika-bm25", "nika-kernel", "nika-error"]
affects_layers: ["L1"]
supersedes: []
superseded_by: []
related: ["ADR-001", "ADR-003", "ADR-016", "ADR-023", "ADR-037", "ADR-039", "ADR-040", "ADR-078", "ADR-107"]
amends: []
requires: ["ADR-078"]
enables: []
fci: ["FCI-001", "FCI-002", "FCI-003"]
inv: ["INV-019", "INV-028"]
shadow_zones: []
nika_codes: ["NIKA-620..629 reserved"]
timeline: "Wave 3 implementation · ~7h focused · post-W2 close"
follow_ups:
  - "Scaffold crates/nika-bm25/ with hand-rolled scorer (~600 LOC across 4 files)"
  - "Add 12-gate admission ceremony evidence (spec · TDD · mutation · benches · etc.)"
  - "Land insta + proptest test suite anchored on Manning/Raghavan/Schütze fixtures"
  - "Promote ADR-038 status to accepted on workspace admission (W3 close)"
---

# ADR-038: Admit nika-bm25 as the first L1 memory satellite

## Status

**Proposed** 2026-05-12 · pre-admission gate-readiness pass per Diamond Phase 1 entry plan §2 W2.4. Status flips to **Accepted** when the W3 12-gate ceremony lands the crate in `crates/nika-bm25/` and registers it in the workspace `[workspace] members` array.

## Context

The Diamond memory subsystem architecture (locked separately in private DX doctrine and surfaced publicly via `crates/nika-kernel-ai/src/memory.rs:1-413`) ships as 1 L2 orchestrator (`nika-memory`) + 8 L1 satellites — `nika-hnsw` · `nika-bm25` · `nika-rrf` · `nika-fsrs` · `nika-graph-algos` · `nika-rdfs-reasoner` · `nika-temporal` · `nika-autodesc` ★. Each satellite is independently admissible via the 12-gate ceremony (ADR-003) and publishable standalone on crates.io as a SuperNovae contribution to the Rust RDF/ML ecosystem.

Per Diamond Phase 1 entry plan §3 (W3 wave) `nika-bm25` is the **simplest L1 satellite to admit first** because :

- BM25 has **zero deps beyond a hand-rolled scorer** (no Oxigraph integration yet · no embedding-model dep)
- The algorithm is a battle-tested **IR standard for 15 years** (Robertson · Walker · SIGIR 1994 · plus the variant catalog in Manning · Raghavan · Schütze *Introduction to Information Retrieval* 2008 ch.11)
- It is testable in isolation against toy corpora · no `nika-kernel` plumbing needed yet
- It proves the satellite-admission flow before tackling the harder satellites (`nika-hnsw` needs an empirical embedding distribution · `nika-autodesc` is the moat differentiator · both should land later)

Per Phase 1 entry plan §2 the kernel-side memory trait surface (`MemoryRemember` / `MemoryRecall` / `MemoryForget` / `MemoryStore` blanket / `MemoryLifecycle` standalone) is already shipped at `crates/nika-kernel-ai/src/memory.rs` (607 LOC · `trait_variant::make` pattern per FCI-001 Decision A1). The kernel-side `MemoryError` enum maps to NIKA_601..604 per Diamond W2.1 + W2.2. `nika-bm25` will :

- Implement `MemoryRecall` (lexical-scoring branch · alongside semantic HNSW)
- Reserve its own NIKA-620..629 sub-range within the Memory 600-649 category (per Phase 1 entry plan §2 architect Q2 sub-allocation lock)

## Decision

Admit `nika-bm25` to the Diamond workspace at Wave 3 as the first L1 memory satellite. Implementation = **CRAFT from scratch** (ADR-001) hand-rolled · NOT a wrapper around `tantivy` (which would violate ADR-001 + introduce ~500K LOC of search-engine infrastructure we do not need at L1).

Target shape ·

```
crates/nika-bm25/
├── Cargo.toml          name=nika-bm25 · version=0.0.1 · license=AGPL-3.0-or-later
├── src/
│   ├── lib.rs          public API surface · MemoryRecall impl
│   ├── scorer.rs       Bm25 struct · k1/b/avgdl params · score() formula
│   ├── index.rs        inverted-index builder · tokenize + postings + IDF
│   └── query.rs        query parsing + multi-term scoring
└── tests/
    └── manning_fixtures.rs  1:1 fidelity against published BM25 textbook examples
```

Default parameter values · `k1=1.2` (term saturation per Robertson) · `b=0.75` (length normalization · industry default). Configurable via `Bm25::with_params()` builder.

Reserve **NIKA-620..629** sub-range for `nika-bm25` error codes (Phase 1 entry plan §2 architect Q2 allocation · within the Memory 600-649 category locked Diamond W2.1).

## Consequences

### Positive
- Proves the L1 satellite admission flow (lower-bound complexity reference for the 7 remaining satellites)
- Adds a real-world `MemoryRecall` implementer the kernel mock can be tested against (parity with the abstract trait)
- Lexical scoring complements future semantic HNSW (the orchestrator fuses both via RRF · nika-rrf admits at W4)
- Publishable on crates.io as `nika-bm25` (sovereign IR primitive · AGPL · zero vendor lock · per Rule 3 vendor-neutral default)
- ~600 LOC well under the 15k crate cap and 1500-LOC file cap (ADR-023 + nika-invariants §"Max LOC per file")

### Negative
- Adds 1 admitted crate to the workspace count (was 7 admitted · target 40-42 · acceptable)
- Hand-rolled BM25 means we own the math forever (mitigation · IR textbook fixtures freeze the parity surface · `cargo mutants` keeps the impl honest)
- Lexical-only · cannot do semantic similarity until `nika-hnsw` ships (intentional · v0.1 scope keeps the surface small)

### Neutral
- NIKA-620..629 sub-range becomes RESERVED in the Memory category (no concrete codes yet · landed when nika-bm25 errors crystallize during impl)
- The crate ships zero `nika-kernel` plumbing at v0.1 (just the algorithm) · plumbing waves later when the orchestrator consumes it

## 12-Gate admission readiness (Wave 3 evidence anchors)

Per ADR-003 the admission ceremony runs all 12 gates. Pre-flight status before W3 ·

| Gate | Check | Pre-flight verdict | Where it lands |
|---|---|---|---|
| **1** | SPEC · `docs/crate-specs/nika-bm25.md` exists | ⏸ pending W3 | New file at W3 ; must cite this ADR + the Diamond Phase 1 entry plan §3 |
| **2** | LOC cap · ≤15k | ✅ trivially (target ~600 LOC) | `wc -l crates/nika-bm25/src/*.rs` |
| **3** | Single-file cap · ≤1500 LOC | ✅ trivially (scorer ~200 · index ~250 · query ~150) | Per ADR-023 file modularity discipline |
| **4** | Function cap · ≤100 LOC | ✅ design splits per-term scoring + corpus stats | Per nika-invariants §"Max LOC per fn" |
| **5** | Zero `.unwrap()` in `src/` | ✅ enforced via `workspace.lints.clippy unwrap_used = "deny"` | Per dx/.claude/rules/security.md §"Propagate errors" |
| **6** | Layer registry (L0/0.5/1/2/3/4/5) | ✅ L1 satellite layer | `[workspace.metadata.diamond.layers]` row added in W3 admission commit |
| **7** | Forward-compat invariants (8 patterns) | ✅ FCI-001 `trait_variant::make` (impls `MemoryRecall`) · FCI-002 `#[non_exhaustive]` on public error enum · FCI-003 versioned BM25 index format | Per `docs/architecture/forward-compat-invariants.md` |
| **8** | Test coverage (TDD · RED→GREEN) | ⏸ pending W3 | Insta snapshots + proptest fuzzing + 1:1 textbook fidelity tests |
| **9** | Clippy `-D warnings` | ✅ enforced via workspace lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| **10** | `cargo fmt --check` | ✅ enforced via lefthook pre-commit | Pattern 1 atomic stage+commit |
| **11** | Documentation (`#![warn(missing_docs)]`) | ⏸ pending W3 | Crate-level `//!` doc + per-item `///` doc |
| **12** | License + AGPL header on every file | ✅ enforced via hygiene vector | `SPDX-License-Identifier: AGPL-3.0-or-later` |

Plus the mandatory pattern gates per `nika/engine/.claude/CLAUDE.md` ·
- **MUTATION ≥90%** via `cargo mutants -p nika-bm25`
- **PROPERTY** testing via `proptest` (TF-IDF monotonicity · score positivity · empty-corpus invariants)
- **CANARY E2E** ↔ a `tests/canary-bm25.nika.yaml` workflow (or documented exemption if the scorer is too pure for an end-to-end canary)
- **PARITY LEGACY** ↔ N/A (brouillon branch v0.79 has no BM25 implementation · CRAFT-from-scratch · no legacy to mirror)
- **REVIEW SWARM** (3 agents · spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer)

## Consumer signal

Per `dx/.claude/rules/steal-pattern.md` §"Step 0 — Consumer-signal gate" (Wave 10 of the DS federation discipline · adopted as a general anti-speculative-port pattern), Tier B/C steal-pattern ports require ≥1 consumer signal before status flips Proposed → Accepted. ADR-038 is an **internal Diamond admission**, not a Tier B/C steal-pattern port, but the same anti-speculative discipline applies ·

- **Locked decision** · the Diamond memory architecture (private DX doctrine + public trait surface at `crates/nika-kernel-ai/src/memory.rs`) lists `nika-bm25` as one of the 8 mandatory L1 memory satellites · ✅ satisfies « locked decision » in §Step 0
- **Scheduled launch** · Phase 1 entry plan §3 schedules `nika-bm25` as W3 admission with ~7h focused effort · ✅ satisfies « scheduled launch »
- **Consumer ticket** · `nika-memory` L2 orchestrator (deferred to W11+) requires both lexical and semantic recall paths · BM25 IS the lexical half · the orchestrator is the consumer-of-record once it ships

## Evidence / Affected code

- `crates/nika-kernel-ai/src/memory.rs:218-223` — `MemoryRecall` trait definition that `nika-bm25` will implement
- `crates/nika-error/src/codes/registry.rs:140-186` — NIKA_600 placeholder + NIKA_601..604 active + NIKA-620..629 reserved for this crate
- W3 admission plan · private SuperNovae DX surface (not engine-bound)
- Commit anchor pre-W3 · `git log --oneline -1` on `main` at admission time will land in the W3 closeout commit body

## Alternatives considered

### Alt A — Wrap `tantivy` (or `meilisearch-sdk`)
Skip the hand-roll · adopt a published Rust search engine. Rejected because :
- Violates ADR-001 (CRAFT not extraction) · we own the math
- `tantivy` is ~500K LOC across the workspace · we want a ~600 LOC primitive
- The orchestrator consumer needs **just BM25 scoring** (not full-text-search infra · query parsers · faceting · etc.)
- Wrapper-of-wrapper guard (steal-pattern.md §"Wrapper-of-wrapper guard") · always port the leaf

### Alt B — Defer `nika-bm25` until `nika-hnsw` admits
Implement HNSW first then BM25. Rejected because :
- HNSW requires an empirical embedding distribution (100k+ real BGE-M3 vectors) we do not have yet
- BM25 has zero dependencies on the embedding model · can land independently
- W5 of the Phase 1 entry plan explicitly DEFERS `nika-hnsw` to a trigger-gated wave (« when ≥100k real vectors exist »)

### Alt C — Admit a placeholder `nika-bm25-stub` first then real impl later
Stub the trait surface · land real algorithm later. Rejected because :
- Stubs are an anti-pattern per ADR-037 §Context (« Stubs that pretend to implement reserved traits accumulate residue »)
- The 12-gate ceremony explicitly forbids stub admission (gate 5 `cargo mutants ≥90%` cannot pass on a `todo!()` impl)
- CRAFT-from-scratch in 7h focused beats stub-then-rewrite in calendar time

## Related

- **ADR-001** — Orphan branch · CRAFT not extraction (rejects Alt A wrapper approach)
- **ADR-003** — 12-gate admission ceremony (governs the W3 admission close)
- **ADR-016** — Cancel-safety discipline (lib functions must be cancel-safe per `MemoryRecall::recall()` contract)
- **ADR-023** — File modularity discipline (≤1500 LOC/file enforced)
- **ADR-037** — Bottom-up Diamond progression (no feature-milestone defer · admit when ready)
- `docs/architecture/crate-layer-registry.md` — L1 row for `nika-bm25` lands in Diamond W2.5
- `docs/architecture/forward-compat-invariants.md` — FCI-001/002/003 patterns this crate respects
- `dx/.claude/rules/no-legacy-no-back-compat.md` — NUKE-LEGACY · BM25 ships as canonical lexical scorer · zero alias / shim / dual-format support
- `dx/.claude/rules/naming-memory-subsystem.md` — Diamond ⊥ Memory orthogonality · `nika-bm25` is a L1 crate organized SELON Diamond methodology
- Robertson & Walker 1994 · « Some simple effective approximations to the 2-Poisson model for probabilistic weighted retrieval »
- Manning · Raghavan · Schütze 2008 · *Introduction to Information Retrieval* ch.11 (fixture source for parity tests)

## Notes

**Promotion gate** · ADR-038 stays `proposed` until ·

1. `crates/nika-bm25/` scaffolded with the 4-file shape
2. 12 gates ✅ per the readiness table above (with W3-pending gates filled in)
3. Workspace `Cargo.toml` `[workspace] members` array includes `crates/nika-bm25`
4. `[workspace.metadata.diamond.layers] nika-bm25 = "L1"` added (Diamond W2.5)
5. `docs/architecture/crate-layer-registry.md` L1 section has the `nika-bm25` row (Diamond W2.5)
6. Single atomic admission commit · message format `feat(nika-bm25): admit to workspace — all 12 gates passed` (per nika/engine commit-granularity.md)

On promotion · status flips to `accepted` · `enables: []` will list nika-rrf and nika-hnsw admission ADRs once they land.

**Out-of-scope for ADR-038** (deferred to future ADRs) ·
- BM25F (per-field weighting) — nice-to-have at v0.2 if a consumer asks
- BM25+ (lower-bound saturation) — same as BM25F · deferred
- Custom tokenizers (CJK · IDF caches · etc.) — orchestrator-driven · post v0.95 Cortex
- Persistent index serialization format — FCI-003 versioning bake-in at W3 · concrete schema in a follow-up ADR
