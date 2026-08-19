# Nika Invariants — architectural rules locked

## Crate structure

- Crate count : **not the invariant** (ruled D-2026-07-21-N1). ADR-037 (accepted 2026-04-17) revised the stale 40-42 target to **50-90 · cap 100 unchanged** — a planning horizon, never a gate. The count invariants are **collapse-vs-publish** (below), the **layer contract** (`docs/architecture/crate-layer-registry.md`), and the **15k prod-LOC wall**; the live count is PROJECTED (`scripts/crate-metrics.sh` · hygiene vector 2), never hand-typed. + **11 Connectome crates** (1 L2 orchestrator `nika-connectome` + **10 L1 satellite crates** · autodesc split per ADR-042) — separate count, the **2.0** Connectome era. Satellites · `hnsw, bm25, rrf, rerank, fsrs, rdfs-reasoner, temporal, graph-algos, autodesc-minimal, autodesc-full`. Phase-M climb · cognition in the 2.0 era · specs at `docs/crate-specs/nika-<sat>.md`. See ADR-004 + ADR-042 + `BLUEPRINT_2036.md`.

## Collapse-vs-publish principle (decision RULE locked · cluster collapses = QUEUED proposals)

> ⚠️ Status precision (2026-06-11 coherence pass) · `BLUEPRINT_2036.md` is
> `status: proposal` (v1.6) and ADR-055/ADR-056 were never authored — the
> prior « LOCKED · post BLUEPRINT_2036 v1.0 » heading over-claimed. What IS
> locked: the decision RULE below + the per-crate granularity of publishable
> clusters. The pre-launch announce climb (amended D-2026-06-20-N1 · was
> "v0.81") ships **per-verb crates** (`nika-verb-{infer,exec,invoke,agent}`)
> per NIKA_ROADMAP S4 + D-2026-06-10-N6 ledger + crate-admission-order steps
> 9-15. The collapse may fire in a later 1.x minor IF ADR-055/056 get authored + accepted.

- **Internal-only clusters → collapse to one crate (QUEUED · not yet decided)** · `nika-effects` (was 5 effect crates · ADR-055 queued) · `nika-verbs` (was 4 verb crates · ADR-056 queued · 4-verb semantic lock preserved at module level · fetch is the `nika:fetch` builtin under invoke, not a verb crate) · `init+lints+catalog-verify` → `nika-cli` subcommands. Per ADR-006 monolithic-kernel-spirit applied across clusters.
- **Externally-publishable clusters → preserve per-crate granularity** · Connectome satellites (10 crates · ADR-004 publishable standalone on crates.io · external Rust RDF/ML ecosystem value) · provider variants (rig · native · mock · 3 crates · heavy-dep isolation). Per ADR-004 + ADR-042.
- **Decision rule** · « is there genuine external value to per-crate granularity? » YES → split · NO → collapse. Default is collapse (12-gate ceremony × N crates costs more than module discipline within 1 crate).
- Max LOC per crate : **15,000 prod LOC** (strict · enforced by CI `check-crate-size.sh` · scope = `src/` minus in-file `#[cfg(test)]` regions, `tests/`+`benches/` excluded — the mutation ratchet (≥90% killed) grows test mass by design and must not fight the size budget · semantics locked 2026-06-11 after nika-schema hit 16.2k total / 10.1k prod)
- Max LOC per file : **1,500** (strict, CI blocks)
- Max lines per fn : **100** (warning, ≤100 preferred)

## Naming conventions (LOCKED)

- `nika-*` prefix for all workspace members
- `nika-verb-<verb>` for verb executors (exec, invoke, infer, agent · fetch is the `nika:fetch` builtin reached via invoke, not a verb)
- Connectome satellites keep pro names (`nika-hnsw`, `nika-bm25`, … — NO `nika-memory-` prefix) <!-- stale-ok -->
- `nika-<noun>` for effect impls (fs, http, blob, etc.)
- `nika-exec-runner` — the shell/process effect (`nika-process` / `nika-shell` rejected names) <!-- stale-ok -->
- `nika-daemon-db` (not `nika-storage`)
- `nika-lints` (plural, dylint collection)

## Deleted / merged crates (post-audit 2026-04-13)

**Deleted entirely** :
- `nika-sdk` — 0 consumers, speculative
- `nika-napi` — already W1, never re-added
- `nika-py` — already W1, never re-added
- `nika-tui` — already W1, rebuild Act 3 or never

**Inlined into other crates** :
- `nika-macros` → REMOVED from plan (Q1 decision 2026-04-16: no proc macros in L0, manual impl + macro_rules!)
- `nika-lsp-core` → merged into `nika-lsp`

**Modules, not crates** (single consumer) :
- `nika-storage` → module in `nika-daemon`
- `nika-cache` → module in `nika-runtime`

## Split crates (heavy deps isolation)

**nika-media split × 5** (Option B aggressive) :
- `nika-media-cas` — CAS store, zero heavy deps
- `nika-media-image` — thumbnail/convert/strip/optimize/phash
- `nika-media-pdf` — pdf_extract
- `nika-media-document` — svg/chart/readability/html_to_md
- `nika-media-provenance` — c2pa/verify/qr

**providers** :
- `nika-providers` — ONE L1.5 crate · 14/14 canonical providers over 3 wire
  formats (anthropic · openai-compat ×12 · gemini) + in-crate mock · rig NOT
  carried — Diamond talks wire directly through the kernel http seam
  (D-2026-05-22-N17)
- `nika-infer-local` — native in-process backend (candle sidecar · ADR-091)

## Added in POST_AUDIT 2026-04-14 expansion (8 new crates · per the ADR-037 count horizon)

**Builtin bundles × 3** (native API adapters):
- `nika-builtin-github` — octocrab wrapper (~800 LOC)
- `nika-builtin-cloud` — aws/cloudflare/vercel/stripe (~2k LOC)
- `nika-builtin-workspace` — slack/notion (~1k LOC)

**pck (package manager) × 5** — package installer + registry:
- `nika-pck-manifest` — L0, manifest types (~1.5k LOC)
- `nika-pck-registry` — L1, git-repo registry (~2.5k LOC)
- `nika-pck-store` — L1, local store (~1.8k LOC)
- `nika-pck` — L2, orchestrator (~3k LOC)
- `nika-git` — L1, gix wrapper for pck (~1.5k LOC)

Count note (D-2026-07-21-N1): these 8 landed under the ADR-037 horizon (**50-90 · cap 100**) — the count is projected, never a gate.

## Kernel hooks Phase 0 (mandatory)

```rust
// nika-kernel/src/memory.rs
pub trait MemoryStore: Send + Sync { ... }
pub trait EmbeddingProvider: Send + Sync { ... }

// nika-kernel/src/tool_executor.rs
pub trait ToolExecutor: Send + Sync { ... }

// nika-kernel/src/provider.rs (ADD fields)
#[non_exhaustive]
pub struct InferRequest {
    // existing...
    pub memory: Option<MemoryDirective>,
}

#[non_exhaustive]
pub struct InferResponse {
    // existing...
    pub memory_frames: Vec<MemoryFrameRef>,
}
```

These hooks allow the Connectome + agent-v2 to land in the 2.0 era (amended
D-2026-06-20-N1 · was "post-v0.90") **without breaking change** to the public API.

## 28+ sacred invariants (inherited)

From main CLAUDE.md + deletion-first.md + honest-assessment.md :

- #11 <!-- INV-011 --> : `Command::new` MUST set `kill_on_drop(true)`
- #12 <!-- INV-012 --> : concurrent pipe reading MUST use `tokio::try_join!`
- #16 <!-- INV-016 --> : `parking_lot::RwLockReadGuard` is !Send, never across `.await`
- #17 <!-- INV-017 --> : unified `InferRequest` is the kernel's single LLM call shape
- #19 <!-- INV-019 --> : per-crate `new()` constructors on all `#[non_exhaustive]` structs
- #23 <!-- INV-023 --> : kernel-adjacent helpers use std / primitive / bytes::Bytes only
- #24 <!-- INV-024 --> : EventKind emission from exactly 1 site per verb path
- #25 <!-- INV-025 --> : all verb-crate error enums `#[non_exhaustive]` from day one
- #26 <!-- INV-026 --> : one-headline-per-session
- #27 <!-- INV-027 --> : Clock + ShellExecutor injection for test hermeticity
- #28 <!-- INV-028 --> : every commit compiles + passes tests + clippy 0
- #29 <!-- INV-029 --> : every crate with concurrent primitives ships Loom tests (ADR-013)

And 17+ more from rust-analyzer adapted invariants in RUST_ENFORCEMENT.md §6.

## Timeline (real semver toward 1.0 · no hard per-phase deadlines)

Real semver toward a **1.0** public launch (amended D-2026-06-20-N1 · was
"forever-v0.x — no v1.0 target"). The engine's current release is whatever the
CHANGELOG top names (never quote a number here · it rots · `main` carries the
next dev version); the **1.0.0** launch ships when the 7 shadow zones are green, and
the remaining crates land additively across the 1.x minors under the ADR-037
count horizon (50-90 · cap 100 · projected, never a gate · ruled D-2026-07-21-N1). The nine-key LANGUAGE envelope (`nika: <id>` ·
model · inputs · const · secrets · permits · run · tasks · outputs · since 0.109) is frozen forever
(orthogonal to the engine version).

No hard per-phase deadlines — quality > speed. The pace is self-paced, not a
commitment. Current phase progress tracked in STATE.md (memory/) and
MEGA_HANDOFF_PHASE_D.md.

🦋
