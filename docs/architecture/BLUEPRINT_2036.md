---
title: "Nika Diamond · Blueprint 2036 · architecture final v0.x"
status: proposal
version: 1.2
date: 2026-05-12
horizon: "10-year build · 2026 → 2036"
deciders: ["@ThibautMelen"]
adr_queue_v1_1: ["ADR-050", "ADR-051", "ADR-052", "ADR-053", "ADR-054", "ADR-055", "ADR-056"]
adr_queue_v1_2: ["ADR-060", "ADR-062", "ADR-064", "ADR-066"]
adr_prose_folded: ["ADR-061-into-066", "ADR-063-mimalloc", "ADR-065-signer", "ADR-067-zerocopy", "ADR-068-hakari"]
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

## §1.5 · Collapse-vs-publish principle (locked v1.1)

The blueprint's aggressive cluster-collapse applies a single decision rule (now ratified in `nika-invariants.md` §Collapse-vs-publish) ·

```
Is there GENUINE EXTERNAL VALUE to per-crate granularity?
├── YES → preserve split (memory satellites · provider variants · pck packages)
│         · publishable standalone on crates.io with own semver
│         · external consumers benefit from isolated dep
└── NO  → collapse to one crate with modules (effects · verbs · cli subcommands)
          · 12-gate ceremony × N crates > module discipline within 1 crate
          · per ADR-006 monolithic-kernel-spirit
```

Application matrix ·

| Cluster | Decision | External value? | Why |
|---|---|:-:|---|
| Memory satellites (9) | SPLIT | ✅ | Rust RDF/ML ecosystem · `nika-bm25` · `nika-hnsw` standalone-useful on crates.io |
| Provider variants (3) | SPLIT | ✅ | Heavy-dep isolation (rig vs native mistral.rs vs mock) · users pick 1 |
| pck packages (3-5) | SPLIT | ✅ | Plugin protocol Phase 4+ · external authors ship pcks |
| Effect impls (5→1) | COLLAPSE | ❌ | Internal trait impls · no external consumer asks for `nika-fs` alone |
| Verb executors (5→1) | COLLAPSE | ❌ | Shared dispatcher · 5-verb lock semantic (preserved at module level) |
| L4 tools (init·lints·catalog-verify) | COLLAPSE | ❌ | Subcommands of `nika-cli` · not independent binaries |

This principle is the structural enforcement (per `supernovae-alignment.md` Rule 5) that distinguishes Diamond from competitor architectures (Restate's 3-service-types · LangGraph's monolithic graph · Temporal's history-heavy SDK).

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

## §2.5 · Per-crate detail (v1.1 enrichment)

Each row · Purpose (1 line) · LOC budget · Key deps · Trait surface · Gate 9 canary stance · Admission target.

### L0 · primitives (6 crates · ~30k LOC total)

- `nika-types` · shared types · 2k LOC · `serde` `chrono` · no public trait (types only) · Gate 9 EXEMPT (pure types) · ADMITTED v0.81
- `nika-error` · error taxonomy `NikaError` enum + NIKA-XXX codes · 3k LOC · `thiserror` `miette` (opt-in) · sealed `NikaError` variant enum · Gate 9 EXEMPT · ADMITTED v0.81
- `nika-catalog` · capability vocabulary (105 MCP · 32 LLM providers · 13 embeddings) · 8-10k LOC · `nika-types` · `Catalog` query API + `ModelPricing` 7-axis · Gate 9 via canary-catalog.yaml fixtures · ADMITTED v0.81
- `nika-catalog-codegen` · build-time codegen of catalog rust types · 2k LOC · `nika-catalog` `quote` · proc-macro (no runtime trait) · Gate 9 EXEMPT (build-tool) · ADMITTED
- `nika-schema` · workflow YAML parser `apiVersion: nika.sh/v1` · 4-6k LOC · `serde_yaml` `nika-types` · `Workflow` parsed struct · Gate 9 via canary-workflows · WIP (W2 target)
- `nika-event-types` · canonical event envelope shape (cross-product · per `olympus-platform-canonical.md`) · 1k LOC · `nika-types` · `Event` `EventKind` enum sealed · Gate 9 EXEMPT · PLANNED L0

### L0.5 · kernel (2 crates · monolithic forever per ADR-006)

- `nika-kernel` · ~20 atomic ISP traits + InferRequest/Response + memory traits + capability tokens · ~5-7k LOC · `async-trait` (`trait_variant`) · sealed traits per ADR-014 · Gate 9 EXEMPT (abstract) · ADMITTED
- `nika-kernel-mock` · test mock impl · 4k LOC · `nika-kernel` · `MockProvider` `MockMemoryStore` etc · Gate 9 via 88 lib tests · ADMITTED

### L1 effects (1 collapsed crate · ADR-055 queue · ~6-8k LOC)

- `nika-effects` · `fs+http+blob+process+clock` impls of kernel L1 traits · modules per axis · 6-8k LOC · `reqwest` (vendored TLS) · `tokio` · `walkdir` · per-module trait impls · Gate 9 via real-fs/http canary fixtures · post-W3 admission

### L1 memory satellites (9 crates · ADR-042 split · publishable standalone)

- `nika-bm25` · lexical BM25 scoring · 600-800 LOC · `nika-kernel` only · `MemoryRecall` impl · Gate 9 EXEMPT (pure-algo) · **W3 target ADR-038**
- `nika-rrf` · Reciprocal Rank Fusion · ~200 LOC · `nika-kernel` · `MemoryRecall` (composing) · Gate 9 EXEMPT · W4 target ADR-039
- `nika-fsrs` · FSRS-6 forgetting scheduler · 400 LOC · `nika-kernel` · `MemoryLifecycle` (consolidate/prune) · Gate 9 EXEMPT · W5 target
- `nika-temporal` · valid-time + transaction-time annotations · 300 LOC · `nika-kernel` `chrono` · `MemoryTemporal` trait · Gate 9 EXEMPT · W6 target
- `nika-hnsw` · approx vector index wrap of `hnsw_rs 0.3.x` · 800-1200 LOC · `nika-kernel` `hnsw_rs` (unsafe encapsulated) · `MemoryRecall` (semantic) · Gate 9 via 100k vector canary (TRIGGER-GATED per ADR-005) · W7 target
- `nika-rdfs-reasoner` · RDFS-13 forward chaining + OWL 2 punning · 1000 LOC · `nika-kernel` `oxigraph` · `RdfsInfer` trait · Gate 9 via owl-test-suite canary · W8 target
- `nika-graph-algos` · Louvain · centrality · shortest-path · communities · 1500 LOC · `nika-kernel` `petgraph` (or maison) · `GraphAlgo` trait · Gate 9 via SNAP datasets canary · W8 target (parallel)
- `nika-autodesc-minimal` ★ · provenance-attached ingest + RDFS-OWL2 punning + SPARQL-star query · 300 LOC · `nika-kernel` `oxigraph` (rdf-star feature) · `Autodesc` trait minimal · Gate 9 via provenance-chain canary · **W4 target ADR-042 (moat foundation)**
- `nika-autodesc-full` ★ · adds schema evolution + graph summarization · 300 LOC delta · `nika-autodesc-minimal` `nika-graph-algos` · `Autodesc` trait full · Gate 9 via summarization canary · Phase 2 (3-condition trigger per ADR-042)

### L2 (6 crates · ~25k LOC)

- `nika-memory` · L2 orchestrator · NikaStore type-state · 9 satellites composed via `Arc<dyn MemoryRecall>` · 5-7k LOC · `nika-kernel` `tokio_util::task::TaskTracker` `tokio::sync::mpsc` · `MemoryStore` blanket + `MemoryLifecycle` · Gate 9 via end-to-end recall canary · W10 target ADR-041
- `nika-runtime` · core runtime + DAG executor · 8-12k LOC · `nika-kernel` `tokio` `petgraph` · `Runtime` trait · Gate 9 via 50 representative workflow canaries · post-W10
- `nika-verbs` · 5 verb executors (`infer · exec · fetch · invoke · agent`) collapsed · modules per verb · 8-12k LOC · `nika-kernel` `nika-effects` `nika-runtime` · `VerbExecutor` trait (sub-trait per verb) · Gate 9 via 5 verb canaries · ADR-056 queue · post-W10
- `nika-protocols` · multi-protocol gateway (MCP · ACP · future) · modules per protocol · 4-6k LOC · `nika-kernel` `tower` `serde_json` · `Protocol` trait · Gate 9 via mcp-conformance canary · ADR-053 queue · Phase 2
- `nika-workflows` · YAML workflow engine consumed by Olympus + CLI · 3-5k LOC · `nika-schema` `nika-runtime` · `WorkflowEngine` trait · Gate 9 via golden-workflows · Phase 2
- `nika-wasm-host` · WASM Component Model plugin host · 3-5k LOC · `wasmtime v38+` `wasmtime-wasi` (WASIp2) · `WasmPluginHost` trait · Gate 9 via component-conformance · ADR-050 queue · Phase 4+

### L3 (2 crates · ~8k LOC)

- `nika-runtime-dag` · DAG execution layer + dispatcher · 5-7k LOC · `nika-runtime` `nika-verbs` · `DagExecutor` trait · Gate 9 via 100-node DAG canary · post-W10
- `nika-binding` · capability/policy binding layer · 3k LOC · `nika-kernel` (policy module · NOT separate crate per `nika-invariants.md`) · `Binding` trait · Gate 9 via policy-fixture canary · post-W10

### L4 interfaces (5 crates · ~25k LOC)

- `nika-cli` · main CLI · subcommands `init` · `lints` · `catalog-verify` · `run` · `inspect` · 12-15k LOC · `clap` (latest) `nika-runtime-dag` `nika-workflows` · binary not trait · Gate 9 via cli-snapshot tests · post-W10
- `nika-tui` · terminal UI · 3-5k LOC · `ratatui` `nika-runtime` · no public trait · Gate 9 via tui-fixture · Phase 3
- `nika-serve` · HTTP/gRPC daemon · 4-6k LOC · `tower` `tonic` `nika-runtime-dag` · binary · Gate 9 via http-canary · Phase 2
- `nika-lsp` · LSP server for workflow YAML · 3-5k LOC · `tower-lsp` `nika-schema` · binary · Gate 9 via lsp-snapshot · Phase 3
- `nika-mcp-server` · expose Nika AS MCP server · 2-3k LOC · `nika-protocols` (MCP module) · binary · Gate 9 via mcp-client canary · Phase 2

### L5 (1 binary · <500 LOC)

- `nika` · composition root binary · <500 LOC · all L4 deps · no trait · Gate 9 via release-canary · v0.90 emergence

### Optional · pck + builtin + provider + media + edge (10-14 crates)

- `nika-pck-{manifest,registry,store,git}` + `nika-pck` orchestrator · 5 crates · ~10k LOC total · Phase 4+
- `nika-builtin-{github,cloud,workspace}` · 3 crates · ~4k LOC total · native API adapters · Phase 3
- `nika-provider-{rig,native,mock}` · 3 crates · heavy-dep isolation · ~6k LOC total · post-W10
- `nika-media` · default 1 crate · audit at first pck need (ADR-054 queue) · ~3k LOC · Phase 3
- `nika-memory-edge` · no_std subset · 1k LOC · feature-gated · ADR-052 queue · Phase 3+
- `nika-memory-sync` · CRDT federation adapter · 2-3k LOC · ADR-051 queue · Phase 2+

**Total LOC budget** · ~42 admitted × ~10k average = ~420k LOC · headroom under 100-crate cap × 15k = 1.5M LOC.

## §2.7 · Best enemies · architectural differentiation (v1.1)

Closest 2026 competitors in workflow-engine + agent-memory + agent-orchestration space ·

### Restate (Rust SDK · context7-grounded)

Per context7 `/restatedev/sdk-rust` · 3 service abstractions ·
1. **Services** (stateless handlers · `#[restate_sdk::service]` macro)
2. **Virtual Objects** (stateful + isolated K/V state + single-writer · `#[restate_sdk::object]`)
3. **Workflows** (durable step-by-step + exactly-once · `#[restate_sdk::workflow]` · `ctx.run()` for durable side-effects · `ctx.promise()` for durable promises)

**Architectural lessons** · macro-driven trait generation · `Context<'_>` parameter for all handlers · automatic journaling + transparent retry + suspension. **Diamond differentiation** · Nika ships **5 verbs locked forever** instead of 3 service-types open · YAML workflow envelope instead of macro-driven Rust trait · AGPL instead of BSL.

### LangGraph (Python · context7-grounded)

Per context7 `/langchain-ai/langgraph` v1.0.8 · graph-based stateful agents · durable execution · streaming + human-in-the-loop. Core abstraction · `StateGraph` with nodes (functions) + edges (transitions) + persistent state per thread.

**Diamond differentiation** · Nika does NOT model agents as graphs (graphs are an EMERGENT property of `agent` verb composing other verbs). Workflow DAG is explicit · per-node state is via `nika-memory` (Diamond memory subsystem) · NOT per-graph-thread-local.

### Temporal (Go · industry baseline)

Workflow history + worker pull-model + deterministic replay. Architectural giant in Go ecosystem. **Diamond differentiation** · Nika is **Rust-first** · embedded single-binary (no separate worker process needed for v0.x) · workflow envelope is YAML (auditable forever per `supernovae-alignment.md` Rule 5).

### Mem0 + Letta + Zep (Python agent memory)

Production agent memory · Python-centric · cloud-hosted SaaS paths · graph + vector hybrid · 7-13 cognitive mechanisms. **Diamond differentiation** · `nika-memory` is **Rust embedded** · **AGPL local-first** · **RDF-star native via Oxigraph** (W3C standard) · **9 satellites publishable standalone on crates.io** · **12 cognitive mechanisms** with 3 LLM-enrich opt-in via Cargo feature.

### Differentiation matrix

| Dimension | Restate | LangGraph | Temporal | Mem0/Letta | **Nika Diamond** |
|---|---|---|---|---|---|
| Language | Java+Rust SDK | Python | Go | Python | **Rust** |
| Durability | ✓ journal | ✓ checkpoint | ✓ history | ✗ | ✓ YAML workflow |
| Memory subsystem | ✗ | partial | ✗ | ✓ | ✓ 1+9 satellites |
| RDF/SPARQL | ✗ | ✗ | ✗ | ✗ | **✓ Oxigraph 0.5.6** |
| Verbs locked | ✓ 3 service types | ✗ open graph | ✓ workflow-only | ✗ | **✓ 5 forever** |
| License | BSL | MIT | Apache | Apache | **AGPL-3.0-or-later** |
| Local-first | ✗ | ✗ | ✗ | ✗ partial | **✓ Rule 1 structural** |
| WASM Component plugins | ✗ | ✗ | ✗ | ✗ | **✓ ADR-050 Phase 4+** |
| Crates publishable | partial | NA | NA | NA | **✓ 9 satellites + provider variants** |

**Conclusion** · Diamond's unique structural moat is **the 4-axis combo none ships together** · Rust embedded + AGPL forever + RDF/SPARQL native + 5-verb locked taxonomy + local-first by default + 9 publishable memory satellites. Per « unified Rust runtime contract » framing of `naming-memory-subsystem.md` v2.3.

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

**RRF fairness amendment** (per rust-async-expert audit 2026-05-12) · `tokio::select!`
**non-biased default** is correct for RRF top-K · biased order skews fusion toward
first-listed satellite contradicting RRF's rank-fusion invariant. `biased;` keyword
ONLY on top-K cutoff guard (fast-path drain) NOT on N-ranker exhaustion polling. Note
· `select_biased!` is `futures::select_biased!` NOT Tokio · Tokio uses `select! { biased; ... }`.

**Loom scope amendment** · per ADR-013 « Loom mandatory crate-by-crate » · full
9-satellite DAG (~10⁶ interleavings × 3-thread) exceeds Loom's reach. **Minimal model
at W3-W9 admission** · 2-thread × 2-recall-fanout + 1 cancel signal captures
cancel-during-fanout + partial-completion merge race. For full-DAG verification at
v0.95 emergence · **Shuttle PCT** (Probabilistic Concurrency Testing) per ADR-013
follow-up · not Loom.

**Cooperative scheduling** (per Tokio 1.40+ stable `consume_budget`) · pure-CPU
satellites (`nika-bm25` scoring · `nika-hnsw` k-NN walk · 100k+ docs) MUST call
`tokio::task::consume_budget().await` every N iterations (N ~ 64-256 · bench-driven
at W3+ admission). Trust-the-runtime fails on tight CPU loops with no `.await`.

### Release profile · LTO + codegen-units=1 (SHIPPED 2026-05-12)

Per rust-perf audit · `Cargo.toml [profile.release]` now ships ·

```toml
lto             = "fat"              # whole-program LTO · ~5-10% on hot loops
codegen-units   = 1                  # required for LTO=fat to fuse
strip           = "symbols"          # ADR-061 byte-determinism target
panic           = "unwind"           # cargo test panic-catch · security.md compat
debug           = "line-tables-only" # cargo flamegraph without bloat
incremental     = false              # release builds never benefit
```

Cost · 2× build time at release · dev builds unaffected. Matches ADR-061 SLSA L3
prep · 5-10% perf delta on BGE-M3 cosine + BM25 + RRF hot paths.

### `const fn` exhaustion · partial shipment (2026-05-12)

Per Rust 1.91 stable `const fn` expansion · 4 safe promotions shipped to
`nika-types::Cost` + `nika-types::Trust` (`new` · `zero` · `is_zero` · `is_at_least`).
`From` trait + `Option::map` blocked (not const-stable yet · 2027+ horizon) · those
constructors stay `pub fn`. Forward-compat per ADR-007 · `pub fn → pub const fn`
is non-breaking · ratchet as const-traits stabilize.

### Cryptographic provenance
Ed25519 signing of memory frames (Phase 2+ multi-device sync) · RDF-star reification carries signature triples · `ed25519-dalek` 2.x via `signature` trait abstraction. PQ migration (Dilithium · SPHINCS+) deferred to 2032+ when NIST PQC standardizes Rust ecosystem.

### Supply chain hardening
`cargo-vet` provenance attestations · `cargo-auditable` embed Cargo.lock in binary · sigstore-signed releases · SLSA L3+ at v0.90 emergence. Reproducible builds priority gate-12 invariant.

## §4.5 · 11/10 amplifiers · « projet Rust le mieux codé au monde 2030 » (v1.2)

Per rust-architect 10-dimension SOTA audit 2026-05-12 PM · 9 new ADRs queued
(ADR-060..068) · NO speculative · all trigger-gated. 4 amplifiers combine into
a singleton 2030 architectural moat ·

### A1 · Kani-verified `nika-kernel` sealed traits (ADR-060)
Formal model-checking of memory-safety + panic-freedom + arithmetic-overflow
on the L0.5 boundary every `pck` extension crosses. AWS uses Kani on `std`
precedent. Zero workflow engine ships this. Trigger · `model-checking/kani`
empirical re-verification at L0.5 stabilization. Effort ~1 week. Creusot /
Prusti functional-spec maturity gap · DEFER 2030+.

### A2 · SLSA L3 + cargo-vet + sigstore + cargo-auditable (ADR-061)
Every release binary carries embedded SBOM + signed provenance + reproducible
bit-exact rebuild. Tokio + uv (Astral) precedent confirmed 2026. Trigger ·
v0.85 release pipeline. Already-shipped · `cargo-deny` config (per `deny.toml`).
Missing · `cargo-vet` + `cargo-auditable` + sigstore + SLSA L3 GitHub Actions
provenance + `[profile.release] strip = "symbols"` byte-determinism.

### A3 · PGO + BOLT at v0.90 (ADR-062)
`cargo-pgo` (Kobzol · rust-compiler-team contributor) + LLVM BOLT post-link =
10-25% perf delta on inference hot path (BGE-M3 cosine · BM25 scoring · RRF
fusion). Profile via internal `nika-bench` corpus · NEVER user telemetry per
`supernovae-alignment.md` Rule 1. Cost = bench-harness + 2× build time.

### A4 · AOT-compiled `pck` packages (ADR-064 · post-ADR-050)
`wasmtime::Module::serialize` pre-warmed plugin runtime · sub-millisecond
cold-start · WASIp2 + Component Model + future Preview 3 (Bytecode Alliance
roadmap 2026-2027). Combined with A1 Kani-verified kernel boundary = **provably-
safe sandboxed-extension architecture** none of Restate/LangGraph/Temporal/Mem0
ships.

### Prose-folded items (NOT separate ADRs · per take-a-step-back audit 2026-05-12)

Per rust-architect coherence audit 2026-05-12 + `socratic-research-discipline.md`
Step 5 Option D · the following are TRIGGER-GATED practices · NOT decision-bearing
ADRs · they live in BLUEPRINT prose only · zero separate ADR file ·

- **mimalloc-rust opt-in** · `--features mimalloc-global` on `nika` binary · `microsoft/mimalloc` 2.5× jemalloc multi-thread per `/microsoft/mimalloc` context7 75 score · trigger · bench at W3+ post-bm25 admission · config-knob not architecture
- **`signature::Signer<T>` abstraction** for Ed25519 provenance · `ed25519-dalek 2.x` per `signature` trait · PQ migration (ML-DSA/Dilithium · SLH-DSA/SPHINCS+) DEFER 2032+ · library-choice not architecture
- **zerocopy 0.8 for binary hot paths** · `nika-hnsw` vector ser + `nika-event-types` framing + Oxigraph embedding literals · per-crate impl choice · NOT YAML
- **cargo-hakari workspace-hack** · evaluate at 20-crate milestone (~v0.85) · uv 50+ crates precedent · trigger-gated · zero decision today
- **MIRI expansion** (post-W3 quality ratchet) · extend to `nika-kernel` + `nika-types` once W3 stabilizes · today `nika-error` + `nika-catalog` run continue-on-error · ratchet not gating

Per LOCK-031 spirit · « no infra behind locked gate » · 5 items above stay
prose-only · save ~5 empty ADR shells.

### Real architectural ADRs (NOT folded · queue 060 · 062 · 064 · 066)

- **ADR-060 · Kani-verified kernel** · which verifier + which boundary · ADR-grade decision
- **ADR-062 · PGO + BOLT release pipeline** · profile corpus + build infra decision · ADR-grade
- **ADR-064 · AOT-compiled `pck`** · `wasmtime::Module::serialize` · post-ADR-050 dependency · ADR-grade · DEFER until ADR-050 ships
- **ADR-066 · OTLP-neutral `ObservabilitySink`** · L0.5 trait surface decision · absorbs ADR-061 SLSA L3 + cargo-vet + sigstore + cargo-auditable as §release-pipeline subsection · ADR-grade · MUST encode `#[tracing::instrument(skip(self, query), level = "debug")]` discipline (orchestrator-level signal · NOT inner hot loops · cost ~100-500ns/span creation untenable on 1M+/sec BGE-M3 cosine calls)
- **ADR-070 · `nika-memory` orchestrator fan-out contract** (NEW · per rust-async audit) · `tokio_util::task::TaskTracker::spawn(token.child_token().run_until_cancelled(fut))` pattern · child-token tree gives per-satellite cancellation without killing siblings · kernel `MemoryRecall::recall` signature stays `(query)` only (NO cancel param · ADR-016 Alt-A explicit) · cooperative `CancelCtx::is_cancelled()` poll happens INSIDE satellites between hot-loop iterations · L2 only · zero kernel pollution

### ADR-041 type-state amendment (queued · per rust-async audit)

`NikaStore::with_recall` · `with_lifecycle` · `build` MUST carry `#[track_caller]`
(Rust 1.46+ stable) · zero runtime cost · panic site shows USER's `.build()` call
location not `nika-memory` internals · canonical 2026 builder idiom.

Net · 9 amplifier candidates → 3 real + 1 deferred (4 ADRs to author) instead of 9 · 55% reduction · « rien de nouveau · améliore et stabilise ».

### The combinatorial moat 2030

Stacking · formal verification (A1) × reproducible-signed releases (A2) ×
PGO-optimized hot path (A3) × AOT-WASM-Component plugins (A4) × 9 publishable
memory satellites (ADR-004) × 5 verbs locked forever (ADR-001) × AGPL-3.0-or-
later = **singleton in 2030**. Nothing else combines this stack.

## §4.7 · Guardian AI · anti-Palantir · Nika + Olympus integration (v1.2)

Per `studio/council/research/2026-04-29-ai-2027-trajectory.md` empirical
research + ai-2027.com modal-year scenario (modal 2027 · median 2029-2032 ·
METR HCAST doubling 4.5 months · 5-milestone ladder SC → SAR → SIAR → ASI) ·
Nika's 7-axes structural position (Rust + YAML + Memory native + MCP+Shield
+ AGPL + local-first + BYO-LLM) is **the distributed third path** between
the AI-2027 « Race » and « Slowdown » endings.

**Anti-Palantir framing** · Palantir = surveillance-capitalism architecture
(centralized data fusion · government contracts · vendor capture by design).
Nika Diamond is the **structural inverse** ·

| Dimension              | Palantir-style      | Nika Diamond                                   |
|------------------------|---------------------|------------------------------------------------|
| Data location          | Centralized cloud   | Local-first per Rule 1 (`supernovae-alignment`)|
| User memory ownership  | Vendor-controlled   | User-controlled (Oxigraph · file-based · RDF) |
| License                | Proprietary         | **AGPL-3.0-or-later** (share-alike forced)    |
| Plugin protocol        | Closed-vendor       | **MCP + ACP + WASM Component** (open)         |
| Provenance             | Black-box           | RDF-star reification (W3C standard · audit)   |
| Inference provider     | Single-vendor       | **9-provider catalog · user picks**           |
| Threat model           | Surveillance        | Sovereignty + capability-scoped agents        |

**Framing lens-pair** · « unified Rust runtime contract » (per `dx/.claude/rules/naming-memory-subsystem.md` substrate lens · one Rust binary · one ABI · zero runtime fragmentation) + « 7-axes distributed third path » (this §4.7 strategic lens · anti-Palantir surface positioning) are SAME structural commitment from different altitudes · NOT contradictory.

Per `olympus-vs-nika-distinction.md` D-2026-05-08-N1 · **Nika engine = public
moat** (competes with GPT/Claude/Cursor/LangGraph/Temporal) · **Olympus =
atelier interne** (Jarvis-like operational alter-ego · NOT a product · consumes
Nika crates one-way). The « mini guardian AI » framing maps cleanly ·

- **Nika engine** = the SOVEREIGN AGENT INFRASTRUCTURE · what the world uses ·
  AGPL forces share-alike on hosted forks · AI-2027 « distributed third path »
- **Olympus** = the GUARDIAN OF THE STUDIO · ingests Nika engine + cockpit +
  hygiene runs · per `olympus-platform-canonical.md` v1.x · atelier-tier
  Jarvis-like surface · alter-ego operational (D-2026-05-11-N7)

The 14 AI-2027-to-Nika mappings (research locked 2026-04-29) survive 2026-Q2
stress-test · Diamond memory subsystem maps directly to AI-2027 « Memory bundle
of vectors mar 2027 » prediction (BGE-M3 + Oxigraph + 9 satellites). Nika is
NOT racing AGI · Nika is building the infrastructure that makes the AI-2027
trajectory LIVABLE without vendor capture.

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
