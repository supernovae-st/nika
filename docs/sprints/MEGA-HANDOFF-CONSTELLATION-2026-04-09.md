# 🌌 MEGA HANDOFF — Constellation Refactor (Sessions 8 → Launch)

> **Self-contained reference. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
>
> **Philosophy:** `perfection > timing`. No "acceptable for launch", no "stretch". Launch date
> follows the work, not the other way around. 5 verbs are sacred. AGPL-3.0-or-later on all Nika crates.
>
> **Last updated:** 2026-04-09 (HEAD `e1fbbf35b`)
>
> **This document replaces and supersedes:**
> - `HANDOFF-CONSTELLATION-SESSION8-2026-04-09.md` (Session 8 detail — still authoritative for commit-level steps, reference it from Part V)
> - Informal memory notes in `~/.claude/.../memory/project_constellation_session*.md`

---

## TABLE OF CONTENTS

```
PART I   — WHERE WE ARE (verified ground truth)
  1. Quick state snapshot
  2. Session 7 retrospective (what got delivered)
  3. Phase 12 progress (34/63 tools)
  4. Full crate inventory (26 crates with LOC)
  5. Test count by crate (10,854 total)
  6. Unwrap count baseline (Phase 21 tracking)

PART II  — ARCHITECTURE REFERENCE
  7. Diamond layering diagram
  8. Media pipeline diagram
  9. Dependency graph (cargo deps)
  10. The KernelToolAdapter pattern
  11. The task_local! + Shield pattern

PART III — VERIFIED CODEBASE SNAPSHOTS (file:line references)
  12. nika-kernel (L0.5) — traits + types
  13. nika-builtin (L2) — 34/63 tools inventory
  14. nika-media (L2) — 29 files, feature matrix
  15. nika-engine (L2) — monolith, extraction targets
  16. MediaContext trait (current thin state)
  17. MediaToolContext concrete struct
  18. MediaOp trait signature
  19. BuiltinError variants (8)
  20. MediaToolError variants (8, NIKA-290..297)
  21. task_local cells + accessors (6+6)

PART IV  — ARCHITECTURAL DECISIONS (rationale)
  22. Why compute_blocking is NOT on MediaContext (object safety)
  23. Why MediaOp impls stay in nika-media (not moved to nika-builtin)
  24. Why builder pattern, not Bundle struct
  25. Why 12.11 is one commit, not split a/b
  26. Why 12.13 deletes with_all_tools (zero users rule)

PART V   — SESSION 8 EXECUTION PLAN (pointer to detailed doc)
  27. Commits 12.9 → 12.9b → 12.10 → 12.11 → 12.11c → 12.12 → 12.13
  28. Test fixtures (§4.0 of detail doc)
  29. Test cases (§4.1-4.5, 26 tests)

PART VI  — PHASES 13-23 FORECAST (post-S8 roadmap)
  30. Phase 13 — verb crates (infer/exec/fetch/invoke/agent)
  31. Phase 14 — nika-runtime + cache + RunContext splinter
  32. Phase 6  — error_domains promotion (180 call sites)
  33. Phase 7  — LSP absorption
  34. Phase 17 — nika-tui split (core/widgets/views/app)
  35. Phase 19 — type system hardening
  36. Phase 21 — zero-unwrap migration (4,263 → <50)
  37. Phase 22 — PGO + binary size
  38. Phase 23 — blake3 AST cache (200ms → <5ms)
  39. V2.3 firm targets table

PART VII — INVARIANTS (NEVER break these)
PART VIII — KNOWN DEBT / DEFERRED (do NOT fix blindly)
PART IX  — SKILLS + TOOLS LIBRARY (every skill relevant, with triggers)
PART X   — MIGRATION PATTERNS LIBRARY (3 patterns)
PART XI  — VERIFICATION CHECKLIST (pre-flight)
PART XII — ROLLBACK STRATEGY
PART XIII — COMMAND REFERENCE CARD
PART XIV — APPENDICES (links, ADRs, error codes, memory files)
```

---

# PART I — WHERE WE ARE

## 1. Quick state snapshot (verified 2026-04-09)

| Metric | Value | Source |
|--------|-------|--------|
| Git HEAD | `e1fbbf35b` | `git log -1 --oneline` |
| Branch | `main` (clean) | `git status` |
| Version | `v0.79.x` | `tools/nika/Cargo.toml` |
| Crates | **26** workspace members | `cargo metadata` |
| Total tests | **10,854** | `cargo test --workspace --lib` |
| Clippy warnings | **0** | `cargo clippy --workspace --lib -- -D warnings` |
| Phase 12 tools migrated | **34/63** (54%) | `nika-builtin/src/` count |
| nika-engine LOC | **157,746** | `wc -l nika-engine/src/**/*.rs` |
| nika-engine target (launch) | **≤100,000** | V2.3 aggressive targets doc |
| Unwrap/expect in prod code | **4,263** | grep across 7 crates |
| Unwrap target (launch) | **<50 with `// REASON:`** | Phase 21 |

## 2. Session 7 retrospective — what got delivered (5 commits, HEAD-9 → HEAD-5)

```
44ee5af24 refactor(builtin): migrate PromptTool to nika-builtin via HitlPrompt bridge (12.8)
ea3fd1073 refactor(kernel): move task_local declarations to nika-kernel/src/task_local.rs (12.6-pre)
acd68c18e refactor(builtin): migrate 5 file tools to nika-builtin with Shield integration (12.6)
d8a78a2af refactor(builtin): migrate RunTool to nika-builtin via RunSpec + EngineRunExecutor (12.7)
574650008 docs(builtin): document introspection tool deferral in router (commit 12.5)
```

**Post-S7 follow-up commits:**
```
dbe702e77 fix(builtin): file tools resolve relative paths correctly via CURRENT_WORKING_DIR
c5b0fd566 fix(builtin): tree_data uses extract_field for dot-path group_by
c64d1187c chore(deps): bump rig-core 0.33 → 0.34
f5d89db8b feat(skills): add 5 new workflow authoring skills
261b74122 chore(claude): remove broken .claude symlink pointing to private dx
```

**Architectural artifacts added in S7:**
- `nika-kernel/src/task_local.rs` — 6 task_locals + 6 accessor functions
- `nika-engine/src/runtime/hitl_bridge.rs` — `HitlBridge` (HitlHandler → HitlPrompt trait)
- `nika-engine/src/runtime/run_executor.rs` — `EngineRunExecutor` (impl RunExecutor with task_local scoping)
- `nika-builtin/src/prompt.rs` — `PromptTool` (headless + handler modes)
- `nika-builtin/src/file/` — 8 files: `read.rs`, `write.rs`, `edit.rs`, `glob.rs`, `grep.rs`, `context.rs`, `shield.rs`, `mod.rs`
- `nika-builtin/src/run_tool.rs` — `KernelRunTool` backed by `RunExecutor` trait
- `nika-kernel/src/scope.rs` — `RunSpec` struct + updated `RunExecutor` trait signature

## 3. Phase 12 progress — 34/63 tools migrated (54%)

**In nika-builtin today (34 tools):**
- **Core (7)**: sleep, log, emit, assert, complete, prompt, run
- **Data (13)**: jq, map, filter, group_by, chunk, token_count, enrich, zip, set_diff, json_merge, json_diff, tree_data, inject
- **Data Sprint 2 (6)**: json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten
- **Introspection (3)**: cost, dag_info, threads
- **File (5)**: read, write, edit, glob, grep (via `file/` subdir with Shield)

**Still in nika-engine (29 remaining):**
- **Media (24)**: impls live in nika-media already, adapter in engine — Session 8 refactors via `EngineMediaContext`
- **Fetch (1)**: `nika:fetch` — stays in engine (SSRF + 9 extract modes coupled)
- **Introspection (3)**: task_status, records, orchestrate — DEFERRED pending `RecordView` DTO
- **Engine-specific**: engine-side copy of PromptTool for rig_agent_loop (different code path — stays)

## 4. Full crate inventory — 26 workspace members with verified LOC

| # | Crate | Layer | LOC | Files | Tests | Role |
|---|-------|-------|-----|-------|-------|------|
| 1 | `nika-core` | L0 | 38,403 | 72 | 1,282 | Pure types, AST, catalogs, policy, trust |
| 2 | `nika-kernel` | L0.5 | 1,268 | 11 | 24 | Effect traits + task_local (zero I/O) |
| 3 | `nika-kernel-mock` | L0.5 | 712 | 6 | 23 | Hand-written test mocks for kernel traits |
| 4 | `nika-clock` | L1 | 85 | 1 | 5 | `SystemClock` impl (ZST) |
| 5 | `nika-fs` | L1 | 262 | 1 | 13 | `TokioFs` impl |
| 6 | `nika-blob` | L1 | 342 | 1 | 13 | `DiskBlobStore` (blake3 CAS) |
| 7 | `nika-http` | L1 | 384 | 2 | 16 | `ReqwestClient` (SSRF guards) |
| 8 | `nika-exec-runner` | L1 | 692 | 2 | 28 | `TokioShell` (command blocklist) |
| 9 | `nika-event` | L1 | 5,703 | 6 | 155 | EventLog + EventEmitter blanket |
| 10 | `nika-macros` | L1 | 667 | 4 | 0 | 3 derives + 1 attr macro (built-in) |
| 11 | `nika-builtin` | L2 | **9,306** | 32 | **250** | 34/63 builtin tools (Phase 12) |
| 12 | `nika-media` | L2 | 14,199 | 35 | 377 | CAS store, image/doc processing (29 MediaOp impls) |
| 13 | `nika-mcp` | L2 | 9,210 | 13 | ? | MCP client (rmcp) |
| 14 | `nika-engine` | L2 | **157,746** | 195 | **4,626** | Monolith — extraction target ≤100k |
| 15 | `nika-vault` | L2 | 1,333 | 1 | ? | Encrypted secrets (XChaCha20 + Argon2i) |
| 16 | `nika-storage` | L2 | 4,017 | 2 | ? | Storage backends |
| 17 | `nika-display` | L2 | 13,364 | 19 | 313 | CLI renderers (Renderer trait) |
| 18 | `nika-lsp-core` | L2 | 11,885 | 23 | 388 | LSP intelligence (pure functions) |
| 19 | `nika-daemon` | L3 | 7,115 | 16 | 161 | Background daemon IPC |
| 20 | `nika-cli` | L4 | 27,954 | 45 | 458 | CLI subcommand implementations |
| 21 | `nika-tui` | L4 | 88,989 | 220 | 2,155 | TUI (Studio + Command + Control) |
| 22 | `nika-serve` | L4 | 7,040 | 18 | ? | HTTP server mode |
| 23 | `nika-lsp` | L4 | 3,503 | 10 | ? | LSP binary wrapper |
| 24 | `nika-sdk` | L4 | 2,541 | 9 | ? | Public SDK surface |
| 25 | `nika-init` | L4 | 21,049 | 23 | ? | Project scaffolding + course |
| 26 | `nika` | L5 | 2,932 | 4 | ? | Binary entry point (target <900) |

**Total LOC (excluding tests dir):** ~430,000
**Total tests:** 10,854

## 5. Unwrap/expect baseline (Phase 21 tracking)

| Crate | Unwrap+Expect count |
|-------|---------------------|
| nika-kernel | **0** ✅ (clean — new crate) |
| nika-engine | 2,148 |
| nika-core | 630 |
| nika-media | 563 |
| nika-cli | 332 |
| nika-builtin | 320 |
| nika-tui | 270 |
| **Total** | **4,263** |

> **Phase 21 target:** <50 with `// REASON:` comments explaining each one.
> Most are in tests — production hot-path count is probably ~1,500.
> CI ratchet from Day 1 of Phase 21 blocks regressions.

## 6. Session 7 fixes since S7 ended (3 follow-ups)

```
dbe702e77  file tools: resolve relative paths via CURRENT_WORKING_DIR task_local
c5b0fd566  tree_data: use extract_field for dot-path group_by (duplicate of my P2 fix)
c64d1187c  bump rig-core 0.33 → 0.34
```

**Pre-S7 review fixes (10 commits I pushed before this mega handoff):**
4 P0 + 10 P1 + 5 P2 bugs across `complete.rs`, `inject.rs`, `jq.rs`, `set_diff.rs`, `json_diff.rs`,
`text.rs`, `sleep.rs`, `threads.rs`, `yaml_validate.rs`, `json_transform.rs`, `error.rs`,
`aggregate.rs`, `router.rs`, `CLAUDE.md`. Full context: see git log `bcba44fec`..`6e1e3fe06`.

---

# PART II — ARCHITECTURE REFERENCE

## 7. Diamond layering diagram

```
                          ┌─────────────────────────┐
                          │   L5: nika (binary)     │  <900 LOC target
                          │   2,932 LOC             │
                          └───────────┬─────────────┘
                                      │
                 ┌────────────────────┼────────────────────┐
                 │                    │                    │
         ┌───────▼────────┐  ┌────────▼────────┐  ┌───────▼────────┐
         │  L4: nika-cli  │  │  L4: nika-tui   │  │ L4: nika-serve │
         │    27,954      │  │    88,989       │  │    7,040       │
         └───────┬────────┘  └────────┬────────┘  └───────┬────────┘
                 │                    │                    │
         ┌───────▼────────┐  ┌────────▼────────┐  ┌───────▼────────┐
         │  L4: nika-lsp  │  │  L4: nika-init  │  │  L4: nika-sdk  │
         └────────────────┘  └─────────────────┘  └────────────────┘
                 │                    │                    │
                 └────────────────────┼────────────────────┘
                                      │
                          ┌───────────▼─────────────┐
                          │  L3: nika-daemon        │
                          │    7,115                │
                          └───────────┬─────────────┘
                                      │
   ┌──────────┬─────────┬─────────────┼────────────┬────────────┬──────────┐
   │          │         │             │            │            │          │
┌──▼────┐ ┌──▼────┐ ┌──▼────┐ ┌──────▼──────┐ ┌──▼──────┐ ┌──▼────┐ ┌──▼────┐
│L2:    │ │L2:    │ │L2:    │ │L2:          │ │L2:      │ │L2:    │ │L2:    │
│engine │ │builtin│ │media  │ │mcp+vault+   │ │display  │ │lsp-   │ │storage│
│157,746│ │ 9,306 │ │14,199 │ │ storage     │ │13,364   │ │core   │ │ 4,017 │
│       │ │       │ │       │ │             │ │         │ │11,885 │ │       │
└───┬───┘ └───┬───┘ └───┬───┘ └──────┬──────┘ └────┬────┘ └───┬───┘ └───┬───┘
    │         │         │            │             │          │         │
    └─────────┴─────────┴────────────┼─────────────┴──────────┴─────────┘
                                     │
        ┌────────┬──────────┬────────┼─────────┬───────────┬──────────┐
        │        │          │        │         │           │          │
  ┌─────▼───┐ ┌──▼──┐ ┌────▼────┐ ┌─▼───┐ ┌───▼────┐ ┌────▼────┐ ┌──▼─────┐
  │L1:clock │ │L1:fs│ │L1:blob  │ │http │ │exec-run│ │event    │ │macros  │
  │   85    │ │ 262 │ │   342   │ │ 384 │ │  692   │ │ 5,703   │ │  667   │
  └─────────┘ └─────┘ └─────────┘ └─────┘ └────────┘ └─────────┘ └────────┘
        │        │          │        │         │           │
        └────────┴──────────┴────────┼─────────┴───────────┘
                                     │
                          ┌──────────▼──────────┐
                          │  L0.5: nika-kernel  │  1,268 LOC (TRAITS ONLY)
                          │  + task_local.rs    │  ─ Provider, Fs, Clock,
                          │  + kernel-mock      │    Shell, Http, BlobStore,
                          │                     │    MediaContext, HitlPrompt,
                          │                     │    RunExecutor, BuiltinTool
                          └──────────┬──────────┘
                                     │
                          ┌──────────▼──────────┐
                          │    L0: nika-core    │  38,403 LOC (pure types)
                          │                     │  AST, catalogs, policy,
                          │                     │  trust, capabilities
                          └─────────────────────┘
```

**Rule:** Each layer depends downward only. L5 → L4 → L3 → L2 → L1 → L0.5 → L0. No sideways or upward deps.

## 8. Media pipeline diagram (Session 8 target state)

```
┌────────────────────────────────────────────────────────────────────────┐
│                    nika-engine (L2)                                    │
│                                                                        │
│  runtime/media_context.rs         runtime/builtin/media/mod.rs         │
│  ┌──────────────────────────┐    ┌──────────────────────────────────┐  │
│  │ pub struct               │    │ pub(crate) struct                │  │
│  │   EngineMediaContext {   │    │   MediaToolAdapter {             │  │
│  │     inner:               │◄───┤     op: Arc<dyn MediaOp>,        │  │
│  │       Arc<MediaTool…>    │    │     concrete_ctx: Arc<MediaTool…>│  │
│  │   }                      │    │     trait_ctx: Arc<dyn MediaCtx> │  │
│  │                          │    │     name, timeout                │  │
│  │ impl MediaContext for .. │    │   }                              │  │
│  │ impl BlobStore shim      │    │                                  │  │
│  └────────┬─────────────────┘    │ impl BuiltinTool for ...         │  │
│           │                      │   → op.execute(args, &concrete)  │  │
│           │                      └──────────────────────────────────┘  │
│           │                                                            │
│           │     runtime/builtin/router.rs                              │
│           │     ┌───────────────────────────────────────────────────┐  │
│           └────►│ .with_media(Arc<dyn MediaContext>)                │  │
│                 │ .with_file_tools(FileToolContext)                 │  │
│                 │ .with_hitl(Arc<dyn HitlHandler>)                  │  │
│                 │ .with_run(Arc<dyn RunExecutor>)                   │  │
│                 │ .with_cost_tool(EventLog)                         │  │
│                 │ .with_introspection(EventLog, RunContext)         │  │
│                 └───────────────────────────────────────────────────┘  │
└─────────────┬──────────────────────────────────────────────────────────┘
              │ uses
              ▼
┌────────────────────────────────────────────────────────────────────────┐
│                       nika-media (L2)                                  │
│                                                                        │
│  tools/context.rs                    tools/mod.rs                      │
│  ┌────────────────────────────────┐  ┌────────────────────────────┐    │
│  │ pub struct MediaToolContext {  │  │ pub trait MediaOp: Send    │    │
│  │   pub cas: CasStore,           │  │                    + Sync {│    │
│  │   pub budget: Arc<MediaBudget>,│  │   fn name(&self) -> &str;  │    │
│  │   pub compute: Arc<ComputePool>│  │   fn execute<'a>(          │    │
│  │   pub working_memory:          │  │     &'a self,              │    │
│  │     Arc<WorkingMemoryBudget>,  │  │     args: Value,           │    │
│  │   pub cancel: CancelToken,     │  │     ctx: &'a               │    │
│  │   pub working_dir:             │  │       MediaToolContext,    │    │
│  │     Option<PathBuf>,           │  │   ) -> Pin<Box<dyn Future  │    │
│  │ }                              │  │     <Output = ... >>>;     │    │
│  └────────────────────────────────┘  │ }                          │    │
│                                      └────────────────────────────┘    │
│  tools/import.rs       tools/thumbnail.rs    tools/dimensions.rs       │
│  tools/decode.rs       tools/convert.rs       tools/thumbhash_tool.rs  │
│  tools/metadata.rs     tools/optimize.rs      tools/color.rs           │
│  tools/svg.rs          tools/chart.rs         tools/phash.rs           │
│  tools/compare.rs      tools/pdf.rs           tools/provenance.rs      │
│  tools/verify.rs       tools/qr.rs            tools/quality.rs         │
│  tools/strip.rs        tools/html_to_md.rs    tools/css_select.rs      │
│  tools/extract_links.rs tools/extract_metadata.rs tools/readability.rs │
│  tools/pipeline.rs     tools/safety.rs                                 │
│  (29 files total)                                                      │
└─────────────┬──────────────────────────────────────────────────────────┘
              │ uses trait
              ▼
┌────────────────────────────────────────────────────────────────────────┐
│                     nika-kernel (L0.5)                                 │
│                                                                        │
│  scope.rs (line 170-176 current, full API after 12.9):                 │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │ #[async_trait::async_trait]                                     │   │
│  │ pub trait MediaContext: Send + Sync {                           │   │
│  │   async fn read_blob(&self, hash: &str)                         │   │
│  │     -> Result<Vec<u8>, BuiltinError>;                           │   │
│  │   async fn store_blob(&self, data: &[u8], task_id: &str)        │   │
│  │     -> Result<BlobStoreResult, BuiltinError>;                   │   │
│  │   fn working_dir(&self) -> Option<&Path>;                       │   │
│  │   fn is_cancelled(&self) -> bool;                               │   │
│  │   fn blob_store(&self) -> &dyn BlobStore;                       │   │
│  │   // NO compute_blocking<F, T> — not object-safe                │   │
│  │ }                                                               │   │
│  │                                                                 │   │
│  │ pub struct BlobStoreResult {                                    │   │
│  │   pub hash: String,                                             │   │
│  │   pub size: u64,                                                │   │
│  │   pub deduplicated: bool,                                       │   │
│  │ }                                                               │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

## 9. nika-engine's actual dependencies (verified from Cargo.toml)

```
nika-engine depends on:
  ├── nika-core      (L0)   — types, AST, catalogs
  ├── nika-kernel    (L0.5) — traits + task_local
  ├── nika-macros    (L1)   — derives + #[builtin_tool]
  ├── nika-display   (L2)   — renderers
  ├── nika-event     (L1)   — EventLog
  ├── nika-mcp       (L2)   — MCP client
  ├── nika-media     (L2)   — MediaOp impls (for adapter)
  ├── nika-builtin   (L2)   — BuiltinTool impls
  ├── nika-vault     (L2)   — secrets
  ├── nika-lsp-core  (L2)   — optional, feature="lsp"
  └── nika-daemon    (L3)   — IPC
```

```
nika-builtin depends on:
  ├── nika-core   (L0)   — Value, RunStats, DagInfo
  ├── nika-event  (L1)   — EventLog (for cost/dag_info/threads)
  └── nika-kernel (L0.5) — BuiltinTool trait + BuiltinError + task_local
```

**Key invariant:** `nika-builtin` does NOT depend on `nika-engine`, `nika-media`, `nika-mcp`.
It can be consumed by future verb crates (Phase 13) without pulling engine internals.

## 10. The `KernelToolAdapter` pattern (established in S7)

```rust
// nika-kernel/src/builtin.rs — sealed trait, BuiltinError
pub trait BuiltinTool: __sealed::Sealed + Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn call<'a>(&'a self, args: String)
        -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>>;
}

// nika-builtin/src/sleep.rs (example) — returns BuiltinError
pub struct SleepTool;
impl __sealed::Sealed for SleepTool {}
impl BuiltinTool for SleepTool { ... }

// nika-engine/src/runtime/builtin/adapter.rs — bridges to NikaError
pub struct KernelToolAdapter<T: nika_kernel::builtin::BuiltinTool>(pub T);
impl<T> EngineBuiltinTool for KernelToolAdapter<T>  // engine trait, NikaError
where T: nika_kernel::builtin::BuiltinTool {
    fn call<'a>(&'a self, args: String) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            self.0.call(args).await.map_err(NikaError::from)  // From<BuiltinError> for NikaError
        })
    }
}

// Router registration (nika-engine/src/runtime/builtin/router.rs:61):
tools.insert("sleep", Arc::new(KernelToolAdapter(SleepTool)));
```

## 11. The `task_local!` + Shield pattern

```rust
// nika-kernel/src/task_local.rs (added in S7 commit 12.6-pre)
tokio::task_local! {
    pub static WORKFLOW_DEPTH: Cell<u32>;
    pub static CURRENT_TASK_ID: Option<Arc<str>>;
    pub static CURRENT_TASK_TRUST: TrustLevel;
    pub static CURRENT_TASK_ELEVATED: bool;
    pub static PARENT_CHAIN: Vec<PathBuf>;
    pub static CURRENT_WORKING_DIR: Option<PathBuf>;  // added post-S7
}

pub fn current_task_trust() -> TrustLevel { ... }
pub fn current_task_elevated() -> bool { ... }
pub fn current_task_id() -> Option<Arc<str>> { ... }
pub fn current_depth() -> u32 { ... }
pub fn current_parent_chain() -> Vec<PathBuf> { ... }
pub fn current_working_dir() -> Option<PathBuf> { ... }
```

**Rule 1:** Trust is set ONCE by the runner before calling any tool. Never passed as argument.

**Rule 2:** `tokio::task_local!` is NOT visible inside rayon closures. To propagate trust into
a `compute.compute(|| ...)` call, capture the value on the tokio side:
```rust
let trust = current_task_trust();  // read on tokio thread
ctx.compute.compute(move || {
    // trust is now usable inside the rayon closure
    if !trust.is_trusted() { ... }
}).await??
```

**Rule 3:** `check_path_readable` in `nika-builtin/src/file/shield.rs` calls
`current_task_trust()` + `current_task_elevated()` to decide whether an untrusted agent
can read sensitive files (`nika.toml`, `.mcp.json`, `.env*`, `*.nika.yaml`).

---

# PART III — VERIFIED CODEBASE SNAPSHOTS (file:line references)

## 12. `nika-kernel` — 1,268 LOC across 11 files

```
tools/nika-kernel/src/
  ├── lib.rs           — re-exports
  ├── builtin.rs       — BuiltinTool sealed trait + BuiltinError (8 variants) + KernelToolAdapter
  ├── scope.rs         — BindingScope, MediaStaging, RecordStore, RunExecutor, HitlPrompt, MediaContext
  ├── task_local.rs    — 6 task_local! cells + accessor functions (added S7)
  ├── clock.rs         — Clock trait
  ├── events.rs        — EventEmitter trait
  ├── filesystem.rs    — Filesystem trait
  ├── http.rs          — HttpClient trait + SSRF guards
  ├── provider.rs      — Provider trait (LLM)
  ├── shell.rs         — ShellExecutor trait
  └── store.rs         — BlobStore trait
```

## 13. `nika-builtin` — 9,306 LOC across 32 files

```
tools/nika-builtin/src/
  ├── lib.rs                    — re-exports all tools
  ├── CLAUDE.md (project root)  — reference doc
  ├── sleep.rs    log.rs   emit.rs   assert.rs   complete.rs           ← Core (5)
  ├── prompt.rs   run_tool.rs                                           ← Core (2, added S7)
  ├── cost.rs                                                           ← Introspection (1)
  ├── introspect_dag.rs   introspect_threads.rs                         ← Introspection (2)
  ├── aggregate.rs   json_verify.rs   yaml_validate.rs                  ← Data Sprint 2 (3)
  ├── locale_lookup.rs   json_transform.rs                              ← Data Sprint 2 (2 files, 3 tools)
  ├── data/
  │   ├── mod.rs
  │   ├── jq.rs          ← jq (1 tool, uses jaq-core)
  │   ├── transform.rs   ← map, filter, group_by, chunk (4 tools)
  │   ├── merge.rs       ← zip, set_diff, json_merge (3 tools)
  │   ├── json_diff.rs   ← json_diff (1 tool)
  │   ├── aggregate.rs   ← tree_data (1 tool)
  │   ├── text.rs        ← token_count, enrich (2 tools)
  │   └── io.rs          ← inject (1 tool)
  └── file/                                                             ← File (5 tools, added S7)
      ├── mod.rs
      ├── context.rs     ← FileToolContext (replaces ToolContext for file tools)
      ├── shield.rs      ← check_path_readable (reads task_locals)
      ├── read.rs        write.rs    edit.rs    glob.rs    grep.rs
```

**Total tools in nika-builtin:** 34 (5 core + 2 core-s7 + 3 introspection + 13 data + 6 sprint2 + 5 file)

## 14. `nika-media` — 14,199 LOC across 35 files (29 tool files)

```
tools/nika-media/src/tools/
  ├── mod.rs          — MediaOp trait definition (line 78-84)
  ├── context.rs      — MediaToolContext + ComputePool + WorkingMemoryBudget (line 23-37)
  ├── error.rs        — MediaToolError (8 variants, NIKA-290..297)
  ├── safety.rs       — decode_image_safe, path validation helpers
  │
  ├── import.rs       — nika:import         Tier 1 (always-on)
  ├── decode.rs       — nika:decode         Tier 1 (always-on)
  ├── dimensions.rs   — nika:dimensions     Tier 1 (always-on)
  ├── thumbhash_tool.rs — nika:thumbhash    Tier 1 (always-on)
  ├── color.rs        — nika:dominant_color Tier 1 (always-on)
  │
  ├── thumbnail.rs    — nika:thumbnail      feature=media-thumbnail
  ├── convert.rs      — nika:convert        feature=media-thumbnail
  ├── strip.rs        — nika:strip          feature=media-thumbnail
  ├── metadata.rs     — nika:metadata       feature=media-metadata
  ├── optimize.rs     — nika:optimize       feature=media-optimize
  ├── svg.rs          — nika:svg_render     feature=media-svg
  │
  ├── chart.rs        — nika:chart          feature=media-chart
  ├── phash.rs        — nika:phash          feature=media-phash
  ├── compare.rs      — nika:compare        feature=media-phash
  ├── pdf.rs          — nika:pdf_extract    feature=media-pdf
  ├── provenance.rs   — nika:provenance     feature=media-provenance
  ├── verify.rs       — nika:verify         feature=media-provenance
  ├── qr.rs           — nika:qr_validate    feature=media-qr
  ├── quality.rs      — nika:quality        feature=media-iqa
  ├── pipeline.rs     — nika:pipeline       always-on (orchestrator)
  │
  ├── html_to_md.rs   — nika:html_to_md     feature=fetch-markdown
  ├── css_select.rs   — nika:css_select     feature=fetch-html
  ├── extract_links.rs — nika:extract_links feature=fetch-html
  ├── extract_metadata.rs — nika:extract_metadata feature=fetch-html
  └── readability.rs  — nika:readability    feature=fetch-article
```

**Feature flags (verified `nika-media/Cargo.toml`):**

| Feature | Enables | Deps pulled |
|---------|---------|-------------|
| `media-compression` | zstd | zstd |
| `media-core` (meta) | thumbnail+metadata+optimize+svg | cascade |
| `media-thumbnail` | thumbnail, convert, strip | fast_image_resize, image |
| `media-metadata` | metadata | nom-exif, lofty |
| `media-optimize` | optimize | oxipng |
| `media-svg` | svg_render | resvg, usvg, tiny-skia, fontdb |
| `media-phash` | phash, compare | image_hasher, image |
| `media-pdf` | pdf_extract | pdf-extract |
| `media-chart` | chart | charts-rs |
| `media-provenance` | provenance, verify | c2pa |
| `media-qr` | qr_validate | qrcode-ai-scanner-core, image |
| `media-iqa` | quality | dssim-core, rgb, image |
| `fetch-html` | css_select, extract_links, extract_metadata | scraper, psl |
| `fetch-markdown` | html_to_md | htmd |
| `fetch-article` | readability | dom_smoothie, scraper |
| `fetch-feed` | (parser only) | feed-rs |
| `fetch-extract` (meta) | fetch-html + fetch-markdown | cascade |

**Default features:** `media-compression, media-core, fetch-extract, fetch-article, fetch-feed, media-chart, media-phash, media-pdf, media-iqa, media-qr`

## 15. `nika-engine` — 157,746 LOC (THE monolith)

**Extraction targets (Phase 14 will split further):**

```
nika-engine/src/
  ├── runtime/           ~60k  — Runner, executor/, builtin/, shield/
  ├── ast/               ~22k  — lower.rs, action.rs, agent.rs, ...
  ├── binding/           ~11k  — template/, resolve.rs, jsonpath.rs, ...
  ├── provider/          ~10k  — rig/, native/, endpoints.rs, cost.rs
  ├── dag/               ~4.6k — flow.rs, indexed.rs, stable.rs, validate.rs
  ├── lsp/               ~12k  — opt-in, absorb into nika-lsp-core (Phase 7)
  ├── tools/             ~4.3k — legacy file tool scaffolding (delete in 12.13)
  ├── io/                ~2.7k — atomic file I/O
  ├── registry/          ~2.9k — MARKED FOR REMOVAL (nuke commit)
  ├── new/               ~2.6k — `nika new` scaffolding
  ├── media/             ~4.9k — media E2E tests (stays)
  ├── error.rs           ~2.9k — NikaError + codes
  ├── error_domains.rs   ~250  — domain sub-enums (Phase 6 target)
  ├── config.rs          ~490
  └── lib.rs             ~100
```

## 16. `MediaContext` trait — CURRENT state (line 170-176 of scope.rs)

```rust
// THIS IS THE CURRENT THIN STATE — Session 8 expands it (see Part V)
pub trait MediaContext: Send + Sync {
    fn blob_store(&self) -> &dyn crate::store::BlobStore;
    fn working_dir(&self) -> &std::path::Path;  // Note: NOT Option<&Path> currently
}
```

**Session 8 target state (object-safe, expanded):**
```rust
#[async_trait::async_trait]
pub trait MediaContext: Send + Sync {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError>;
    async fn store_blob(&self, data: &[u8], task_id: &str) -> Result<BlobStoreResult, BuiltinError>;
    fn working_dir(&self) -> Option<&std::path::Path>;  // changed: Option<&Path>
    fn is_cancelled(&self) -> bool;
    fn blob_store(&self) -> &dyn crate::store::BlobStore;
    // NO compute_blocking<F, T> — generics break object safety
}
```

## 17. `MediaToolContext` — concrete struct (file:line verified)

```rust
// tools/nika-media/src/tools/context.rs:23-37
pub struct MediaToolContext {
    pub cas: CasStore,                             // owned BY VALUE, NOT Arc
    pub budget: Arc<MediaBudget>,                  // quota tracker (shared)
    pub compute: Arc<ComputePool>,                 // rayon wrapper
    pub working_memory: Arc<WorkingMemoryBudget>,  // RAM budget (RAII guard)
    pub cancel: CancellationToken,                 // field is `cancel`, NOT `cancellation_token`
    pub working_dir: Option<std::path::PathBuf>,
}
```

**Verified API methods (file:line from verification):**
- `ctx.read_media(hash).await -> Result<Vec<u8>, MediaToolError>` (context.rs:62-65)
- `ctx.store_media(data, task_id).await -> Result<StoreResult, MediaToolError>` (context.rs:68-87)
- `ctx.check_cancelled() -> Result<(), MediaToolError>` (context.rs:90-96) — allocates
- `ctx.cancel.is_cancelled() -> bool` — non-allocating
- `ctx.working_memory.acquire(size) -> Result<WorkingMemoryGuard<'_>, MediaToolError>` (context.rs:187-209)
- `ctx.compute.compute(f).await -> Result<T, MediaToolError>` (context.rs:135-147)

## 18. `MediaOp` trait (file:line verified)

```rust
// tools/nika-media/src/tools/mod.rs:78-84
pub trait MediaOp: Send + Sync {
    fn name(&self) -> &str;
    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a MediaToolContext,  // ← concrete type reference
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>>;
}

// mod.rs:86-98 — return type
#[derive(Debug)]
pub enum MediaOpResult {
    Metadata(serde_json::Value),
    Binary {
        data: Vec<u8>,
        mime_type: String,
        extension: String,
        metadata: serde_json::Value,
    },
}
```

## 19. `BuiltinError` variants (nika-kernel/src/builtin.rs:99-131)

```rust
pub enum BuiltinError {
    InvalidArgs { tool: String, reason: String },       // → NIKA-212
    Io          { tool: String, reason: String },       // → NIKA-210
    Parse       { tool: String, reason: String },       // → NIKA-210
    Timeout     { tool: String },                       // → NIKA-210
    Schema      { tool: String, reason: String },       // → NIKA-210
    Denied      { tool: String, reason: String },       // → NIKA-380 (CapabilityDenied, sentinel "(builtin)")
    AssertionFailed { message: String, condition: String }, // → NIKA-213
    Other       { tool: String, reason: String },       // → NIKA-210
}
```

## 20. `MediaToolError` variants (tools/nika-media/src/tools/error.rs:9-47)

```rust
#[derive(Debug, thiserror::Error)]
pub enum MediaToolError {
    ToolError         { tool: String, reason: String },        // NIKA-290
    UnsupportedFormat { tool: String, mime: String },          // NIKA-291
    DependencyMissing { tool: String, feature: String },       // NIKA-292
    Timeout           { tool: String },                        // NIKA-293
    InvalidArgs       { tool: String, reason: String },        // NIKA-294
    PipelineStepFailed{ step: usize, reason: String },         // NIKA-295
    PipelineEmpty,                                             // NIKA-296
    SecurityViolation { tool: String, reason: String },        // NIKA-297
    Media(#[from] crate::error::MediaError),                   // transparent → NIKA-251..259
}
```

## 21. `task_local.rs` — 6 cells + 6 accessors (verified post-S7)

```rust
// tools/nika-kernel/src/task_local.rs — line numbers verified
tokio::task_local! {
    pub static WORKFLOW_DEPTH:       Cell<u32>;              // line 33
    pub static CURRENT_TASK_ID:      Option<Arc<str>>;       // line 38
    pub static CURRENT_TASK_TRUST:   TrustLevel;             // line 45
    pub static CURRENT_TASK_ELEVATED: bool;                  // line 51
    pub static PARENT_CHAIN:         Vec<PathBuf>;           // line 57
    pub static CURRENT_WORKING_DIR:  Option<PathBuf>;        // line 65 (added post-S7)
}

#[inline] pub fn current_depth()           -> u32             { ... }  // line 74
#[inline] pub fn current_task_id()         -> Option<Arc<str>>{ ... }  // line 80
#[inline] pub fn current_task_trust()      -> TrustLevel      { ... }  // line 87
#[inline] pub fn current_task_elevated()   -> bool            { ... }  // line 96
#[inline] pub fn current_parent_chain()    -> Vec<PathBuf>    { ... }  // line 103
#[inline] pub fn current_working_dir()     -> Option<PathBuf> { ... }  // line 110
```

---

# PART IV — ARCHITECTURAL DECISIONS (rationale)

## 22. Why `compute_blocking` is NOT on `MediaContext` (ADR)

**Decision:** Do NOT put `compute_blocking<F, T>` on the trait. Keep it on `MediaToolContext`.

**Status:** Firm (post-review, 2026-04-09).

**Context:** We initially planned `async fn compute_blocking<F, T>(&self, f: F) -> ...`
on the `MediaContext` trait to provide a rayon-bridging CPU-bound dispatch method.

**Problem:** Traits with generic methods are NOT object-safe. `Arc<dyn MediaContext>`
cannot exist if the trait contains any `fn x<T>(...)`. The vtable does not know what
concrete `T` to dispatch to.

**Evidence:**
```rust
// This trait definition:
trait Foo { fn x<T>(&self, t: T); }
// Produces this compile error:
// error[E0038]: the trait `Foo` cannot be made into an object
// --> the trait cannot be made into an object because method `x` has generic type parameters
```

**Consequence for our plan:** The entire `Arc<dyn MediaContext>` router injection pattern would
have been broken. Every `with_media(Arc<dyn MediaContext>)` call would fail to compile.

**Resolution:** `compute_blocking` stays as a concrete method on `MediaToolContext`. Media tools
(nika-media) already have `&MediaToolContext` via `MediaOp::execute` and can call
`ctx.compute.compute(f).await??` directly. The trait is only needed for router injection and
test mocks (nika-kernel-mock), neither of which do CPU-bound work — they deal with metadata
and small blobs only.

**Alternatives considered and rejected:**
- `Box<dyn FnOnce() -> Box<dyn Any + Send>>` type erasure → ugly, forces runtime downcast
- Extension trait `MediaContextExt` with generic `compute_blocking` → non-object-safe at call
  sites that need `dyn MediaContext`, defeats the purpose
- `compute_bytes` + `compute_json` specialized methods → forces tools to serialize intermediate
  state to bytes/JSON, losing type safety

## 23. Why MediaOp impls stay in nika-media (not moved to nika-builtin)

**Decision:** MediaOp implementations live in `nika-media`, not `nika-builtin`.

**Rationale:**
- Moving 29 MediaOp files to nika-builtin would add `image`, `fast_image_resize`, `webp`,
  `oxipng`, `dssim-core`, `scraper`, `dom_smoothie`, `htmd`, `feed-rs`, `c2pa`, etc. as
  nika-builtin dependencies — bloats compile time massively.
- nika-builtin's role is simple tool plumbing (data transforms, file ops, cost tracking).
  It should remain lean.
- The `MediaContext` trait provides the abstraction layer that future consumers (verb crates,
  external tools) need. Tools themselves stay where the libraries live.

**Result:** Phase 12 media work is an *adapter refactor*, not a *tool migration*. Only ~4-5k
LOC of net change in `nika-engine` (NOT the 28k we'd get if we moved everything).

## 24. Why builder pattern, not Bundle struct

**Decision:** `BuiltinToolRouter::new().with_X().with_Y().with_Z()` consuming builder.

**Rejected:** `BuiltinToolRouter::new(Bundle { hitl: ..., media: ..., file: ..., run: ... })`.

**Reasoning:**
- Tests routinely want subsets (e.g., only file tools, only data tools). Bundle forces
  constructing stub fields for every component.
- The current router already uses `.with_file_tools()`, `.with_all_tools()`,
  `.with_cost_tool()`, `.with_records_tool()`, `.with_introspection()`, `.with_hitl()` —
  the consuming builder pattern is established. Session 8 extends it with `.with_media()`.
- Compiler catches missing fields as easily with `Option<Arc<dyn X>>` inside the router
  as with Bundle.

## 25. Why 12.11 is a single commit, not split 12.11a/b

**Original plan (v1):** 12.11a for Tier 1 (5 tools), 12.11b for Tier 2+3 (19 tools).
**Revised:** single commit 12.11.

**Reason:** The v1 plan assumed we'd update `MediaOp::execute` to take `&dyn MediaContext`,
requiring edits to 24 files. The v2 plan keeps `MediaOp::execute(&MediaToolContext)`
unchanged — only the adapter struct in nika-engine changes. That's ~1 file, ~50 LOC.
Splitting into a/b became pointless overhead.

## 26. Why 12.13 deletes `with_all_tools` (zero users rule)

**Memory reference:** `feedback_no_backward_compat.md` — "ZERO users = ZERO backward compat. Only @0.12 matters."

**Decision:** Delete `with_all_tools(file_ctx, media_ctx)` in 12.13. Migrate the 6 call sites
to the new builder chain.

**Call sites (verified):**
```
nika-engine/src/runtime/executor/mod.rs:269       (production wiring)
nika-engine/src/runtime/builtin/router.rs:188     (the definition)
nika-engine/src/runtime/builtin/media/tests_e2e_workflow.rs:72, 861
nika-engine/src/runtime/builtin/media/tests_integration.rs:599, 626
```

After migration:
```rust
// Before:
BuiltinToolRouter::with_all_tools(tool_ctx.clone(), media_ctx)

// After:
let media: Arc<dyn MediaContext> = Arc::new(EngineMediaContext::new(media_ctx));
BuiltinToolRouter::new()
    .with_file_tools(tool_ctx.clone())
    .with_media(media)
```

---

# PART V — SESSION 8 EXECUTION PLAN

**Authoritative commit-level detail:** `nika/docs/sprints/HANDOFF-CONSTELLATION-SESSION8-2026-04-09.md`

**Commit order (verified against architectural review):**

| # | Commit | Crate | Files touched | LOC delta | Risk |
|---|--------|-------|---------------|-----------|------|
| 12.9 | Expand MediaContext trait + EngineMediaContext | nika-kernel + nika-engine | 3 | +150 | MED (new trait) |
| 12.9b | nika-kernel-mock MediaContext impl | nika-kernel-mock | 2 | +200 | LOW |
| 12.10 | Fix 3 async hazards in nika-media | nika-media | 3 | +50 | MED (behavior change) |
| 12.11 | MediaToolAdapter uses Arc<dyn MediaContext> | nika-engine | 2 | +30 | LOW |
| 12.11c | Update 8 engine integration tests | nika-engine | 8 | +40 | LOW |
| 12.12 | Router `.with_media()` builder | nika-engine | 2 | +30 | LOW |
| 12.13 | Delete `with_all_tools`, cleanup | nika-engine | 6 | -80 | LOW |

**Total Session 8 delta:** ~+420/-80 lines of code, ~10-15 commits (with TDD tests).

**Test fixtures required (detail in S8 handoff §4.0):**
- `make_test_engine_media_context()` — functional with 256MB budget, 16MB working memory, 2-thread rayon
- `make_cancellable_engine_media_context()` — returns `(ctx, CancellationToken)`
- `make_test_context_with_budget(bytes)` — tight budget for exhaustion tests
- `MockMediaContext` in nika-kernel-mock — call tracking + in-memory blob store
- `tiny_png_bytes()` — 2×2 PNG for decode tests

**Test cases required (detail in S8 handoff §4.1-4.5):**
- §4.1 EngineMediaContext: 10 tests (P0 = 5, P1 = 3, P2 = 2)
- §4.2 Async hazards: 8 tests (P0 = 3, P1 = 5)
- §4.3 MediaToolAdapter: 8 tests (P0 = 4, P1 = 3, P2 = 1)
- §4.4 Router: 5 tests (all P0)
- §4.5 Edge cases: 5 tests

**Expected test count after Session 8:** 10,854 → ~10,900+ (+50-60 new tests)

---

# PART VI — PHASES 13-23 FORECAST (post-Session 8 roadmap)

## 30. Phase 13 — verb crates (after Phase 12 complete)

**Goal:** Create 5 verb crates that each own one Nika verb:
```
nika-verb-infer    — LLM generation via Arc<dyn Provider>
nika-verb-exec     — Shell execution via Arc<dyn ShellExecutor>
nika-verb-fetch    — HTTP + extraction via Arc<dyn HttpClient>
nika-verb-invoke   — MCP + builtin tool dispatch
nika-verb-agent    — Multi-turn loop over existing verbs
```

**Dependency pattern:** each verb crate depends on `nika-kernel` + `nika-core` only. The engine
composes them. This removes ~40-60k LOC from `nika-engine/runtime/executor/*.rs`.

**Blocker:** Phase 12 must be complete (all builtin tools migrated). Currently at 34/63.

**Expected LOC:**
- nika-verb-infer: ~8-10k (streaming, structured output, retry, repair)
- nika-verb-exec: ~2k (blocklist, timeout, shell escaping)
- nika-verb-fetch: ~5-6k (9 extract modes, SSRF, feature flags)
- nika-verb-invoke: ~2k (just dispatch to router)
- nika-verb-agent: ~4-5k (tool loop, guardrails, limits)

## 31. Phase 14 — nika-runtime + cache + RunContext splinter

**Goal:** Extract runner + executor + cache from nika-engine into `nika-runtime`. Split
`RunContext` into focused scopes (`BindingScope`, `MediaScope`, `RecordScope`).

**Target LOC:** ~30k in `nika-runtime`, shrinks `nika-engine` by ~45-60k.

**New crate:** `nika-cache` — LLM/HTTP response cache backed by blake3 keys.

## 32. Phase 6 — error_domains promotion

**Goal:** Replace most `NikaError::*` variants with domain sub-enums
(`ExecutionError`, `ProviderError`, `BindingError`, `DagError`) via `#[error(transparent)]`.

**Scale:** 180+ call sites across engine. Single big-bang commit (miette's `#[diagnostic]`
doesn't delegate through `#[error(transparent)]`, so partial migration is worse than big-bang).

**Estimated effort:** 1 dedicated session (~8-16 hours).

## 33. Phase 7 — LSP absorption

**Goal:** Move `nika-engine/src/lsp/` (~12k LOC) into `nika-lsp-core`. Delete duplicate logic.

**Blocker:** Handler migration pattern must be designed first. Current `nika-lsp-core` is
pure functions; `nika-engine/src/lsp/` has stateful handlers (completion, hover, definition).

**Expected reduction:** -4k LOC net in nika-engine.

## 34. Phase 17 — nika-tui split

**Goal:** Split the 88,989 LOC monolithic `nika-tui` into 4 crates:
```
nika-tui-core       — TuiState, events, reducers
nika-tui-widgets    — reusable ratatui components
nika-tui-views      — Studio + Command + Control views
nika-tui-app        — main event loop
nika-tui            — facade re-export
```

**Scale:** Largest single refactor of the whole Constellation plan. Expected to run 2-3 sessions.

## 35. Phase 19 — type system hardening

**Goal:** Introduce newtypes (`TaskId`, `WorkflowId`, `HashKey`) to replace `String` at
trust boundaries. Sealed traits. `Runner<State>` type-state pattern (Initialized → Ready → Running → Done).

## 36. Phase 21 — Zero-unwrap migration

**Current baseline:** 4,263 `.unwrap()` / `.expect()` in production code (see §5).

**Target:** <50 total, each with `// REASON:` comment explaining why it's impossible to panic.

**Approach:**
1. Day 1: Add `#![warn(clippy::unwrap_used, clippy::expect_used)]` to all crate roots
2. Day 1: CI ratchet — the number can only go DOWN, never UP
3. Weeks 1-6: Systematic replacement, grouped by file, using `?` + `BuiltinError::*`
4. Final: Remove the warn → `#![deny]` once count < 50

**Effort:** 6-10 weeks of iterative work. Not blocking for launch.

## 37. Phase 22 — PGO + binary size

**Goal:** Profile-guided optimization + binary strip + LTO tuning.
**Expected:** -20-30% binary size, +10-20% runtime perf on hot paths.

## 38. Phase 23 — blake3 AST cache

**Decision:** Replace the planned Salsa incremental-compute library with a simpler
blake3-keyed cache on the Analyzed AST boundary.

**Rationale:** Salsa would take 2 months; blake3 cache takes 2 weeks. Same user-visible
win (`nika check` repeat < 5ms).

**Pattern:**
```rust
fn check_cached(path: &Path) -> Result<AnalyzedWorkflow> {
    let bytes = fs::read(path)?;
    let key = blake3::hash(&bytes);
    if let Some(cached) = CACHE.get(&key) { return Ok(cached); }
    let analyzed = analyze(&bytes)?;
    CACHE.insert(key, analyzed.clone());
    Ok(analyzed)
}
```

## 39. V2.3 firm targets table

| Metric | Current | Launch target | Post-launch |
|--------|---------|---------------|-------------|
| `nika-engine` LOC | 157,746 | **≤100,000** | <80,000 (Phase 15) |
| Unwrap in prod | 4,263 | **<50** with `// REASON:` | 0 total |
| `nika check` repeat | ~200ms | **<5ms** (blake3 cache) | comemo sub-file |
| Workspace crates | 26 | **28** | ~32 |
| Tests | 10,854 | **10,900+** | 12,000+ |
| `main.rs` LOC | 2,932 | **<900** | <500 |
| Phase 12 tools migrated | 34/63 | **63/63** | — |
| Zero clippy warnings | ✅ | ✅ | ✅ |

Reference: `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md`

---

# PART VII — INVARIANTS (NEVER break these)

1. **Trust via task_local only** — never pass `TrustLevel` as function argument to `call()` methods
2. **`task_local!` is NOT visible in rayon closures** — capture on tokio side before dispatch
3. **No generic methods on `MediaContext` trait** — breaks object safety; put on concrete type
4. **`let _name = guard;` NOT `let _ = guard;`** — underscore-only binding drops immediately
5. **All files have `// SPDX-License-Identifier: AGPL-3.0-or-later`** — verify with grep pre-commit
6. **Zero `.unwrap()` in production paths** — use `?` with `BuiltinError::*`
7. **`async fn` in traits uses `#[async_trait]` (NEVER `?Send`)** — router spawns across tokio workers
8. **Tests validate VALUES programmatically** — `assert_eq!(parsed["hash"].len(), 71)` not `!is_null()`
9. **Commit co-author:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic
10. **`cargo test --lib` only** — never full test without `--lib` (macOS keychain popups)
11. **One commit per logical unit** — no "misc fixes" mega-commits
12. **Zero backward-compat for <v1.0** — delete dead methods, don't deprecate
13. **Object-safety compile assertion** — `const _: fn(&dyn MediaContext) = |_| {};` must compile
14. **`nika-builtin` does NOT depend on `nika-engine`** — verify with `cargo tree` after changes
15. **5 verbs are sacred** — NEVER add a 6th. Use `invoke: nika:*` for new capabilities
16. **Trust-aware template interpolation** — untrusted data spotlight-fenced before `{{ }}`
17. **Runner-set-then-read task_locals** — runner.rs sets them once, tools only read
18. **MCP names use `server::tool` double colon** — never slash, never single colon

---

# PART VIII — KNOWN DEBT / DEFERRED (do NOT fix blindly)

| Item | Why deferred | Resolution phase |
|------|--------------|------------------|
| `token_count` model param ignored (always heuristic) | tiktoken dispatch complexity | post-launch |
| `locale_lookup` returns `Err` for not-found (should be `{found: false}`) | API design decision | post-launch |
| `nika-engine/src/registry/` (2,870 LOC) | Phantom feature, no public registry | nuke commit (standalone) |
| Introspection tools (task_status, records, orchestrate) | Need `RecordView` DTO in nika-core | Phase 13 prerequisite |
| `IndexedDag` (878 LOC) not wired into Runner | Runner uses old path | Phase 14 |
| `error_domains.rs` (250 LOC, 180 call sites) | Big-bang commit needed | Phase 6 |
| `nika-engine/src/lsp/` (12k LOC duplicate) | Handler migration pattern not designed | Phase 7 |
| `nika-tui` 88k LOC not split | Largest refactor, 2-3 sessions | Phase 17 |
| 4,263 unwrap/expect in prod | Systematic migration | Phase 21 |
| Windows daemon features disabled | Intentional — `#[cfg(unix)]` accepted | Won't fix |
| `nika pkg` module | Nuked in decision 2026-04-09 | Re-add post-launch via GH releases |
| `nika-macros` has 0 tests | Compile-time only tests via macrotests (planned) | Phase 19 |

---

# PART IX — SKILLS + TOOLS LIBRARY (every skill relevant, with triggers)

## Mandatory skills (use EVERY session)

| Skill | Trigger | Why |
|-------|---------|-----|
| `spn-powers:test-driven-development` | Before writing ANY implementation code | RED-GREEN-REFACTOR |
| `spn-powers:verification-before-completion` | Before EVERY commit | Evidence before assertions |
| `spn-powers:using-superpowers` | Session start (automatic) | Establishes mandatory workflows |
| `spn-powers:using-git-worktrees` | Multi-commit sessions (4+) | Isolated rollback space |
| `spn-powers:testing-anti-patterns` | Before writing any `assert!` | Prevents shallow/mock tests |

## Rust-specific (use for implementation)

| Skill | Trigger | Why |
|-------|---------|-----|
| `spn-rust:rust-core` | Designing traits, error types, ownership | Senior-level patterns |
| `spn-rust:rust-async` | Async/tokio/rayon work | No-lock-across-await, channels, actors |
| `spn-rust:rust-ai` | LLM + ort/candle/rmcp | Production ML patterns |
| `spn-rust:rust-agentic` | Multi-agent + DAG + RAG | Workflow DSL design |
| `rust-testing` (global) | Writing tests | TDD patterns, property-based, coverage |
| `rust-best-practices` (global) | Reviewing code | Borrow vs clone, ownership |
| `rust-patterns` (global) | Idiomatic patterns | Ownership, error handling, concurrency |

## Debugging + review

| Skill | Trigger | Why |
|-------|---------|-----|
| `spn-powers:systematic-debugging` | Any test failure or clippy error | 4-phase framework (no guessing) |
| `spn-powers:root-cause-tracing` | Error deep in call stack | Trace backward with instrumentation |
| `spn-powers:receiving-code-review` | Review feedback arrives | Technical rigor, verify before acting |
| `spn-powers:requesting-code-review` | Before merging major changes | Dispatches code-reviewer subagent |
| `spn-powers:defense-in-depth` | Invalid data breaks deep in execution | Validate at every layer |
| `spn-powers:condition-based-waiting` | Flaky timeout tests | Replace wall-clock with condition polling |
| `spn-powers:dispatching-parallel-agents` | 3+ independent failures | Parallel investigation |
| `spn-powers:subagent-driven-development` | Executing plan with independent tasks | Fresh subagent per task |

## Planning + brainstorming

| Skill | Trigger | Why |
|-------|---------|-----|
| `spn-powers:brainstorming` | Creating new features, before code | Socratic refinement |
| `spn-powers:writing-plans` | After brainstorming, before implementation | Detailed task breakdown |
| `spn-powers:executing-plans` | Plan provided, ready to implement | Batch + review checkpoints |
| `spn-powers:finishing-a-development-branch` | All tests pass, ready to merge | Integration options |

## Nika slash commands

| Command | Purpose | When |
|---------|---------|------|
| `/nika-smoke` | 2-minute sanity check (workflows) | Before + after session |
| `/ast-sync-check` | nika-core ↔ nika AST sync | After trait changes |
| `/nika-audit` | Deep audit (530+ workflows, 8 tiers) | Milestone review |

## Git skills

| Skill | Trigger |
|-------|---------|
| `spn-powers:git:commit` | Fast commit with conventional commits |
| `spn-powers:git:push` | Commit + push with progress |

---

# PART X — MIGRATION PATTERNS LIBRARY

## Pattern 1 — Trait abstraction over concrete type

**When:** You want downstream crates to use a type without pulling its dependencies.

**Example:** `Arc<dyn Provider>` for `RigProvider` (Phase 11).

**Steps:**
1. Define trait in L0.5 crate (`nika-kernel`)
2. Implement trait in L2 crate (`impl Provider for RigProvider`)
3. Expose via accessor: `fn get_dyn_provider(&self, name) -> Arc<dyn Provider>`
4. Downstream crates consume `Arc<dyn Provider>`

**Object safety requirements:**
- No generic methods (`fn x<T>(...)`) — use concrete types or type erasure
- No `Self` in return type except `Self::Assoc` associated types
- `async fn` requires `#[async_trait]` — NEVER `#[async_trait(?Send)]`

## Pattern 2 — Connect, don't delete

**When:** You find "dead" scaffolding (unused trait, unused method, unused field).

**Rule:** Do NOT delete it. Wire it up. The scaffold exists because a prior session planned
to connect it and ran out of time. Reading intent from code is the Constellation-v2 principle.

**Exceptions:**
- Absorbed code (e.g., `nika-engine/src/lsp/` absorbed into `nika-lsp-core` in Phase 7)
- Explicitly nuked features (e.g., `nika pkg`, `nika-engine/src/registry/`)
- Memory rule violations (e.g., backward-compat methods with zero users)

## Pattern 3 — Migrate then delete

**When:** A signature change affects multiple files and you can't atomically update all of them.

**Steps:**
1. Commit A: Add NEW method/field alongside the old one (both work)
2. Commit B: Migrate call sites one at a time (each commit compiles)
3. Commit C: Delete the old method/field
4. Each commit in between has a passing `cargo test --workspace --lib`

**Example:** Moving `tokio::task_local!` from `nika-engine` to `nika-kernel` (S7 commit 12.6-pre)
used this pattern — the engine kept re-exports (`pub(crate) use nika_kernel::task_local::*`)
so existing call sites in runner.rs compiled unchanged while the declaration moved.

---

# PART XI — VERIFICATION CHECKLIST (pre-flight before any session)

Run these commands in order. Every one must succeed before touching code.

```bash
cd /Users/thibaut/dev/supernovae/nika

# 1. Git state clean
git status
# Expected: "nothing to commit, working tree clean"

# 2. On main branch, up to date
git log -1 --oneline
# Expected: e1fbbf35b or later (commits since the mega handoff)

# 3. Baseline test count
cd tools
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10,854 or higher (regressions = STOP)

# 4. Zero clippy warnings
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -5
# Expected: "Finished" (clean)

# 5. Specific crate health
cargo test -p nika-builtin --lib -q 2>&1 | tail -3
# Expected: 250+ tests passing

# 6. Compile check (full workspace, non-test)
cargo check --workspace 2>&1 | tail -5
# Expected: clean Finished

# 7. AST sync check
# (run the slash command: /ast-sync-check)

# 8. Full workflow smoke test
# (run: /nika-smoke)

# 9. Memory alignment
# Read ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md
# Verify the quick state block matches current HEAD

# 10. Verify kernel has no unwraps (invariant)
find tools/nika-kernel/src -name "*.rs" -not -name "tests*.rs" | \
    xargs grep -c "\.unwrap()\|\.expect(" 2>/dev/null | \
    awk -F: '{s+=$2} END{print "nika-kernel unwrap count:", s}'
# Expected: 0
```

**Additional verification before Session 8 specifically:**
```bash
# 11. Verify MediaContext current state (should be the thin version)
grep -A 6 "pub trait MediaContext" tools/nika-kernel/src/scope.rs
# Expected: only 2 methods (blob_store, working_dir)

# 12. Verify nika-media compile
cargo check -p nika-media --lib
# Expected: clean

# 13. Verify test fixtures don't already exist
test -f tools/nika-engine/src/runtime/media_context.rs && echo "EXISTS (unexpected)" || echo "not yet"
test -f tools/nika-kernel-mock/src/media.rs && echo "EXISTS (unexpected)" || echo "not yet"
# Expected: "not yet" for both (they're created in 12.9 and 12.9b)
```

---

# PART XII — ROLLBACK STRATEGY

## Per-commit rollback

Each commit must compile + pass tests INDIVIDUALLY. If a commit breaks, rollback:
```bash
git reset --hard HEAD~1   # undo last commit (WORKING TREE LOSS)
# OR
git revert HEAD            # new commit reversing the last one (safe)
```

## Worktree-based isolation (recommended for Session 8)

Before starting Session 8:
```bash
cd /Users/thibaut/dev/supernovae
git worktree add nika-session8 main
cd nika-session8
# All work happens here; main repo is untouched
# After session completes:
cd ../nika
git cherry-pick <commits>   # or git pull from worktree
git worktree remove ../nika-session8
```

## Session-level rollback

If 12.9's trait expansion turns out to be wrong:
```bash
git log --oneline dbe702e77..HEAD   # list commits since S7 end
git reset --hard dbe702e77           # nuclear option — back to S7 state
git push --force-with-lease main    # DANGEROUS — ask user first
```

**Never `git push --force` without `--force-with-lease` and user authorization.**

---

# PART XIII — COMMAND REFERENCE CARD

```bash
# === TESTS ===
cargo test --workspace --lib -q                      # full workspace (10,854 tests)
cargo test -p nika-builtin --lib -q                  # single crate
cargo test -p nika-engine --lib test_name -q         # single test
cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{s+=$4} END{print s}'
                                                     # count total tests

# === CLIPPY ===
cargo clippy --workspace --lib -- -D warnings        # all crates, fail on warning
cargo clippy -p nika-builtin -p nika-engine --lib -- -D warnings  # subset

# === BUILD ===
cargo check --workspace                              # fastest compile check
cargo build --release -p nika                        # production binary

# === LOC AND UNWRAP TRACKING ===
find tools/nika-engine/src -name "*.rs" -not -name "tests*.rs" | xargs wc -l | tail -1
                                                     # engine LOC
find tools/nika-engine/src -name "*.rs" -not -name "tests*.rs" | \
    xargs grep -c "\.unwrap()\|\.expect(" | awk -F: '{s+=$2} END{print s}'
                                                     # engine unwrap count

# === GIT ===
git log --oneline -20                                # recent commits
git log --graph --oneline --all -30                  # visual history
git diff --stat HEAD                                 # working tree delta
git worktree add ../nika-sessionN main               # isolated worktree

# === FIND BLOCKING I/O IN ASYNC CODE ===
grep -rn "\.canonicalize()" tools/nika-media/src/    # should only find tokio::fs::canonicalize
grep -rn "std::fs::" tools/nika-media/src/tools/ | grep -v "//\|test"
                                                     # should be zero (all I/O via tokio or nika-fs)

# === CHECK AGPL HEADERS ===
git diff --name-only HEAD | grep "\.rs$" | xargs grep -L "AGPL-3.0-or-later"
                                                     # lists files MISSING the header

# === WORKFLOW SCAN ===
grep -rn "fn call.*BuiltinTool" tools/nika-builtin/src/ | wc -l
                                                     # count BuiltinTool impls in nika-builtin
```

---

# PART XIV — APPENDICES

## A. Key documents (full paths)

| Document | Path |
|----------|------|
| This mega handoff | `nika/docs/sprints/MEGA-HANDOFF-CONSTELLATION-2026-04-09.md` |
| Session 8 commit detail | `nika/docs/sprints/HANDOFF-CONSTELLATION-SESSION8-2026-04-09.md` |
| Session 7 (historical) | `nika/docs/sprints/HANDOFF-CONSTELLATION-SESSION7-2026-04-09.md` |
| Session 6 (historical) | `nika/docs/sprints/HANDOFF-CONSTELLATION-SESSION6-2026-04-09.md` |
| V2.3 aggressive targets | `nika/docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md` |
| Mega plan | `nika/docs/plans/2026-04-08-constellation-v2-mega-plan.md` |
| Engine architecture | `nika/tools/nika-engine/ARCHITECTURE.md` |
| nika-builtin reference | `nika/tools/nika-builtin/CLAUDE.md` |
| Security threat model | `nika/SECURITY.md` |
| Root AGENTS.md | `nika/AGENTS.md` |
| Project CLAUDE.md | `nika/CLAUDE.md` |

## B. Error codes index (all NIKA-XXX codes encountered)

| Code | Name | Crate | Meaning |
|------|------|-------|---------|
| NIKA-010 | SchemaValidation | core | YAML schema validation error |
| NIKA-020 | DagCycle | core | Cycle detected in DAG |
| NIKA-026 | UpstreamFailed | core | Dependency chain failed |
| NIKA-041 | TemplateResolution | engine | `{{with.x}}` error |
| NIKA-045 | FetchError | engine | SSRF blocked, timeout, invalid URL |
| NIKA-046 | ExtractError | engine | CSS selector failed, unsupported mode |
| NIKA-053 | BlockedCommand | engine | Shell blocklist / unescaped binding |
| NIKA-071 | UnknownAlias | engine | `{{with.alias}}` not declared |
| NIKA-072 | NullValue | engine | Null at path in strict mode |
| NIKA-100 | McpConnection | mcp | MCP connection error |
| NIKA-101 | McpServerStart | mcp | MCP server failed to start |
| NIKA-107 | McpParamValidation | mcp | Missing/invalid MCP params |
| NIKA-112 | AgentGuardrail | engine | Guardrail violation |
| NIKA-140 | AstAnalysis | core | AST analysis failure |
| NIKA-210 | BuiltinToolError | engine | Generic builtin error |
| NIKA-212 | BuiltinInvalidParams | engine | Invalid params |
| NIKA-213 | AssertionFailed | engine | Assertion failure |
| NIKA-215 | ArtifactExists | engine | nika:write overwrite |
| NIKA-251..259 | MediaError (various) | media | Pipeline errors |
| NIKA-270 | SkillFileNotFound | engine | Missing skill file |
| NIKA-271 | SkillIntegrityFailed | engine | blake3 hash mismatch |
| NIKA-281 | ArtifactWrite | engine | Disk/permission error |
| NIKA-290 | MediaToolError | media | Generic media tool error |
| NIKA-291 | UnsupportedFormat | media | Unsupported MIME type |
| NIKA-292 | DependencyMissing | media | Feature flag not enabled |
| NIKA-293 | MediaTimeout | media | Media operation timeout |
| NIKA-294 | MediaInvalidArgs | media | Invalid params |
| NIKA-295 | PipelineStepFailed | media | Pipeline step error |
| NIKA-296 | PipelineEmpty | media | Empty pipeline |
| NIKA-297 | SecurityViolation | media | Path/safety violation |
| NIKA-300 | StructuredOutputValidation | engine | JSON schema failed |
| NIKA-380 | CapabilityDenied | engine | Trust/Shield denial |
| NIKA-381 | TrustViolation | engine | Strict mode invariant |
| NIKA-382 | CanaryLeaked | engine | Canary token in output |
| NIKA-383 | InjectionDetected | engine | Scanner match |
| NIKA-384 | SpotlightRequired | engine | Missing fence |
| NIKA-385 | MlModelMissing | engine | ML model not loaded |
| NIKA-386 | RunDepthExceeded | engine | Nested run limit |
| NIKA-387 | RunCycleDetected | engine | Recursion cycle |
| NIKA-388 | CanaryInThinking | engine | Extended thinking leak |
| NIKA-389 | UntrustedVisionBlocked | engine | Adversarial image |

## C. Session memory files (persistent across conversations)

```
~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/
  ├── MEMORY.md                                 — index (updated post-S7)
  ├── project_constellation_session6.md         — S6: Phase 12 commits 1-7
  ├── project_constellation_session7.md         — S7: done (5 commits + review fixes)
  ├── project_constellation_findings_log.md     — running findings log
  ├── project_aggressive_targets_v23.md         — V2.3 targets reference
  ├── project_nika_shield_review_findings.md    — Shield architecture
  ├── feedback_no_backward_compat.md            — zero users rule
  ├── feedback_agpl_license.md                  — all crates AGPL-3.0-or-later
  ├── feedback_no_claude_coauthor.md            — only Nika co-author
  ├── feedback_zero_unwrap_policy.md            — Phase 21 rule
  └── ... (27 total memory files)
```

## D. Launch context (non-technical but critical)

- **Target launch date:** 2026-05-05 (Show HN Tuesday/Wednesday 14h Paris)
- **Brand positioning:** "Inference as Code" + "One file. Any AI."
- **Public vs private repos:**
  - `nika/` and `novanet/` are PUBLIC GitHub repos (submodules)
  - `supernovae-hq/` is PRIVATE (strategy, research, launch plans)
  - Research, competitive intel, brand strategy → `supernovae/docs/` ONLY
- **Competitive moat:** Zero Rust workflow engines at scale. First-mover advantage.
- **Co-founder:** Nicolas manages GPU infra (H100, L40S on Scaleway)

---

## 🦋 END OF MEGA HANDOFF

**If you are a fresh Claude session reading this:**

1. Run the Part XI verification checklist FIRST. Every command must pass.
2. Read Part V for the specific Session 8 commits. The detail doc is in the same directory.
3. Announce the skills you plan to use from Part IX before coding.
4. Use Part X migration patterns when refactoring signatures.
5. Check Part VII invariants when in doubt.
6. The user's preferred language is Franglais conversations, English code/docs/commits.
7. Commits use ONLY `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic.
8. When unsure, ask. Architecture questions are 🔴 ASK per the user's global rules.

Launch is in 4 weeks. The plan follows the work, not the other way around. Ship perfect.
