---
title: "Nika Diamond · Blueprint 2036 · architecture final v0.x"
status: proposal
version: 1.0
date: 2026-05-12
horizon: "10-year build · 2026 → 2036"
deciders: ["@ThibautMelen"]
adr_queue: ["ADR-050", "ADR-051", "ADR-052", "ADR-053", "ADR-054", "ADR-055", "ADR-056"]
companion_adrs: ["ADR-001", "ADR-002", "ADR-004", "ADR-006", "ADR-014", "ADR-038", "ADR-039", "ADR-040", "ADR-041", "ADR-042"]
sources:
  - "context7 /bytecodealliance/wasmtime v38.0.4 (validated 2026-05-12)"
  - "context7 /automerge/automerge AutoCommit + sync protocol (validated 2026-05-12)"
  - "4-agent review swarm 2026-05-12 PM (rust-architect synthesis)"
---

# Nika Diamond · Blueprint final v0.x · 10-year horizon (2026 → 2036)

## §0 · TL;DR · 5 sentences

1. **5 verbs LOCKED forever** — `infer · exec · fetch · invoke · agent` survive 10-yr horizon · new verb candidates (`embed` · `evaluate` · `train` · `serve`) collapse cleanly into existing taxonomy.
2. **42-crate target HOLDS** via aggressive cluster-collapse — `nika-effects` merges 5 L1 effect crates · `nika-verbs` merges 5 verb crates · `init+lints+catalog-verify` collapse to `nika-cli` subcommands.
3. **Memory cluster 1+9** preserves publishable-satellite discipline (per ADR-004 · external open-source value) · only place where fine-grained splits stay justified.
4. **7-ADR queue** for 2026-2030 milestones — WASM Component Model plugin protocol · CRDT federation adapter · edge subset · multi-protocol gateway · 3 cluster-collapses.
5. **Structural locks unchanged** — AGPL forever · forever-v0.x · RDF/SPARQL canonical · `Co-Authored-By: Nika 🦋` · 12-gate admission · zero-unwrap.

## §1 · 5-verb lock validation · 2036 horizon

Per ADR-001 + INV-022 · 5 verbs locked forever. 10-year stress test ·

| Candidate new verb | Collapses into | Why |
|---|---|---|
| `embed` | `infer` | Embedding IS inference (BGE-M3 forward pass · zero new semantic) |
| `evaluate` | `agent` | Evaluation = control loop with judge · not a new primitive |
| `train` | `exec` | Training = process orchestration with GPU sub-process |
| `serve` | `invoke` | Serving = function exposed as endpoint · invocation surface |
| `stream` | `invoke` (or `infer`) | Streaming is transport · not a verb |
| `transform` | `exec` | Pure compute IS exec |

**Verdict** · 5 verbs hold to 2036. Lock confirmed.

## §2 · Refined crate table · BLUEPRINT v0.x final

Aggressive cluster-collapse per ADR-006 monolithic-kernel spirit applied to ALL clusters where external-publishability isn't load-bearing.

| Layer | Crates | Count | Notes |
|---|---|---:|---|
| L0 | `nika-types` · `nika-error` · `nika-catalog` · `nika-catalog-codegen` · `nika-schema` · `nika-event-types` | 6 | Stable · ADR-040 features locked |
| L0.5 | `nika-kernel` · `nika-kernel-mock` | 2 | Monolithic forever per ADR-006 amendment |
| L1 effects | `nika-effects` *(merged 5→1 · ADR-055 queue)* | 1 | Was fs+http+blob+process+clock · single dispatch crate |
| L1 memory | `nika-hnsw` · `nika-bm25` · `nika-rrf` · `nika-fsrs` · `nika-graph-algos` · `nika-rdfs-reasoner` · `nika-temporal` · `nika-autodesc-minimal` ★ · `nika-autodesc-full` ★ | 9 | KEEP split · publishable on crates.io per ADR-004 · external value |
| L2 | `nika-memory` · `nika-runtime` · `nika-verbs` *(merged 5→1)* · `nika-protocols` *(MCP+ACP+...)* · `nika-workflows` · `nika-wasm-host` *(Phase 4+)* | 6 | ADR-053 protocols · ADR-056 verbs · ADR-050 WASM |
| L3 | `nika-runtime-dag` · `nika-binding` | 2 | `nika-policy` = module not crate |
| L4 | `nika-cli` · `nika-tui` · `nika-serve` · `nika-lsp` · `nika-mcp-server` | 5 | `init` · `lints` · `catalog-verify` = subcommands of `nika-cli` |
| L5 | `nika` (binary) | 1 | Composition root <500 LOC per nika-invariants |
| `pck` | first-party packages (3-5) | 3-5 | Phase 4 dyn-trait · Phase 7 WASM Component Model |
| builtin | `nika-builtin-github` · `nika-builtin-cloud` · `nika-builtin-workspace` | 3 | Native API adapters |
| provider | `nika-provider-rig` · `nika-provider-native` · `nika-provider-mock` | 3 | 9-provider catalog dispatch |
| media | `nika-media` *(audit · ADR-054 collapse pending empirical signal)* | 1-5 | Default = single crate · split only if `pck` shows real need |
| Edge | `nika-memory-edge` *(Phase 3+ · feature-gated no_std)* | 1 | DEFERRED |
| **TOTAL ADMITTED** | | **~42** | **ON TARGET** |
| With optional/deferred | | **~52** | Under cap 100 |

## §3 · ADR queue · 7 amendments for 2026-2030 horizon

### ADR-050 · WASM Component Model plugin protocol (Phase 4 → Phase 7)

**Grounded** · context7 `/bytecodealliance/wasmtime` v38.0.4 confirms Component Model + async stable in 2026 · `wasm_component_model(true)` · `bindgen!` macro · `instantiate_async` · WASIp2 production-ready.

Phase 4 (2027+) ships `pck` as dynamic-loaded Rust trait objects · Phase 7 (2029+) introduces WIT-defined ABI alongside · Phase 8 (2030+) Rust-native deprecates in favor of WIT-only. Sovereignty win · any-language `pck` · cost ~2-3 MB binary + ~5 MB wasmtime runtime.

### ADR-051 · CRDT federation adapter (Phase 2+)

**Grounded** · context7 `/automerge/automerge` confirms `AutoCommit` + sync protocol mature 2026 · `generate_sync_message` / `receive_sync_message` · production-ready local-first sync.

`nika-memory-sync` (L2) projects local Oxigraph state into Automerge at federation boundary · preserves « one writer · deterministic » invariant locally · CRDT only at sync seam. Per `supernovae-alignment.md` Rule 1 (memory sovereignty across user devices).

### ADR-052 · Edge subset · `no_std` viability (Phase 3+)

`nika-bm25` + minimal `nika-rdfs-reasoner` are plausible `no_std` (pure compute) · `nika-hnsw` needs `alloc` but feasible · Oxigraph 0.5.6 is `std`-only so embedded path REPLACES with stripped in-memory quad-store. Target ESP32-S3 8MB PSRAM realistic · 4MB tight.

### ADR-053 · Multi-protocol gateway `nika-protocols`

Promote `nika-mcp` → `nika-protocols` · sub-modules per protocol (MCP today · ACP 2027 · future Anthropic/OpenAI protocols). Single crate per ADR-006 spirit · multi-protocol surface · vendor-neutral per `supernovae-alignment.md` Rule 3.

### ADR-054 · Media split audit · empirical reassessment

ADR-042 split `nika-media` into 5 sub-crates (cas · image · pdf · document · provenance). Reassess at first `pck` admission · if no media `pck` ships by Phase 3, COLLAPSE back to single `nika-media` per ADR-006 spirit. Audit trigger condition · same shape as ADR-042 §Admission trigger for autodesc-full.

### ADR-055 · L1 effects collapse `nika-effects`

Merge `fs + http + blob + process + clock` → single `nika-effects` crate. Per ADR-006 amendment (kernel monolithic forever · « shipping 5 effect crates for 5 trait impls » same anti-pattern). Each effect stays a module · trait surface unchanged · feature-gated dispatch.

### ADR-056 · L2 verbs collapse `nika-verbs`

Merge `verb-{infer,exec,fetch,invoke,agent}` → single `nika-verbs` crate · sub-modules per verb. 5-verb lock preserved at semantic level · single dispatcher justifies single crate. Reduces L2 admission ceremony from 5 × 12-gate to 1 × 12-gate.

## §4 · SOTA 2030 angles · new dimensions

### Hot-path SIMD vectorization
`std::simd` portable stable timeline unclear (2030 target). Today · `wide` crate for portable f32×N · platform intrinsics fallback for BGE-M3 cosine distance (1024-dim · cache-line aligned). `hnsw_rs 0.3.2` autovectorization reliability empirical at W7 admission.

### Allocator strategy
Default `std::alloc` for v0.x. `mimalloc` global allocator opt-in via feature flag · bench at W3+ admission. Scoped arenas for recall-fan-out hot path (Phase 2 optimization · not gating Phase 1.5).

### Profile-guided optimization
`cargo-pgo` + `llvm-bolt` at v0.90 emergence gate · realistic perf delta 5-15% on inference hot path. PGO trace via internal benchmark suite · not user telemetry (Rule 1 sovereignty).

### Structured concurrency
`tokio_util::task::TaskTracker` + `CancellationToken` mandatory for `nika-memory` orchestrator satellite fan-out · per `olympus-os-discipline.md` discipline applied to engine. Loom verification per `nika-memory` orchestrator · ADR-013 mandatory crate-by-crate.

### Backpressure
`tokio::sync::mpsc` bounded channels for L2 RRF fusion · bound = top-K × N-rankers · prevents satellite-flood. Specific to ADR-039 streaming MemoryRecall consumer at W4.

### Cryptographic provenance
Ed25519 signing of memory frames (Phase 2+ multi-device sync) · RDF-star reification carries signature triples · `ed25519-dalek` 2.x via `signature` trait abstraction. PQ migration (Dilithium · SPHINCS+) deferred to 2032+ when NIST PQC standardizes Rust ecosystem.

### Supply chain hardening
`cargo-vet` provenance attestations · `cargo-auditable` embed Cargo.lock in binary · sigstore-signed releases · SLSA L3+ at v0.90 emergence. Reproducible builds priority gate-12 invariant.

## §5 · Migration roadmap · Phase 1.5 → Phase 8

```
2026-08-30  Phase 1.5 close — memory cluster admission (1+9) · BGE-M3 + Oxigraph stack
2026-09+    Phase 2 — workflow integration · verbs admission · CRDT prep (ADR-051 stub)
2027+       Phase 3 — pck plugin system v1 (dyn-trait) · edge subset gate (ADR-052)
2027+       Phase 4 — pck v1 stable · WASM Component Model dual-ABI prep (ADR-050)
2028+       Phase 5 — federation ships (ADR-051) · multi-device sync
2029+       Phase 6 — WASM Component Model primary plugin protocol (ADR-050 phase 2)
2030+       Phase 7 — multi-protocol gateway shipped (ADR-053) · MCP + ACP + ...
2032+       Phase 8 — PQ crypto migration · post-quantum signature primitives
2036        10-yr build close — v0.90+ emergence · 42 crates admitted · all 7 shadow zones green
```

Per `time-architecture.md` Layer 1 DECENNIAL · vision immutable in spirit · amendable in detail. Annual review at anniversary 2026-04-XX.

## §6 · Coherence test · 5 raisons

| Raison | Grade | Justification |
|---|:---:|---|
| ① Liberté cognitive | ✅ | WASM Component Model (ADR-050) = any-language `pck` · no Rust lock-in for ecosystem · 100% local-first by default |
| ② Souveraineté | ✅ | CRDT federation (ADR-051) preserves local-first · edge subset (ADR-052) = device-level sovereignty · zero vendor-hosted state |
| ③ Joy 🦋 | ✅ | 42-crate restraint discipline · aggressive cluster-collapse · craft over sprawl · « refined not bloated » |
| ④ Composable galaxy | ✅ | 9 memory satellites publishable standalone · `nika-protocols` multi-protocol · WASM Component plugins · open ecosystem |
| ⑤ Studio signature | ✅ | « collapse aggressively · publish satellites externally · lock 5 verbs forever » = SuperNovae intellectual style locked structural |

## §7 · Anti-fragility · what survives 10-yr stress

Per `time-architecture.md` Layer 2 multi-year forever-v0.x ·
- **AGPL-3.0-or-later** · forces share-alike on hosted forks · survives vendor capture attempts
- **RDF/SPARQL canonical** · W3C standard since 2004 · still standard 2036
- **5 verbs locked** · semantic taxonomy passes 10-yr stress test (§1)
- **12-gate admission** · quality discipline scales with crate count
- **`Co-Authored-By: Nika 🦋`** · studio signature non-replicable

What MIGHT break ·
- Specific Rust crates (hnsw_rs · fastembed · Oxigraph) versions evolve · trait abstraction insulates
- WASM Component Model API surface · Phase 4+ migration is the bet
- MCP/ACP protocol churn · `nika-protocols` multi-protocol layer absorbs

## §8 · Verification anchors

Per `socratic-research-discipline.md` Rule 6 · grounded claims ·

- WASM Component Model + async stable · context7 `/bytecodealliance/wasmtime` v38.0.4 · `wasm_component_model(true)` · `bindgen!` macro · `instantiate_async` · WASIp2 (2026-05-12 query)
- Automerge sync protocol mature · context7 `/automerge/automerge` · `AutoCommit` + `generate_sync_message` / `receive_sync_message` (2026-05-12 query)
- Memory subsystem stack · `naming-memory-subsystem.md` v2.4 + ADR-004 + ADR-005 + ADR-038-042 (this engine repo)
- 5-verb lock · ADR-001 + `nika-invariants.md` (this engine repo)
- 40-42 crate cap · ADR-006 amendment + ROADMAP.md (this engine repo)

## §9 · This blueprint's status

**Proposal-grade** · NOT lock-grade. Per `socratic-research-discipline.md` Step 4 + `cross-source-validation.md` 3-bucket triage · 7 ADRs (050-056) queue for individual ratification at admission gates. Annual decennial review at anniversary (2027-04 onwards) re-validates against actual ecosystem state.

## §10 · Related

- ADR-001 — orphan branch CRAFT method
- ADR-002 — forever v0.x
- ADR-004 — context-window sized crates
- ADR-006 — kernel ISP traits + monolithic amendment
- ADR-014 — sealed kernel traits
- ADR-038 — nika-bm25 admission
- ADR-039 — streaming MemoryRecall
- ADR-040 — Cargo feature matrix
- ADR-041 — type-state orchestrator
- ADR-042 — autodesc MINIMAL/FULL split
- ROADMAP.md — bottom-up plan + ADR-037 + tag scheme
- DIAMOND.md — strategy overview + 7 shadow zones
