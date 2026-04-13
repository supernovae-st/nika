# Nika Invariants — architectural rules locked

## Crate structure

- Total target : **32-34 crates diamond** + 3 memory satellites (Phase 9+)
- Max LOC per crate : **15,000** (strict, enforced by xtask/CI)
- Max LOC per file : **1,500** (strict, CI blocks)
- Max lines per fn : **100** (warning, ≤100 preferred)

## Naming conventions (LOCKED)

- `nika-*` prefix for all workspace members
- `nika-verb-<verb>` for verb executors (exec, fetch, invoke, infer, agent)
- `nika-memory-*` for memory subsystem satellites
- `nika-<noun>` for effect impls (fs, http, blob, process, etc.)
- `nika-process` (not `nika-exec-runner`, not `nika-shell`)
- `nika-daemon-db` (not `nika-storage`)
- `nika-lints` (plural, dylint collection)

## Deleted / merged crates (post-audit 2026-04-13)

**Deleted entirely** :
- `nika-sdk` — 0 consumers, speculative
- `nika-napi` — already W1, never re-added
- `nika-py` — already W1, never re-added
- `nika-tui` — already W1, rebuild Act 3 or never

**Inlined into other crates** :
- `nika-macros` → inlined into `nika-event`
- `nika-lsp-core` → merged into `nika-lsp`

**Modules, not crates** (single consumer) :
- `nika-policy` → module in `nika-runtime`
- `nika-storage` → module in `nika-daemon`
- `nika-cache` → module in `nika-runtime`

## Split crates (heavy deps isolation)

**nika-media split × 5** (Option B aggressive) :
- `nika-media-cas` — CAS store, zero heavy deps
- `nika-media-image` — thumbnail/convert/strip/optimize/phash
- `nika-media-pdf` — pdf_extract
- `nika-media-document` — svg/chart/readability/html_to_md
- `nika-media-provenance` — c2pa/verify/qr

**nika-provider split × 3** :
- `nika-provider-rig` — 7 cloud + 7 OpenAI-compat via rig-core
- `nika-provider-native` — mistral.rs GGUF local, feature-gated
- `nika-provider-mock` — deterministic testing, zero external deps

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

These hooks allow Cortex + agent-v2 to land post-v0.90 **without breaking
change** to the public API.

## 28+ sacred invariants (inherited)

From main CLAUDE.md + deletion-first.md + honest-assessment.md :

- #11 : `Command::new` MUST set `kill_on_drop(true)`
- #12 : concurrent pipe reading MUST use `tokio::try_join!`
- #16 : `parking_lot::RwLockReadGuard` is !Send, never across `.await`
- #17 : unified `InferRequest` is the kernel's single LLM call shape
- #19 : per-crate `new()` constructors on all `#[non_exhaustive]` structs
- #23 : kernel-adjacent helpers use std / primitive / bytes::Bytes only
- #24 : EventKind emission from exactly 1 site per verb path
- #25 : all verb-crate error enums `#[non_exhaustive]` from day one
- #26 : one-headline-per-session
- #27 : Clock + ShellExecutor injection for test hermeticity
- #28 : every commit compiles + passes tests + clippy 0

And 17+ more from rust-analyzer adapted invariants in RUST_ENFORCEMENT.md §6.

## Timeline locked

- Phase 0 : 1 semaine ✓ DONE (2026-04-13)
- Phase 1 revised : 5-7 semaines (split nika-core)
- Phase 2 : 3-4 semaines (copy/rewrite 9 extract-ready)
- Phase 3 : 12-15 semaines (verbs + provider split + media split)
- Phase 4 : 8-10 semaines (runtime + daemon + interfaces + shadow zones)
- Phase 5 : 4 semaines (parity + shadow zone validation)
- Phase 6 : 2 semaines (cutover + tag v0.90)

**Total : 11-12 mois honnête.** Pas 14 semaines, pas 9 mois.

🦋
