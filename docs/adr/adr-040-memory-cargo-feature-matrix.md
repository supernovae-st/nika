---
id: ADR-040
title: "Cargo feature matrix for nika-memory · zero-cost modularity"
status: proposed
date: "2026-05-12"
phase: "Phase 1.5 prep (Diamond memory subsystem)"
deciders: ["@ThibautMelen"]
tags: ["memory", "cargo-features", "modularity", "zero-cost", "sovereignty"]
affects_crates: ["nika-memory", "nika-bm25", "nika-rrf", "nika-fsrs", "nika-temporal", "nika-hnsw", "nika-rdfs-reasoner", "nika-graph-algos", "nika-autodesc"]
affects_layers: ["L1", "L2"]
supersedes: []
superseded_by: []
related: ["ADR-004", "ADR-005", "ADR-006", "ADR-007", "ADR-014", "ADR-038", "ADR-039", "ADR-041", "ADR-042", "ADR-078", "ADR-079"]
requires: ["ADR-004", "ADR-039"]
enables: ["ADR-041", "ADR-042"]
fci: ["FCI-003", "FCI-006"]
inv: ["INV-019"]
shadow_zones: []
nika_codes: []
timeline: "Phase 1.5 → 2026-08-30 (lands incrementally W3-W10)"
follow_ups:
  - "ADR-041 type-state orchestrator (Building → Ready → Querying)"
  - "ADR-042 autodesc-MINIMAL/FULL split formalization"
---

# ADR-040 — Cargo feature matrix for `nika-memory` · zero-cost modularity

## Context

Diamond memory subsystem ships 1 L2 orchestrator + 8 L1 satellites per ADR-004.
Consumer profiles diverge wildly :

- **CLI tool** (lexical search only) — wants `bm25 + rrf + fsrs` · zero ML deps · ~5MB binary
- **Editor extension** — wants `bm25 + hnsw` semantic recall · BGE-M3 vendored ONNX · ~80MB
- **Research lab** — wants `full` with autodesc + reasoner + graph-algos
- **Olympus MCP bridge** — wants minimal `bm25 + rrf` (read-only catalog) · NOT the embedding/RDF stack

Without feature gating, every consumer pays for the maximal stack (fastembed-rs ONNX
runtime ~30MB · BGE-M3 ONNX weights ~50MB · `hnsw_rs` C-FFI · `oxigraph` RocksDB
backend ~15MB). That's a maintenance + bundle-size violation of `supernovae-alignment.md`
Rule 3 (vendor-neutral default · users pick).

Bucket-C lock G2=B+D 2026-05-12 explicitly added `feature = "llm-enrich"` for
M4/M5/M11 async LLM-augmented mechanisms · this ADR generalizes the pattern to
ALL satellites + runtime backends.

## Decision

`nika-memory` ships a 14-feature matrix · default = deterministic-zero-ML core ·
every heavy dep behind opt-in feature gate.

### `[features]` block (proposed)

```toml
# nika-memory/Cargo.toml
[features]
default = ["bm25", "rrf", "fsrs", "temporal"]

# L1 satellites · pure-algo · zero heavy deps · ~600 LOC each
bm25 = ["dep:nika-bm25"]
rrf = ["dep:nika-rrf"]
fsrs = ["dep:nika-fsrs"]
temporal = ["dep:nika-temporal"]

# L1 satellites · heavy deps · opt-in
hnsw = ["dep:nika-hnsw", "dep:fastembed"]   # consumer MUST pick ml-runtime-{onnx|candle}
graph-algos = ["dep:nika-graph-algos"]
rdfs-reasoner = ["dep:nika-rdfs-reasoner"]

# Embedding runtime · mutually-exclusive choice (consumer picks ONE · compile_error gate below)
ml-runtime-onnx = ["fastembed/ort"]        # Path A · prod default · cross-platform
                                            #   UNVERIFIED · validate `fastembed` v5 Cargo.toml exposes `ort`
                                            #   feature at W7 hnsw admission · queue empirical check
ml-runtime-candle = ["dep:candle-core",     # Path B · pure-Rust · Metal/CUDA
                     "dep:candle-transformers"]

# Storage backend · Oxigraph RDF-star (sovereignty structural-lock · Rule 5)
rdf-star = ["oxigraph/rdf-12", "oxigraph/sparql-12"]

# Mechanisms · M4/M5/M11 LLM-augmented opt-in (G2 lock 2026-05-12)
llm-enrich = ["dep:nika-verb-infer"]       # M4 prospective · M5 narrative · M11 auto-link
                                            # errors propagate via nika-verb-infer NIKA-5xx range · no new range

# Autodesc bundles (G3 lock 2026-05-12 · per ADR-042 1+9 split)
autodesc-minimal = ["dep:nika-autodesc-minimal", "rdfs-reasoner", "temporal"]  # W4 · provenance+punning
autodesc-full = ["dep:nika-autodesc-full", "autodesc-minimal", "graph-algos"]  # Phase 2 · + summarization

# Convenience bundle · everything
full = [
    "bm25", "rrf", "fsrs", "temporal",
    "hnsw", "ml-runtime-candle",
    "rdf-star", "rdfs-reasoner", "graph-algos",
    "llm-enrich", "autodesc-full"
]
```

### Feature DAG (zero-conflict)

```
default (4 deterministic)
   │
   ├── bm25 · rrf · fsrs · temporal        [pure-algo · no deps]
   │
   ├── ml-runtime { onnx ⊕ candle }         [mutually-exclusive · marker]
   │
   ├── hnsw ───── requires ml-runtime
   ├── rdf-star ── independent (oxigraph feature flags)
   ├── llm-enrich ─ independent (peer dep nika-verb-infer)
   │
   ├── autodesc-minimal ─── rdfs-reasoner + temporal
   ├── autodesc-full ────── autodesc-minimal + graph-algos
   │
   └── full ─────── union of all
```

### Compile-error guards

```rust
// nika-memory/src/lib.rs
#[cfg(all(feature = "ml-runtime-onnx", feature = "ml-runtime-candle"))]
compile_error!("Pick ONE ml-runtime backend · onnx OR candle · not both");

#[cfg(all(feature = "hnsw", not(any(feature = "ml-runtime-onnx", feature = "ml-runtime-candle"))))]
compile_error!("Feature `hnsw` requires `ml-runtime-onnx` or `ml-runtime-candle`");

#[cfg(all(feature = "autodesc-full", not(feature = "graph-algos")))]
compile_error!("Feature `autodesc-full` requires `graph-algos`");
```

## Consequences

### Positive

- **CLI consumer (lexical-only) ships ~5MB** instead of ~150MB · default features only.
- **Sovereignty preserved** (Rule 5) · `rdf-star` opt-in but DEFAULT-ON for memory consumers via `autodesc-minimal`. Vendor-neutral runtime choice (Path A/B).
- **Sovereignty structural** · `llm-enrich = []` by default · users who want pure-deterministic compile without LLM-augmentation get it by construction · not discipline.
- **Test matrix tractable** · CI runs `default` + `full` + 3 representative subsets (no combinatorial explosion).
- **L1 satellites publishable independently on crates.io** · each is `nika-<satellite>` with its own zero-dep core + optional features.

### Negative

- **Documentation burden** · README must explain feature matrix · cargo doc per feature combo. Mitigated by `[package.metadata.docs.rs] all-features = true`.
- **Maintenance** · adding a 9th satellite means adding a feature row · cargo-deny + hygiene vector to enforce feature presence in `Cargo.toml`. Acceptable.
- **CI cost** · ~3 feature combos × test matrix · ~3× CI time vs single-feature. Acceptable for Phase 1.5 scope.

### Neutral

- Forward-compat per ADR-007 · adding a new feature is additive (existing consumers unaffected). Removing one is a breaking change — a MINOR bump pre-1.0, a MAJOR bump post-1.0 (ADR-002 · amended D-2026-06-20-N1 · real semver toward 1.0) · SemVer-strict on features per `cargo public-api`.

## Evidence

- `nika/engine/docs/adr/adr-004-phase-1-option-b-lock.md` — 1+8 architecture
- `nika/engine/docs/adr/adr-005-phase-0-exit-anchor.md` — BGE-M3 + Oxigraph 0.5.6 stack lock
- `nika/engine/docs/adr/adr-038-nika-bm25-admission.md` — W3 admission · feature `bm25 = []` precedent
- Internal Bucket-C lock 2026-05-12 (private audit) — G2 + G3 source decisions

## Alternatives considered

### Alt A — No features · ship everything always
Rejected · violates Rule 3 (force users to pay for stack they don't want) · bundle-size explosion · contradicts atelier-vs-produit moat positioning (Nika = lean · composable).

### Alt B — Per-satellite separate crates · no orchestrator features
Rejected · orchestrator still needs to gate which satellites it composes · features land at orchestrator level not just satellite level. Satellites stay independently publishable (current ADR-004 plan).

### Alt C — Runtime selection via builder pattern (not compile-time)
Rejected · runtime selection means dragging ALL deps into binary even if unused · defeats the bundle-size objective · violates zero-cost-abstraction principle.

## Related

- ADR-004 — 1+8 architecture (this ADR refines)
- ADR-005 — Phase 0 exit (BGE-M3 + Oxigraph stack)
- ADR-006 — kernel ISP traits (atomic trait + blanket pattern)
- ADR-007 — forward-compat invariants (feature additions are additive)
- ADR-014 — sealed kernel traits (feature gates respect sealed pattern)
- ADR-038 — nika-bm25 admission (precedent for satellite features)
- ADR-039 (queued) — streaming `MemoryRecall::recall_stream()` for lazy RRF fusion
- ADR-041 (queued) — type-state orchestrator (Building → Ready → Querying)
- ADR-042 (queued) — autodesc-MINIMAL / FULL split formalization

## Notes

Feature matrix lands incrementally · each satellite admission (W3-W10) adds its
own feature row in this matrix. ADR-040 ships the SHAPE · individual satellites
ship the IMPL. Gate 12 admission for each satellite includes « own feature
declared in `nika-memory/Cargo.toml` `[features]` block » as sub-gate.

Companion · ADR-041 (type-state orchestrator · compile-time `NikaStore<Building>`
→ `NikaStore<Ready>` for satellite-set enforcement at compile time) ratifies
the runtime-state-to-type-state migration per Rust 2026 idioms (per Perplexity
2026-05-12 research synthesis · MindPalace 7-layer pattern + zero-cost newtypes).
