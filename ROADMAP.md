# Nika Roadmap

> **⚠️ VERSION POLICY AMENDED 2026-06-20 (D-2026-06-20-N1) · "forever-v0.x" RETIRED.**
> Nika now follows real semver toward a **1.0** public launch. The latest
> tagged public release lives on the
> [releases page](https://github.com/supernovae-st/nika/releases)
> (release-candidate grade); `main` advances to the
> next `-dev` version immediately after release → the contract system, the
> **whole Connectome** (memory + cognition) and the hundred-year machinery
> (NEP door · conformance tiers · archives · the compatibility promise ·
> the deterministic checker · succession) land BEFORE the launch, declared
> pre-conditions of the official 1.0 (D-2026-07-22-N1) → design-partner
> `1.0.0-rc.N` → first public launch **1.0.0**, the culmination → 1.x minors
> add the remaining crates additively → the next major stays **un-numbered and
> unnamed**, its content the operator's to declare (D-2026-07-10-N4). The nine-key LANGUAGE
> envelope (`nika: <id>` · ADR-113) is frozen forever and unaffected. The `v0.8X.Y` layer-tag scheme below
> is SUPERSEDED (kept for history). See ADR-002 amendment + §Tag scheme.
> The dated history and the forward gates live in the machine-verified
> [timeline](https://github.com/supernovae-st/nika-spec/blob/main/timeline/timeline.yaml)
> (every provable claim re-proven by the spec's CI · conditions, never dates).
>
> **🦋 ADR-037 — Bottom-up diamond progression (Accepted 2026-04-17, SUPREME)**
>
> Nika is built bottom-up, one layer at a time: **L0 → L0.5 → L1 → L2 → L3 → L4 → L5**.
> Every feature is crafted as its layer is reached. No feature defer. No
> version-milestone ceremony. Real semver toward a 1.0 launch (amended
> D-2026-06-20-N1 · was "no v1.0 target · forever-v0.x").
>
> Tag scheme (Q-plan 3b): `v0.8X.Y` where `X` climbs per layer-phase completion
> and `Y` increments per crate admission. See §Tag scheme below.
>
> Schema envelope (Q-R5 · amended 2026-08-13 by ADR-113): nine keys **forever** ·
> the identity rides ON `nika:` (`nika: <kebab-id>` · the version slot died
> losslessly · `workflow:` is not an envelope key) · supersedes the single
> version marker of ADR-082 and the K8s `apiVersion:` form of ADR-021.
> See nika-spec spec/01-envelope.md.
>
> Crate count: the **Diamond architecture target is 42** (admitted census in
> the generated status block below); the **long-term** envelope grows to **50-90** (cap 100), driven by the
> 11-crate Connectome cluster (1 L2 orchestrator + 10 L1 satellites · ratified
> 2026-06-11 · lands whole before 1.0 per D-2026-07-22-N1), `nika-embed`, WASM + sandbox. See
> §Crate sequence.
>
> The target counts **architectural units, not workspace members**
> (D-2026-07-09-N1): a size-cap descent (one crate splitting at the 15k
> prod-LOC wall — `nika-cap` out of `nika-schema` · `nika-dap` out of
> `nika-cli`) is the SAME unit in two members, named by parentage in the
> descending crate's spec. A `43 / 42` census line is two members of the
> cli unit, not a 43rd unit.

**Philosophy: diamond-grade at every release, real semver toward 1.0**
(amended D-2026-06-20-N1 · was "forever v0.x"). Nika ships a **1.0** at its first
public launch and increments quality additively afterward (1.1, 1.2, … each
diamond-grade for its declared scope). The version number CLIMBS to communicate
maturity. Unshipped features are on this roadmap — **nothing ships as a surprise,
nothing ships half-baked**. The SQLite precedent below is the model: a complete,
diamond-grade product at each release, with features added across the X.x line.

> "Diamond is a CRAFT STANDARD, not a scope list. SQLite 1.0 didn't have WAL,
> FTS, JSON1, or window functions. Those came across 3.x releases over 20 years.
> Each release was diamond-grade. The product was complete AT EACH RELEASE for
> what it claimed to do."

## Why Diamond

The legacy v0.79 codebase grew to 322k LOC across 31 crates. At that scale the
single biggest correctness risk stopped being algorithmic and started being
**context**: no AI assistant could reason about the whole system; no human
could hold it in working memory. Refactors happened by pattern-match, and
pattern-match hallucinated.

Diamond is a rewrite under one constraint: **every crate fits in an AI context
window**. 15k LOC cap per crate. 1,500 LOC cap per file. 100 lines cap per
function. 50-90 crates instead of 31, because smaller, sharper crates compose
better than fewer, bloated ones.

The math: legacy 138k-LOC monolith = ~750k tokens = 75% of a 1M-token context
just to read ONE crate. Diamond 14k-LOC crate = ~70k tokens = 7% of context.
Same task. 10x the reasoning headroom. Zero hallucination refactor.

This is why we rewrite instead of refactor. Refactoring a 322k codebase with
1,276 `.unwrap()` calls and 47 files above 1,500 LOC is not a craft activity —
it's damage control. We want to craft. So we start from zero, keep legacy as a
read-only reference (`git show brouillon:...`), and admit each crate through 12 gates
before it joins the workspace.

See `docs/architecture/ai-velocity.md` for the full argument.

## Current state

> First public release (2026-06-21 · `brew install supernovae-st/tap/nika`).
> The workspace census (admitted · WIP · per-layer) lives in the auto-generated
> block below · RC-grade toward the **1.0.0** launch, which the 7 shadow zones
> gate (D-2026-06-20-N1).

> Single source of truth: `bash scripts/refresh-status.sh`. The block
> below is regenerated by that script and parity-enforced by hygiene
> vector 23 (`check-status-claims-sync.sh`).

<!-- AUTO-GENERATED by scripts/refresh-status.sh — do not edit by hand -->
<!-- Status drift between this block and any quoting doc is caught by
     scripts/hygiene/check-status-claims-sync.sh (vector 23). No branch
     row since 2026-08-26 (#1240): it stamped the generation branch —
     always a deleted feature branch under PR flow — never main. -->

| field            | value                                          |
|------------------|------------------------------------------------|
| HEAD             | `d1756f3b7` (`d1756f3b7541aa599147b5403e991d24e4901ad9`)             |
| workspace        | v0.116.2                                  |
| crates (workspace)| 71                                              |
| crates (admitted)| 63                                             |
| crates (WIP)     | 8 — nika-chart nika-fx nika-proof nika-store nika-harness nika-execution nika-service-execution nika-serve                                  |
| L0               | 22                                              |
| L0.5             | 6                                              |
| L1               | 17                                              |
| L1.5             | 4                                              |
| L2               | 5                                              |
| L3               | 3                                              |
| L4               | 14                                              |
| lib tests        | 6932 passed, 0 failed                              |
| clippy           | 0 warnings                              |

Diamond foundation — orphan branch from scratch. Live counts (admitted ·
WIP · per-layer) are in the AUTO-GENERATED status block above (the
hand-typed "8 crates admitted" this paragraph carried rotted to 30+ ·
the PILLAR-1 class). The list below is the per-crate HISTORY (admission
notes · key decisions), not a census.

- **nika-types** — L0 foundation value types (split from nika-error 2026-04-16, `5baeee044`)
  - 23 types (cost, trust, budget, retry, baggage, schema, hash, resource, role, token_usage, etc.)
  - Re-exported by `nika-error::prelude::*` (and now `nika-kernel::prelude::*` via Q7)
- **nika-error** — error infrastructure (144 tests, 88.5% mutation, `42909b1c7`)
  - 23 L0 foundational types (cost, trust, budget, retry, baggage, schema, hash, resource)
  - Cost stdlib arithmetic (`Add`/`Sub` with `checked_add`/`checked_sub`)
  - `TrustLevel::Default` removed (safe-by-default inversion)
  - 20 proptest lattice/identity laws (cost, trust, baggage)
- **nika-catalog** — static catalogs with phf+unicase lookup (226 tests after 4B)
  - 42-variant typed `Tag` enum, Cargo feature gating, Shield XOR invariant
  - 105 MCP servers, **32 LLM providers**, 13 embeddings, 63 builtins, 65 transforms
    *(this is the `nika-catalog` **inventory** — what the engine has metadata for. Distinct from the `nika-spec` **v0.1 stdlib contract** of 16 providers · 27 builtins · 9 extract modes that a conformant engine must support (counts per canon.yaml · spec ADR-104 promoted huggingface + nvidia · the media pair joined the contract at 0.94) · the catalog's extra builtins beyond the contract stay opt-in metadata. The 63→22 catalog reconciliation was D-2026-05-26-N5/N6 + ADR-086/087/088 Rams sweep 2026-05-27.)*
  - **TOML-driven capability resolver** — **49 rules**, zero-alloc, proptest 10k parity
  - `api_dialect` on all 32 providers (closed set, FK-validated at build time)
  - **12-field `ModelCapabilities`**: token_limit_param + temperature + stop + reasoning + input/output modalities + tokenizer (**12 families**: Cl100k/O200k/ClaudeV3/Gemini/LlamaV3/LlamaV4/MistralV3/DeepSeek/Qwen/Granite/Glm/Grok) + supported_parameters (**13 flags**: incl. BatchApi/ContextCaching/PredictedOutputs/ComputerUse/Citations/IncludeReasoning) + system_messages + context_window_tokens + max_output_tokens + json_mode
  - **8 Modalities**: Text/Image/Audio/Video/Pdf + Embedding/Speech/ImageGen (4B)
  - 7-axis `ModelPricing`: input + output + cached_input + image + reasoning_tokens + provider + pattern
  - Invariant #19 FULL, Gate 8 GREEN
  - Community extension pattern: `nika-catalog-cn`, `nika-catalog-eu`
- **nika-catalog-verify** — online registry verifier binary (`a977e35b1`)
- **nika-kernel + nika-kernel-mock** — kernel traits + mock impls (144 + 116 tests)
  - 6 L0.5 traits (`IdGenerator`, `SecretResolver`, `MetricsExporter`, `TracerProvider`, `EventSink`, `BillingSink`)
  - Sealing pattern on `Provider`, `EventSink`, `BillingSink`, `SecretResolver`
  - Forward-compat seams: `cancel.rs`, `plugin/wasm.rs`, `plugin/sandbox.rs` (observability unified trait dropped per Q12 rev.3 — replaced by `infra/{audit,metrics,trace,event_sink,billing}.rs`)
  - `InferResponse.cost: Option<Cost>` + structured `DenialKind`
  - `MemoryId` UUIDv7 migration, `#[deprecated] cost_usd` bridge
  - `HttpStreamResponse::new()` (4A-stabilize, inv #19)
  - `#[non_exhaustive]` on all 20 mock structs (4A-stabilize)
  - **Q7 prelude hub** (2026-04-16, `d967f4a7a`): `nika_kernel::prelude::*` re-exports `nika_error::prelude::*` so L2+ verb crates depend on `nika-kernel` only (3 lines of impl + 3 regression tests pinning 23 symbols)
- **nika-schema (ADMITTED · 2026-06-18)** — workflow AST + parser + analyzer + static-check · all 12 gates green (Gate-5 budget 290≤300) · the pre-Diamond rounds below are superseded history (the `nika: v1` rewrite shipped + admitted) <!-- stale-ok: superseded admission history -->
  - **Round 2c** (`b85b612ca`, 2026-04-16): parser scaffold, top-level scalars (`name`, `description`, `goal`, `provider`, `model`, `schema`)
  - **Round 2d** (`2480822df`): `tasks:` sequence + 5-verb action discriminator (`enum Verb` exhaustive match) + minimum required field per verb
  - **Round 2e-part-1** (`eac346c71`): optional task-level `depends_on`, `condition`, `for_each`
  - 38 parser tests + 100+ across raw/types/guardrails/source/trust/error
  - **PENDING REWRITE** — Phase D nuke + redo with `nika: v1` + `kind: Workflow` + `metadata` + `spec` envelope (per nika-spec · the `kind` discriminator survives, the version-field shape is `nika: v1` not `apiVersion: nika.sh/v1`) <!-- stale-ok: superseded admission history -->

Totals: live numbers (lib tests · clippy · LOC · providers · capability
rules · ADRs · hygiene vectors) are in the AUTO-GENERATED status block
above (`scripts/refresh-status.sh` · vector 23 parity) — this paragraph
used to hand-type them and drifted (said 905 tests while the block said
1267 · the exact class PILLAR 1 forbids).

Phase C: Wave 2 ✅ (L0 types + L0.5 traits), Wave 3 ✅ (stabilization + review swarm).
Phase D: Session 1 ✅, Session 2a ✅, Session 2b ✅, Session 3 ✅, Session 4A ✅, Session 4B ✅,
       Round 2c ✅, Round 2d ✅, Round 2e-part-1 ✅ (parser scaffolding through verb dispatch).
Phase A (this session): baseline status reset ✅. Foundation v0.81 plan
LOCKED 2026-04-16 via 8 ADRs (006-amend + 021-028) + 14-crate constellation.

**See `docs/adr/adr-021..028.md` for the locked decisions and
`~/.claude/.../memory/project_foundation_v081_constellation.md` for the
14-crate constellation.** Wave 4A/4B reservations (FCI-035) add stub
ADR-029/030/031/032/035 — prose pending Phase C.

Next phases (~35 commits across B-H):
- Phase B: hygiene foundation (vectors 22 + 26-33) + crates.io legacy yank
  + add `publish = false` to all foundation Cargo.toml + P0 Rust fixes
  (#[non_exhaustive] on Span/Spanned/LineCol/Templatable, private fields on
  TaskId/ToolCallId, Spanned PartialEq fix) + .config/nextest.toml +
  rust-toolchain.toml
- Phase B': nika-catalog internal cleanup (split data/models.rs + build/) +
  rename kernel/io/process.rs → shell.rs
- Phase C: flesh out 8 ADR stubs into full prose + nuke/redo
  docs/crate-specs/nika-schema.md + write 6 new specs (envelope, event,
  binding-transform, binding, catalog-codegen, schema-ast, macros)
- Phase D: foundation refactor — rename nika-types → nika-core, drop
  nika-error facade, create nika-macros stub, extract nika-envelope, split
  nika-schema → nika-schema-ast + nika-schema, refactor parser to envelope
  (apiVersion/kind/metadata/spec, drop goal, single-string model), relabel
  nika-catalog-verify L0 → L4
- Phase E: envelope hygiene + JSON Schema publication
- Phase F: extract nika-catalog-codegen
- Phase G: build nika-event + nika-binding-transform + nika-binding
- Phase H: SOTA patterns adoption (bon, Arc<str>, camino, public-api,
  release-plz, doc examples ratchet)

**Next steps** · the admission order is the tag scheme below plus the
forward gates of the machine-verified timeline; the 2026-05-12 waypoint list
that stood here is superseded (git keeps it) — its memory-subsystem naming
predates the Connectome canon.

The **4-verb invariant (`infer · exec · invoke · agent`)** is locked through 2036 per BLUEPRINT_2036 §1 stress-test (D-2026-05-22-N18 · `fetch` is the `nika:fetch` builtin via `invoke`, not a verb) · candidates `fetch/embed/evaluate/train/serve/stream/transform` all collapse cleanly into the 4-verb taxonomy. ADRs 050-056 queue Phase 2-7 amendments (WASM Component Model · CRDT federation · edge no_std subset · multi-protocol gateway · 3 cluster-collapses).

## Tag scheme (real semver · amended D-2026-06-20-N1)

Real semver toward a 1.0 launch, then `MAJOR.MINOR.PATCH`:

- **MINOR** within a major = additive (new crates · builtins · providers · polish).
- **PATCH** = bug fixes, zero contract change.
- **MAJOR** = an architectural epoch (the nine-key language envelope is frozen,
  so a major is driven by the engine epoch, not by language breaks).

| Tag        | Meaning                                                              |
|------------|----------------------------------------------------------------------|
| `0.91.0`   | release-candidate grade — usable vertical slice works (4 verbs · 14 providers · effects · static-check · MCP/LSP · CLI) · headless workspace build |
| `0.92.0`   | **agent-native** — `nika wire codex` · `nika init` ships the repo-level agent skill (`.agents/skills/nika-authoring`) · the plugin marketplaces (Codex + Claude Code · `supernovae-st/nika-plugins`) · MCP learning tools (schema · examples · template · canon) · cold-first-run fixes |
| `0.93.0`   | `nika-cap` admitted (capability tokens · L0) — 40/42 · `nika run --resume` durable-lite (ADR-099) · `nika test` harness · 180s provider-plane timeout unbricks local thinking models |
| `0.94.0`   | **the media suite** — `nika:image_generate` + `nika:tts_generate` (stdlib 25) · trace files by default · `catalog`/`tools --json` + their MCP twins (protocol 2026-07-28) · per-task `cost_usd` · huggingface + nvidia join (16 canonical providers) |
| `0.95.0`   | **local-first catalog** — the 5 local servers get their catalog face (keyless by construction · sovereign models stay unpriced) · seam-discipline hygiene vector |
| `0.96.0`   | **the run becomes a place** — `nika dap` time-travel replay · `nika trace export` (OTLP/JSON) · the caller contract (`check --json` requirements) · did-you-mean on unknown fields · tamper-evident journal |
| `0.9x`     | the hardening train — `nika-pck-manifest` (manifest DTO · L0) + `nika-binding` (types-first slice) land **42/42** · the 7 shadow zones close |
| `1.0.0-rc.N` | design-partner hardening                                            |
| `1.0.0`    | **first public launch** — language + installable binary, validated   |
| `1.1 · 1.2 · …` | additive minors — new builtins, new providers, polish            |
| pre-1.0    | **the contract system + the whole Connectome + the hundred-year machinery** land before the launch — declared pre-conditions of the official 1.0 (D-2026-07-22-N1 · ADR-004) |
| next major | reserved · un-numbered and unnamed · content the operator's to declare (D-2026-07-10-N4) |

> **Superseded `v0.8X.Y` layer-tag scheme (kept for history).** Before
> D-2026-06-20-N1 the tags climbed per layer-phase (`v0.81`=L0 complete …
> `v0.90`=L3 complete … `v0.92`=L5 binary, "forever-v0.x"). That scheme is
> retired · the engine re-versions `0.80.0 → 0.90.0` and the crate count
> moves to post-1.0 minors (ADR-037 horizon 50-90 · cap 100 · projected,
> never a gate · ruled D-2026-07-21-N1). Original table preserved in git history.

## Schema envelope — nine keys, forever (Q-R5 · per nika-spec · ADR-113)

Every `.nika.yaml` workflow opens with `nika: <kebab-id>` **forever** — the
key says « this is a Nika file », the value is the file's NAME (the `v1`
version slot died losslessly on 2026-08-12: one legal value is not a
version · ADR-113). The envelope is exactly nine keys · `nika` · `model` ·
`inputs` · `const` · `secrets` · `permits` · `run` · `tasks` · `outputs` ·
and never bumps to a v2 (a `v2` is effectively never). Canonical envelope:
`nika-spec` spec/01-envelope.md. `https://nika.sh/spec/v1` is the internal
RDF/conformance URI only, never typed by authors.

Sub-field maturity is annotated inline via `x-nika-alpha` / `x-nika-beta` /
`x-nika-deprecated` hints that parsers surface as warnings, never errors.
The kernel schema registry gains promotion paths for these annotations
(alpha → beta → stable → deprecated) without breaking any workflow.

A workflow that validates on an early tag must still validate on every later one.
Removed fields become `x-nika-deprecated` with a 12-month sunset window; they
continue to parse — they just warn.

## Crate sequence (ADR-037, dep-ordered)

Bottom-up build order. Every crate is crafted when its layer is reached.
No defer, no feature-milestone ceremony. Strictly downward dependencies.

> Live admission state per layer lives in the auto-generated
> §Current state block above (`scripts/refresh-status.sh` · vector 23).
> The lists below are the dep-ordered PLAN.

### L0 — foundation (~9 crates, pure types, sync)

Plan: `nika-types`, `nika-error`, `nika-catalog`, `nika-catalog-codegen`,
`nika-schema`, `nika-event`, `nika-pack`, then `nika-cap` (capability
tokens, proposal A from swarm-1), `nika-pck-manifest` (manifest DTO
scaffold, proposal B), `nika-binding` (types-first slice).

### L0.5 — kernel traits (6 crates post 4-way split 2026-06-10, sealed, ISP)

`nika-kernel` (facade + range-registry hub), `nika-kernel-core`,
`nika-kernel-ai`, `nika-kernel-runtime`, `nika-kernel-plugin`,
`nika-kernel-mock`. See `docs/architecture/kernel-split-census-2026-06-10.md`.

### L1 — effects (~50+ crates, tokio-async OK)

Sub-phased per Q2 (topological × user-value):

- **1a primitives**: `nika-fs` ✅ · `nika-exec-runner` ✅ (shell/process
  effect) · `nika-http` ✅ · `nika-blob` ✅ · `nika-keys-env`
- **1b deps-étage-1**: `nika-git` (gix), `nika-keys-keychain`, `nika-catalog-sync`
- **1c providers** ✅ SHIPPED 2026-06-11 as **`nika-providers`** (ONE L1.5
  crate · 14/14 canonical providers over 3 wire formats: anthropic ·
  openai-compat ×12 · gemini · in-crate mock — rig NOT carried, Diamond
  talks wire directly through the kernel http seam per D-2026-05-22-N17).
  The native in-process backend ships separately as `nika-infer-local`
  (candle sidecar · ADR-091)
- **1d pck backend**: `nika-pck-registry`, `nika-pck-store`
- **1e memory foundation** (the Connectome climb): `nika-embed` (own
  quantized local model), `nika-bm25` ✅ (ADR-038), `nika-hnsw`, `nika-rrf`,
  `nika-autodesc-minimal`
- **1f memory advanced**: `nika-temporal`, `nika-fsrs`, `nika-rdfs-reasoner`,
  `nika-graph-algos`, `nika-rerank` (local reranker), `nika-autodesc-full`
- **1g frontier satellites** (graduated as pulled forward): `nika-consolidator`,
  `nika-sync` (CRDT), `nika-stream`, `nika-trust`, `nika-causal`,
  `nika-datalog`, `nika-multimodal`, `nika-coordinator`. Tulving-class
  memory levels (episodic · working · procedural · …) are `MemoryLevel`
  frame classes inside the orchestrator, not crates.

### L2 — domain (~15 crates)

`nika-verb-{exec,invoke,infer,agent}` (fetch = `nika:fetch` builtin via invoke), `nika-pck` orchestrator,
`nika-connectome` orchestrator (the Connectome), `nika-builtin`,
`nika-builtin-{github,cloud,workspace}`,
`nika-mcp`, `nika-display`.

### L3 — orchestration (~6 crates)

`nika-runtime`, `nika-shield`, `nika-wasm-host`,
`nika-sandbox-{linux,macos,windows}`.

### L4 — interfaces (~7 crates)

`nika-cli`, `nika-daemon`, `nika-serve`, `nika-mcp-server`, `nika-lsp`,
`nika-sdk`, `nika-init`.

### L5 — binary (1 crate)

`nika` (composition root, <500 LOC, zero logic).

### Memory subsystem detail — the Connectome cluster (1 orchestrator + 10 satellites · ADR-004)

Specs: `docs/crate-specs/nika-<sat>.md`.

**10 satellites** (admitted during L1 phases 1e-1f, composed by `nika-connectome` L2):

- `nika-hnsw` — ANN vector recall (M adaptive per store size, efConstruction=400)
- `nika-bm25` ✅ — lexical recall (ADMITTED · ADR-038)
- `nika-rrf` — Reciprocal Rank Fusion hybrid ranking
- `nika-rerank` — local reranker, M13 (late-interaction tier + optional 0.6B cross-encoder)
- `nika-fsrs` — FSRS spaced-repetition forgetting (decay → demotion, never hard-delete)
- `nika-rdfs-reasoner` — RDFS + OWL 2 RL inference
- `nika-graph-algos` — petgraph surface (BFS/DFS/SCC/PageRank/Leiden communities)
- `nika-temporal` — bitemporal model (valid-time + transaction-time, Graphiti-class)
- `nika-autodesc-minimal` ★ — provenance-attached zero-LLM ingest (PROV-O · the moat)
- `nika-autodesc-full` ★ — schema evolution + graph summarization (ADR-042 split)

**Storage**: Oxigraph
in-memory + N-Quads snapshot at boot/shutdown (pure-Rust · single binary ·
zero system dep). Vendored-RocksDB persistence is opt-in `feature = "rocksdb"`
when scale demands. Embeddings ride as RDF literals (Scenario D); the HNSW
cache is ephemeral, rebuilt at boot. The kernel sealed traits
(`MemoryRemember`/`MemoryRecall`/`MemoryForget` + `MemoryLifecycle` +
`EmbeddingProvider`, NIKA-601..649) remain the plugin seam; community
backends ship via `nika pck install` with Sigstore signature required.

**Trust model** (B4 locked, hybrid defense-in-depth):

- Per-frame Ed25519 signature — provenance, NOT optional
- Query-rank trust-weighted — low-trust frames demote before fusion
- Decay over time — age-weighted trust half-life (configurable)
- Human-in-loop verdict — manual promote/demote logged to `EventLog`
- Audit trail — every trust change is an immutable event

**Tenant isolation** (B5 · compile-time keyspace partition):

```rust
// nika-kernel-ai/src/memory.rs — shipped form
pub struct MemoryFrame { /* … */ pub tenant: TenantId, /* … */ }
pub struct RecallQuery { /* … */ pub tenant: TenantId, /* … */ }
```

Reserved since the early schema for post-1.0 multi-tenant. Every satellite partitions its
keyspace by `tenant`; cross-tenant reads are structurally unexpressable
through the sealed recall path.

## Layer-phase deliverables (reference, by phase not by version)

The sections below inventory what gets built **within each layer phase**.
Originally written as "v0.81 / v0.90 / v0.95 / v0.100" marketing milestones,
now reframed per ADR-037 as layer-phase deliverables. Content preserved;
only the framing changed.

## L0 + L0.5 — Forward-compat seams + hygiene hardening (Phase B complete)

**Theme**: lock the structural seams that turn 7 → 50-90 crates into a
non-breaking extension (not a rewrite), before admission velocity picks up.
Pure additions — no behaviour change for existing consumers.

### Kernel forward-compat seams — ✅ SHIPPED (Wave 2, 2026-04-16)

- ~~New `crates/nika-kernel/src/cancel.rs`~~ — ✅ `CancelCtx` module with
  Acquire/Release semantics + `CancelDropGuard`
- ~~New `crates/nika-kernel/src/plugin.rs`~~ — ✅ `WasmPluginHost` trait stub +
  `PluginCall` / `PluginResult` / `PluginError` DTOs, all
  `#[non_exhaustive]`, zero impls
- ~~New `crates/nika-kernel/src/sandbox.rs`~~ — ✅ `Sandbox` trait stub +
  `SandboxPolicy` + `PluginCapabilities` enums + structured `DenialKind`
- ~~`MemoryFrame` reserved fields~~ — ✅ `cipher`, `provenance`,
  `retention`, `redactions` (all `Option<_>`, default `None`)
- ~~`ProviderStream::infer_stream` + `ContextCompressor::compress` cancel param~~ — ✅

All L0.5 kernel seams shipped on HEAD (`244dcc807`). Memory subsystem (L1
phases 1e-1f) + WASM host (L3) are now strictly additive.
See `docs/architecture/forward-compat-invariants.md` §9.

### Reserved traits — ✅ SHIPPED (visible on HEAD, impl per layer-phase)

| Trait | Kernel file | Status | First impl (layer) |
|---|---|---|---|
| `WasmPluginHost` | `src/plugin.rs` | ✅ shipped | `nika-wasm-host` (L3) |
| `Sandbox` | `src/sandbox.rs` | ✅ shipped | `nika-sandbox-{linux,macos,windows}` (L3) |
| `MemoryStore` | `src/ai/memory.rs` | ✅ shipped | `nika-connectome` (L2 · composes the 10 satellites, L1 phase 1e-1f) |
| `EmbeddingProvider` | `src/memory.rs` | ✅ shipped | `nika-embed` (L1 phase 1e) |
| `IdGenerator` | `src/id_gen.rs` | ✅ shipped (W2) | in-tree |
| `SecretResolver` | `src/secret.rs` | ✅ shipped (W2, sealed) | in-tree |
| `MetricsExporter` | `src/metrics.rs` | ✅ shipped (W2) | `nika-observability-otel` (L3/L4) |
| `TracerProvider` | `src/trace.rs` | ✅ shipped (W2) | `nika-observability-otel` (L3/L4) |
| `EventSink` | `src/event_sink.rs` | ✅ shipped (W2, sealed) | in-tree |
| `BillingSink` | `src/billing.rs` | ✅ shipped (W2, sealed) | in-tree |

### Hygiene vectors expansion (10 → 21) — ✅ SHIPPED (Wave 1, 2026-04-16)

21/21 vectors GREEN. P0/P1 in pre-commit fast path; P2/P3 in pre-push
full path. Includes layering, security-axes, cargo-geiger unsafe-deps,
env-example parity, license consistency, no-async-in-L0, catalog owned
strings, kernel-no-spawn, no-`Box<dyn Error>`, cancel-safety docs,
case-insensitive collisions.

## L0 tooling + release automation (Phase B/B')

- **`tools/` → `crates/` rename** — DONE (converged on Cargo community
  convention; verified by `cargo test --workspace --lib` staying
  green and grep returning zero `tools/nika-` outside historical ADR
  references and legacy-lookup commands).
- **Release automation skeleton** — `release-plz.toml`, `cliff.toml`,
  `justfile` at engine root, and `crates/xtask/` port of the release +
  hygiene runners into typed Rust.

## L1 + L2 — Runtime-complete foundation (Phases 1a → 2)

> **No deadline.** Quality > speed. All inventory below is built incrementally
> as its crate's layer-phase is reached. Tag scheme climbs `v0.82 → v0.89` as
> phases complete. No "ship event" — diamond is incremental.

**Scope**: complete workflow engine + package manager + native API adapters +
hybrid memory subsystem + forward-compatible architecture.

### Runtime-complete

**Core engine**: schema, binding, event, runtime, daemon, cli, 4 verbs
(infer/exec/invoke/agent · fetch = `nika:fetch` builtin via invoke).

**Catalog v3**: TOML source-of-truth + `build.rs` + `phf_codegen` compile-time
generation. `xtask verify-catalog` with parallel npm/pypi/docker registry
checks. GitHub Actions daily cron for drift detection. Multi-distribution
support: `Distribution::{Npm,Pypi,Oci,Remote,Binary,Mcpb}` aligned with
official MCP Registry schema (`packages[]` + `remotes[]`).

**Providers** ✅ SHIPPED 2026-06-11 as **`nika-providers`** (one L1.5 crate ·
the catalog carries 32 provider rows as metadata · the v0.1 stdlib contract
wires the canonical 14 over 3 wire formats — anthropic · openai-compat ×12 ·
gemini · in-crate mock · rig NOT carried). The native in-process backend is
`nika-infer-local` (candle sidecar · ADR-091).

**Tier 1 catalog additions** (high-priority provider metadata):
- `moonshot` — Kimi K2 (1M+ context, top-tier open-weight)
- `qwen` — Alibaba DashScope (Qwen3-Max competitive with GPT-5/Claude 4.5)
- `deepinfra` — OpenAI-compat, popular open-weight hosting
- `novita` — OpenAI-compat, growing rapidly

**Default models updated to April 2026 flagships**: Claude Sonnet 4.5, GPT-5,
Grok 4, Gemini 3.0 Pro, Command A, Jamba 1.6, Voyage 3.5, cross-region Bedrock
profiles. Pricing entries for `gpt-5-{mini,nano}`, Perplexity Sonar (all 4
variants: sonar/sonar-pro/sonar-reasoning/sonar-deep-research), Cohere,
AI21 Jamba 1.6, Voyage embeddings (voyage-3.5/voyage-3.5-lite/voyage-code-3),
Grok 4 variants (grok-4/grok-4-fast).

**Model metadata expanded** (all models):
- `context_window_tokens: u32` — critical for `| token_count` sanity + cost
  estimation + fallback decisions
- `max_output_tokens: u32` — per-model output caps
- Per-provider capabilities (vision, thinking, structured output)

**Catalog P1 improvements** (fix silent bugs + UX):
- ~~Scope `find_pricing` by provider~~ — ✅ DONE (`find_pricing_scoped`)
- ~~Enforce pricing-pattern ordering at compile time~~ — ✅ DONE (build.rs assertion)
- ~~`suggest()` fuzzy matcher with `strsim`~~ — ✅ DONE
- ~~`Category` as enum, not `&'static str`~~ — ✅ DONE
- 42-variant `Tag` enum — typed vocabulary for modalities, economics, sovereignty,
  MCP permissioning. Build-time enforced sort/dedup + MCP read-only XOR destructive.
- Cargo features for subset compilation (full/minimal/per-content + extension-author)
- Community extension pattern (`nika-catalog-cn`, `-eu`, `-enterprise`)
- ~~Capability migration to TOML~~ — ✅ DONE (Session 2a: 42→49 rules, `CapPatch` + `Matcher`)
- ~~Modality/tokenizer/supported_parameters~~ — ✅ DONE (Session 2b + 4B: 8 modalities, 12 tokenizers, 13 param flags)
- ~~32 LLM providers with capability rules~~ — ✅ DONE (Session 3 + 4B)
- Phase D remaining: 5-axis pricing population (Phase E2), provider trust/residency,
  HTTP API catalog, MCP lifecycle, CatalogOverlay trait (nika-kernel), perf polish,
  Gate 5 mutation ≥90%

**Security (7 shadow zones GREEN, non-negotiable)**:
- Gate 1: `nika serve` input trust (P0)
- Gate 2: cross-provider structured output parity (P0)
- Gate 3: binding/template hardening
- Gate 4: L1 taint runtime
- Gate 5: `for_each` per-element spotlight
- Gate 6: `NikaError` Display parity
- Gate 7: provider parity matrix

### Runtime internals — itemized

**4 verbs** (each a crate under `nika-verb-*`):
- `exec` — subprocess + `kill_on_drop(true)` + `tokio::try_join!` for concurrent pipe reading
- `invoke` — builtins (`nika:*`) + MCP tools (`server::tool`) with shared dispatch
- `infer` — LLM call with structured output 5-layer defense (tool injection → rig extractor → validation → retry → repair)
- `agent` — multi-turn loop with `completion` modes (explicit/natural/pattern) + 4 guardrail types (length/schema/regex/llm-judge) + `token_budget` + `max_cost_usd`

(`fetch` is **not** a verb — fetching a URL is calling a tool. It is the
`nika:fetch` builtin: HTTP + SSRF guard + 9 extract modes
(markdown/article/text/selector/metadata/links/jsonpath/feed/llm_txt) +
`response: {full, binary}`, reached via `invoke: {tool: "nika:fetch"}`.
Per D-2026-05-22-N18.)

**63 builtin tools** (full engine inventory · the `nika-spec` **v0.1 stdlib contract** curates **25** of these — 6 core · 5 file · 8 data · 2 network · 2 introspection · 2 media — per the ADR-086/087/088 Rams collapses (jq-subsumption · `wait` + `inspect` unifications · was 42 → 22, then `nika:compose` + the media pair joined) · the remaining media builtins stay stdlib-v0.x opt-in) split by domain (lives in `nika-builtin` L2, with native API adapters split into bundle crates `nika-builtin-{github,cloud,workspace}` for heavy deps isolation):
- Core (7): `sleep`, `log`, `emit`, `assert`, `prompt`, `run`, `complete`
- File (5): `read`, `write`, `edit`, `glob`, `grep`
- Introspection (6): `dag_info`, `task_status`, `threads`, `orchestrate`, `cost`, `records`
- Data (19): `json_merge`, `json_diff`, `set_diff`, `zip`, `map`, `filter`, `group_by`, `chunk`, `token_count`, `enrich`, `jq`, `tree_data`, `inject`, `json_verify`, `yaml_validate`, `locale_lookup`, `aggregate`, `json_flatten`, `json_unflatten`
- Media always-on (5): `import`, `decode`, `dimensions`, `thumbhash`, `dominant_color`
- Media core (3): `thumbnail`, `convert`, `strip`
- Media opt-in (18): `metadata`, `optimize`, `svg_render`, `chart`, `phash`, `compare`, `pdf_extract`, `provenance`, `verify`, `qr_validate`, `quality`, `html_to_md`, `css_select`, `extract_metadata`, `extract_links`, `readability`, `pipeline`

> ⚠️ **The per-category lists above are the engine's pre-D-N6 full-inventory snapshot** (§2.7 · preserve + flag). The **spec-curated 22** (the contract · `nika-spec/stdlib/builtins-v0.1.md`) is canonical: jq subsumes ~13 of the Data entries (`map`/`filter`/`group_by`/`flatten`/`aggregate`/`enrich`/`inject`/`base64`/`json_to_csv`/`locale_lookup`/`json_merge`) · `json_verify`+`yaml_validate`→`validate` · canonical Core = `log`/`emit`/`assert`/`prompt`/`done`/`wait` (`sleep`+`wait_until`→`wait` per ADR-087 · `run` removed · `complete`→`done` per D-N18) · canonical Introspection = `inspect` (view-discriminated · `cost`/`records`/`dag_info`/`threads` collapsed per ADR-088 · `task_status`→`${{ tasks.X.status }}` · `orchestrate` cut) · fetch extract-mode `jsonpath`→`jq`. The engine's actual L2 `nika-builtin` set reconciles to the spec at admission (POST_AUDIT-governed · not a cascade edit).

**65 pipe transforms** (in `nika-binding`): string (9), array (9), aggregation (7), numeric (5), type (5), logic (1), introspection (1), parametric (8), query (9), string-test (3), URL (4), encoding (4), JQ (1 — `jq(expr)` full stdlib), system (1 — `shell` escape).

**Schema AST (`nika-schema` L0 ~13k LOC)**:
- Raw AST → analyzed AST with `Spanned<T>` source tracking for `ariadne` diagnostics
- Parser: YAML → workflow (the nine-key envelope · `nika: <id>` per nika-spec · ADR-113)
- Analyzer: DAG construction + cycle detection + unreachable task detection + run-depth enforcement (workflow recursion guard)
- Validator: type coercion on 65 templatable fields (string → f64/u32/bool/provider enum/etc.)
- Taint propagation: `Trust::Untrusted` bits flow through `with:` bindings, enforced at verb boundaries
- Guardrail schema compilation (agent verb)
- Include partials (workflow composition via `include:` with prefix namespacing)
- `when:` conditionals (template expression evaluation)
- `after:` control edges — explicit ordering (`depends_on` died in W2 · `NIKA-PARSE-024`)
- `for_each` concurrency + `fail_fast` semantics

**Binding / templating (`nika-binding` L0 ~13k LOC)**:
- Single-pass template resolver (no re-parse storms)
- 65 pipe transforms + `jq(expr)` subset
- Null-safety: 19 transforms fail on null; `default(x)` recovery
- `{{with.alias}}` (always required prefix — no bare `{{x}}`)
- `$binding_ref` in `for_each` (templates rejected per BUG-003)
- `$env.VAR` for environment access
- `| shell` transform REQUIRED on all bindings in `shell: true` exec (SEC-2)
- Skill injection into all `infer:` system prompts (not just `agent:`)

**Shield engine (`nika-shield` L1 ~5k LOC)**:
- Spotlight wrapping on untrusted data (NIKA-384)
- Canary token emission + outbound scanner (NIKA-382, NIKA-388 also in thinking trace)
- ML injection detector (NIKA-383, BERT-based — lands during L1 memory phase, heuristic ships L3 shield)
- Capability-denied on dangerous tool + untrusted data combo (NIKA-380)
- Trust violation strict mode (NIKA-381)
- Run depth / cycle detection for workflow recursion (NIKA-386/387)
- Untrusted vision images blocked (NIKA-389)

**Event log (`nika-event` L1)**:
- `EventKind` enum with exhaustive emission sites (invariant #24: exactly 1 emit per verb path)
- `EventLog` + `TraceWriter` for NDJSON output
- `nika-macros` inlined (`builtin_tool!`, `event_task_id!`, `nika_error_code!`)
- Subscription API for `nika serve` SSE + TUI live mode

**Display / renderer (`nika-display` L2 ~14k LOC)**:
- `Renderer` trait with modes: `tty` (spinners, icons, colors), `json` (machine), `ndjson` (streaming)
- Live DAG render + per-task status
- Benchmark display (TTFT, tok/s, per-provider comparison)
- CLI format (tables, progress bars)
- Feeds the TUI (when rebuilt, during L4 phase or later)

**`nika serve` (HTTP API, `nika-serve` L4)**:
- Routes: `POST /jobs`, `GET /jobs/:id`, `GET /jobs/:id/stream` (SSE), `DELETE /jobs/:id`
- Auth: bearer token + per-job scoping (env `NIKA_JOB_ID`/`NIKA_JOB_DIR`)
- Rate limiting per token
- Webhook delivery for job completion
- OpenAPI spec generation
- Worker pool with cancellation propagation

**`nika-cli` subcommands (L4 ~20k LOC)** — 40+ commands, keep/drop decision per:
- Run: `run`, `check`, `test`, `test --golden`, `eval`, `dry-run`, `explain`
- Lint: `lint` (rule IDs `L001/L010/L020/L030/L031/L050/L060/L070/L080/L090` — SEC-prefixed subset)
- Scaffolding: `new`, `init`, `init --from <git-url>`, `showcase`, `demo`
- Provider: `provider list`, `model list`, `switch`, `bench` (TTFT, tok/s, quality)
- MCP: shipped read-only `nika mcp` (`nika_check` · `nika_explain`);
  future package/runtime surfaces may add `mcp list`, `mcp install`, `mcp run`
- Keys: `keys` subsystem (10 subcommands) — see `project_nika_keys_design.md`
- Daemon/Serve: `serve`, `daemon start|stop|status`
- pck: `pck add`, `pck update`, `pck publish`, `pck search`
- Diag: `doctor`, `trace`, `records`, `discover`, `inputs`, `tools`, `verbs`, `schema`
- TUI: `ui` (deferred — rebuilt Act 3 or never)
- Housekeeping: `clean`, `rules`, `onboarding`, `machine install`

**Event subsystem, artifact modes, output modes, templatable fields**:
- Artifact modes: `overwrite | append | unique | fail`, with optional `manifest: true` → `artifacts.json` index
- Output modes: `text | json | yaml | markdown | binary`
- 65 typed fields accept `${{ }}` CEL (type-coerced after resolution, NIKA-041 on error)
- Self-hosted LLMs use `model: <provider>/<name>` slash syntax · the provider base URL is operator/engine config, not the project file (`nika.yaml` carries `ceiling` · `arm` · `traces` · `registry`)
- `nika:decode` builtin: base64 → CAS blob (for APIs returning inline images)
- `nika:import` builtin: file → CAS blob

**Per-provider features**:
- Anthropic: `extended_thinking: true` + `thinking_budget: N` tokens (exposed as `InferRequest` field)
- OpenAI: `reasoning_effort: low|medium|high` (o1, o3, o4 families)
- Gemini: JSON mode (`response_format: json`)
- All providers: streaming (MUST, even mock)

**MCP pool (`nika-mcp` L2 ~9k LOC)**:
- `rmcp_adapter` for official SDK compat
- Pool with retry policy per server
- Remote (streamable-http, sse) + local (stdio) transport
- 102 curated aliases + `server::tool` namespace
- Parameter validation (NIKA-107) + connection error (NIKA-100) + start error (NIKA-101)

**Multimodal / vision**:
- `content:` array alternative to `prompt:` (image + text parts)
- `source: "<CAS hash>"` — ONLY CAS hashes accepted (never file paths)
- Per-provider support: anthropic, openai, mistral, groq, gemini, xai
- Text-only: `native` (GGUF), `deepseek` (VisionNotSupported error)

### Cargo features + distribution

**Feature flags strategy** — minimal binary by default, opt-in for heavy deps:
- `nika-infer-local` — sidecar binary (candle · ADR-091 · never linked into `nika`)
- `nika-media-pdf` — `[feature] pdf` (gates pdfium + pdf-extract)
- `nika-media-document` — `[feature] html-md` (gates dom_smoothie, readability)
- `nika-media-provenance` — `[feature] c2pa` (gates c2pa full stack)
- `unstable-*` prefix for API surfaces not yet locked (per forward-compat-invariants)

**Distribution channels**:
- **Homebrew** — `brew install supernovae-st/tap/nika` (formula at `supernovae-st/homebrew-tap` · SHIPPED since v0.90.0)
- **`curl | sh`** — `curl -LsSf https://nika.sh/install.sh | sh` (platform detection, fallback to GH release tarball · SHIPPED)
- **crates.io** — `cargo install nika` joins at the 1.0.0 launch
- **GitHub Releases** — four pre-built tarballs from the tag-triggered release
  train (macOS arm64/x86_64, Linux arm64/x86_64), with checksums, GitHub
  attestations and SLSA provenance; `RELEASING.md` is the ceremony authority

### Site + docs + design

**nika.sh** (Astro 5 static, Scaleway par-fr + Cloudflare CDN):
- `/` landing + hero demo + manifesto link
- `/method` — the craft manifesto (public)
- `/changelog` — per-admission entries, RSS feed
- `/timeline` — the one verifiable record (renders the spec timeline SSOT)
- `/blog` — monthly deep dives
- `/errors/[NIKA-XXX]` — per-code lookup pages (prerendered)
- `/architecture` — 3D diamond view (R3F, P1 post-launch)
- `/install.sh` + `/schema/workflow.json` (static assets for CLI consumers)

**docs.nika.sh** (Mintlify, deployed from
[`supernovae-st/nika-docs`](https://github.com/supernovae-st/nika-docs)
— split out of this repo in Wave 4E, 2026-04-17):
- Getting started, concepts, guides, reference
- 2-tab navigation (Guide / Reference), live snapshot of workspace state

**Agent skill surface** ([`supernovae-st/nika-plugins`](https://github.com/supernovae-st/nika-plugins)):
- The public skill + read-only MCP oracle ("make your AI agent a Nika expert")
- Design tokens + motion ride the spec SSOT (`nika-spec` `design/`,
  vendored into the engine pack by `scripts/sync-pack.sh`)
- Replaces the og-era `nika-design-skill` row — that repo is retired
  (private archive), and its public link here was the last dead one

### Native API adapters (7 builtins)

Thin workflow-grade wrappers over existing Rust SDKs — NOT CLI reimplementations.
Each adapter ≤500 LOC, covers 10-15 most-requested operations:

- `nika:github` (octocrab, ~800 LOC)
- `nika:aws` (aws-sdk-{s3,ec2,iam}, ~600 LOC)
- `nika:stripe` (async-stripe, ~400 LOC)
- `nika:cloudflare` (cloudflare-rs, ~500 LOC)
- `nika:slack` (slack-morphism, ~400 LOC)
- `nika:notion` (thin reqwest, ~400 LOC)
- `nika:vercel` (thin reqwest, ~500 LOC)

Packaged in 3 bundle crates: `nika-builtin-github`, `nika-builtin-cloud`,
`nika-builtin-workspace`.

### nika pck MVP — the Cargo of AI workflows

**Positioning**: NOT "the npm of AI". Opinionated, signed, versioned,
reproducible, eval-backed. Small sharp type system.

**Architecture**: git-repo registry (homebrew-tap pattern) + direct-from-GitHub
fallback (`nika pck install github.com/user/repo`). Zero central HTTP
infrastructure. PR-based publishing for quality gate.

**9 first-class content types** (classified by runtime maturity):

🥇 **Gold (runtime-complete)** — day-one public registry:
1. **workflow** — `.nika.yaml` DAG, the atomic unit
2. **skill** — markdown prompt (absorbs IDE rulesets via `targets:` field for
   Claude Code / Cursor / Windsurf / Zed)
3. **agent** — agent preset (verb + tools + completion + guardrails + budget)
4. **provider** — LLM provider + auth (absorbs Model presets via `[[models]]` inline)
5. **mcp** — alias → npm/pypi/oci server (Distribution enum)

🥈 **Beta (spec stable, runtime partial)** — `--experimental` flag:
6. **eval** — dataset + assertions (foundational trust layer)
7. **recipe** — meta-bundle (workflow + skills + providers + mcp + eval).
   The killer primitive. `create-react-app` for AI.
8. **shield** — security policy (trust/taint/spotlight/canary — Nika-unique moat)

🧪 **Experimental** — feature-flagged:
9. **lints** — dylint + workflow lint packs

**Crates** (~10k LOC):
- `nika-pck-manifest` (L0, ~1.5k LOC) — TOML types + validation
- `nika-pck-registry` (L1, ~2.5k LOC) — git clone via `gix`, index resolve
- `nika-pck-store` (L1, ~1.8k LOC) — `~/.nika/pkg/` layout, `pck.lock`
- `nika-pck` (L2, ~3k LOC) — install/publish/search/info/remove orchestration
- `nika-git` (L1, ~1.5k LOC) — gix wrapper, `GitClient` trait impl

**Commands**: `nika pck install <spec>`, `publish`, `search <query>`,
`info <name>`, `remove <name>`, `list`, `update`, `audit`.

**Extensibility**: `nika.*` core namespace reserved. Community uses `x-*` or
reverse-DNS (`com.acme.dashboard/v1`). Promotion path: `x-foo` → core requires
100+ installs + RFC + 2 Nika team reviewers.

### Sharing primitive: `nika init --from`

Git-native template scaffold. Zero registry dependency.

```bash
nika init --from github.com/user/awesome-workflow
```

Covers 80% of the "share my setup" use case. Pre-pck. Zero infrastructure.

### Forward-compatible kernel hooks (additive across all layer-phases)

**Kernel traits defined, mock impls only** — real implementations arrive per
layer-phase without touching already-admitted crates:

- `MemoryStore` trait — satellites land L1 phase 1e-1f
- `EmbeddingProvider` trait — `nika-embed` lands L1 phase 1e
- `ToolExecutor` trait — real impl lands L2 (native builtins via `nika-builtin`)
- `WasmPluginHost` trait — `nika-wasm-host` lands L3
- `MetricsExporter` + `TracerProvider` — `nika-observability-otel` lands L3/L4
  (unified `ObservabilitySink` dropped per Q12 rev.3)

`InferRequest` reserves `memory: Option<MemoryDirective>` (surfaced L1 memory),
`budget: Option<BudgetDirective>` (surfaced L2 runtime), `replay_seed: Option<u64>`
(surfaced L3 shield). All structs `#[non_exhaustive]` — future additions are
NOT breaking changes.

### Layer discipline (L0 → L4)

The 50-90 crates slot into a strict downward-only dependency stack.
Enforced by `[workspace.metadata.diamond.layers]` in `Cargo.toml` and
`scripts/ci/check-layering.sh` (shipped, hygiene vector 11).

| Layer | Role | Allowed I/O | Depends on |
|---|---|---|---|
| L0 | Pure types + lookup — zero I/O, sync only | none | (leaf) |
| L0.5 | Kernel trait definitions — async OK, zero impl | none (traits only) | L0 |
| L1 | Effect implementations — async, tokio-based | fs / net / process per declared axes | L0, L0.5 |
| L2 | Composition — pcks, verbs, policies, builtins | via L1 traits | L0, L0.5, L1 |
| L3 | Runtime + binaries — engine, daemon, cli | via L2 | L0..L2 |
| L4 | Community overlays — unstable, WASM plugins | gated via `unstable-*` | L0..L3 |

L0 and L0.5 are the context-window discipline backbone. `nika-error` and
`nika-catalog` (L0) plus `nika-kernel-*` (L0.5) stay small and sync-safe so
that L1 effect crates can be swapped without touching the lower tiers.

See `docs/architecture/crate-layer-registry.md` for the full ASCII map,
the allowed I/O axes per layer, and the enforcement anti-patterns.

## L1 memory phase (1e + 1f) — Hybrid ontology + auto-descriptive memory

**Theme**: build the memory subsystem as dedicated L1 satellites, exposed
through the `nika-connectome` L2 orchestrator (10 satellites · ADR-004). Each satellite is admitted through the 12 gates and
composable through a pluggable trait set in `nika-kernel`.

See the §Memory subsystem detail table above for the satellite
list, the 4 shipped storage backends, the hybrid trust model, and the
compile-time tenant isolation contract.

### Agent-v2 (builds during L2, blocked on memory L1)

- Parallel tool calls
- ReWOO planning
- Reflection loops
- Resume with state snapshots
- Context compression
- 4-type guardrails (length + schema + regex + LLM judge)

### pck full (builds during L2, blocked on L1 pck backend)

- sigstore keyless signing (mandatory for community publish)
- PubGrub semver resolver
- Automated PR flow for `nika pck publish`
- 3 beta types (eval/recipe/shield) graduate to gold
- Community registry seeded with 20-30 `supernovae/*` recipes

### Additional native adapters (builds during L2, 8 more)

`nika:linear`, `nika:huggingface`, `nika:ollama`, `nika:supabase`,
`nika:discord`, `nika:resend`, `nika:elevenlabs`, `nika:replicate`.

### Media subsystem (L1 side-branch, 5-crate split)

- `nika-media-cas` — content-addressed storage
- `nika-media-image` — thumbnail/convert/strip/optimize/phash
- `nika-media-pdf` — PDF text extraction
- `nika-media-document` — html_to_md, readability
- `nika-media-provenance` — C2PA

## L3 + L4 — Ecosystem (Phases 3 + 4)

- **WASM community plugins** (wasmtime + extism sandbox, L3) — third-party
  native builtins via `nika pck install builtin:...` with signed WASM blobs
- **Full observability** (L3/L4): OpenTelemetry, `tracing` spans, trace
  replay, budget enforcement, metrics (tokio-metrics integration)
- **LSP full** (L4): autocomplete, hover docs, go-to-definition, inline
  diagnostics
- **Scheduled triggers**: cron-like workflow scheduling
- **Keys subsystem**: macOS Keychain + Linux keyring + OAuth device flow
- **Public launch**: `nika.sh`, community registry promotion, blog launch

## Polish forever — post-1.0 (no layer climbs, no marketing events)

Incremental quality additions forever. Order determined by community pull +
Nika team priorities. Possible directions include (no commitment):

- **Hosted cloud runner** (`nika.sh` as optional SaaS) — self-host remains primary
- **Commercial relicense** (AGPL → commercial for enterprise customers)
- **Mobile SDK** (iOS/Android bindings to local Nika runtime)
- **RAG as composable primitive** (distinct from agent memory)
- **Enterprise features**: SSO, audit log retention, compliance dashboards
- **Performance**: hot-path optimization, zero-copy, SIMD for media
- **Security audits**: external review, fuzzing campaigns, supply chain
- **Additional native adapters**: ranked by usage telemetry
- **Graduate reserved memory satellites** (causal/episodic/working/meta/
  procedural/spatial) as usage patterns justify

## What we deliberately dropped (from legacy or never built)

Per ADR-037 there is **no defer**. Everything listed here is either deleted
outright or reclassified as a future layer-phase deliverable (no version tag).

| Item | Decision | Why |
|---|---|---|
| `nika-sdk` (remote client SDK) | DELETED | 0 consumers, speculative abstraction |
| `nika-napi` (Node.js bindings) | DELETED | W1 removed, rebuild as `@nika/client` TypeScript if needed |
| `nika-py` (Python bindings) | DELETED | W1 removed, no clear user pull |
| `nika-tui` (terminal UI) | DELETED → rebuilt later if pull justifies | W1 removed |
| `ProviderCategory` enum | DELETED | 11 ex-MCP providers migrated to `McpAlias` catalog |
| Agent-v2 (multi-turn, 4 guardrails) | **Built during L2** | blocked on memory L1 phase 1e-1f |
| The Connectome (1 orchestrator + 10 satellites) | **Built during L1 phases 1e-1f-1g** | ontology-graph + auto-descriptive, see §Memory subsystem detail |
| WASM host (wasmtime + extism) | **Built during L3** | Sandbox-3 triplet ships alongside |
| Keys subsystem (Keychain/OAuth) | **Built during L1 phases 1a-1b** | `nika-keys-env` + `nika-keys-keychain` |
| Hosted cloud runner | Polish forever (post-1.0) | Self-host is primary; SaaS optional |
| Commercial relicense (AGPL → commercial) | Polish forever (post-1.0) | Only when enterprise pull warrants legal setup |

## What this roadmap deliberately excludes (never)

- **Visual no-code editor** (anti-positioning — `.nika.yaml` is the user-facing format)
- **Chat UI / consumer assistant** (GPT Store territory)
- **Own model training infrastructure** (HuggingFace territory)

Nothing is forbidden forever. Items are re-evaluated each phase.

## Governance

Nika is open source under AGPL-3.0-or-later with commercial relicense
available. **Benevolent dictator model**: Thibaut Melen (SuperNovae Studio) is
the final arbiter on architecture, scope, and release. Community contributes
via pull request.

Trusted reviewers may be delegated as the community grows. No DAO, no voting.

## Release cadence

Per ADR-037 the cadence is **admission-driven**, not calendar-driven — and
per D-2026-06-20-N1 the tag scheme is **real semver** (`MAJOR.MINOR.PATCH` ·
the historical `v0.8X.Y` layer-phase scheme is retired).

- **Minor** — when a meaningful capability slice completes. No fixed
  interval; slices take what they take. Quality > speed.
- **Patch** — fixes on the released line.
- `main` moves to the next `-dev` version immediately after each release so
  contributor binaries cannot be confused with release assets.

## See also

- [`README.md`](README.md) — landing page · current state · quick-start
- [`DIAMOND.md`](DIAMOND.md) — strategy overview · 7 shadow zones
- [`docs/architecture/BLUEPRINT_2036.md`](docs/architecture/BLUEPRINT_2036.md)
  — **10-year horizon canon (v1.3) · LAYER 1 DECENNIAL anchor** ·
  ADR-037 count horizon (50-90 · cap 100) · 4-verb 2036 stress-test · 11/10 amplifiers ·
  ADR queue 050-056 + 060/062/064/066/070/071/072/073
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — 12-gate admission · contributor process
- [`SECURITY.md`](SECURITY.md) — vulnerability disclosure · 11-row defense layers + NIKA-390 queued
- [`CHANGELOG.md`](CHANGELOG.md) — release log · per-crate semver entries
- [`docs/adr/`](docs/adr/) — 38+ ADRs · architectural decisions canonical
- **Pre-release labels are reserved for the design-partner track**
  (`1.0.0-rc.N` · per D-2026-06-20-N1). The historical `v0.80.0-alpha.*`
  tags predate this policy. Every stable tag is diamond — if it isn't
  diamond, it doesn't get tagged.

## Architectural forward-compatibility guarantees

See `docs/architecture/forward-compat-invariants.md` for the full list of
patterns, decisions, and rules that ensure every admitted crate accommodates
L1 → L5 layers and post-1.0 polish **without breaking changes**.

🦋
