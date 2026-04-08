# Constellation Execution Handoff — SESSION 4

> **Copy-paste this ENTIRE file as context for a fresh Claude Code session.**

---

## SITUATION

- **Version:** v0.79.0 (unchanged — no version bump yet)
- **Branch:** main
- **Last commit:** `e4a421a03` refactor(core): split analyze.rs (5531 LOC) into 6 module files
- **Tests:** 10,755 passed, 0 failed (`cargo test --workspace --lib`)
- **Clippy:** Zero warnings
- **Crates:** 24 workspace members (17 original + 7 new)
- **Launch:** May 5, 2026 (J-27)
- **Codename:** Constellation v2.1

---

## WHAT WAS DONE (Sessions 1-3, 2026-04-08)

### SESSION 1 — Unblock + Quick Wins (9 commits)
- ARM64 linker fix, dead MPSC receivers, McpInvoke redaction, Serve Mutex fix
- #[must_use] on Runner builders, FxHashSet in template.rs, OnceLock for workspace_root
- Arc::from(task_id) hoisted, ARCHITECTURE.md for nika-engine

### SESSION 2 — Kernel + Foundation (7 commits)
- **nika-kernel** crate (717 LOC, 10 traits, L0.5 layer)
- **nika-kernel-mock** crate (744 LOC, 5 mocks, 23 tests)
- rstest pilot (26 → 5 parametrized tests)
- EventEmitter blanket impl for Arc<T>
- transform.rs split (5570 → 5 files), template.rs split (4938 → 2 files)

### SESSION 3 — Effect Crates + God File Split (2 commits)
- **5 L1 effect crates** (62 tests):
  - nika-clock: SystemClock (tokio::time)
  - nika-fs: TokioFs (tokio::fs + globset)
  - nika-blob: DiskBlobStore (blake3 CAS)
  - nika-http: ReqwestClient (SSRF protection)
  - nika-exec-runner: TokioShell (command blocklist)
- **analyze.rs split** (5531 → 6 files, largest source 1109 LOC)

---

## COMPLETED PHASES

| Phase | What | Key Files |
|-------|------|-----------|
| 1 | nika-kernel (10 traits) | `tools/nika-kernel/src/` |
| 2 | nika-kernel-mock (5 mocks) | `tools/nika-kernel-mock/src/` |
| 4 | rstest pilot | `nika-core/src/binding/transform/tests.rs` |
| 5.1 | EventEmitter blanket impl | `nika-event/src/emitter.rs` |
| 8a | transform.rs split | `nika-core/src/binding/transform/{mod,apply,parser,helpers,tests}.rs` |
| 8b | template.rs split | `nika-engine/src/binding/template/{mod,tests}.rs` |
| 9+10 | 5 L1 effect crates | `tools/nika-{clock,fs,blob,http,exec-runner}/` |
| 16 partial | analyze.rs split | `nika-core/src/ast/analyzer/analyze/{mod,verb_analysis,validation,cycle_detection,task_table,tests}.rs` |

---

## DEFERRED (with reasons)

### Phase 5.2: EventSink hot site wiring → Phase 14
30+ structs hold EventLog by value. Cascades through TaskExecutor → Runner → 25+ child structs.

### Phase 6: error_domains big-bang → Dedicated session
180+ call sites, miette #[diagnostic] delegation issues, ExecutionError(String) 70 sites need semantic analysis.

### Phase 7: LSP absorption → Dedicated session
engine/lsp and nika-lsp-core are NOT simple duplicates:
- Different handler signatures (position-based vs CursorContext-based)
- Core has 4 unique handlers (references, rename, document_links, folding_ranges)
- Engine has unique AST index, model_intel, semantic diagnostics
- Already partially integrated (engine delegates to core)

### Phase 8c: runner/mod.rs → Phase 14
Tight internal coupling; meaningful split requires VerbExecutor restructure.

---

## REMAINING GOD FILES

| File | LOC | Action | When |
|------|-----|--------|------|
| `nika/src/main.rs` | 5,530 | Migrate to nika-cli/verbs/ | Phase 15 |
| `nika-engine/src/error.rs` | 2,874 | error_domains promotion | Phase 6 |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Restructure with VerbExecutor | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Source halves tightly coupled | Phase 14+ |

---

## NEXT PHASES — SESSION 4

### Priority 1: Phase 11 — Provider trait cutover
The BIG one. Wrap `RigProvider` enum in `Provider` trait impl.

Current state:
- `RigProvider` enum: 9 variants in `nika-engine/src/provider/rig/mod.rs`
- `dispatch_rig!` macro: 5 call sites in `inference.rs` and `provider_streaming.rs`
- 26 files reference RigProvider
- Kernel `Provider` trait with DTOs already defined in `nika-kernel/src/provider.rs`

Strategy (v2.1 big-bang):
1. `impl Provider for RigProvider` that bridges kernel DTOs ↔ rig-core types
2. Flip TaskExecutor to use `Arc<dyn Provider>` instead of `RigProvider` directly
3. Keep `dispatch_rig!` inside the trait impl (connect, don't delete)
4. Mock and Native get separate `impl Provider` blocks

### Priority 2: Phase 12 — nika-builtin extraction
Create `nika-builtin` crate with 63 builtin tools + sealed `BuiltinTool` trait.

### Priority 3: Phase 15 — main.rs migration
Move 5530 LOC from `nika/src/main.rs` to `nika-cli/src/verbs/`.

---

## KEY NUMBERS

| Metric | v0.79.0 | After S1+2 | After S3 |
|--------|---------|------------|----------|
| Workspace crates | 17 | 19 | **24** |
| Tests | 10,666 | 10,693 | **10,755** |
| God files (>1500 LOC source) | 5 | 3 | **2** (analyze.rs split) |
| Traits defined | 0 | 10 | 10 |
| Production trait impls | 0 | 0 | **5** (effect crates) |
| Mock implementations | 0 | 5 | 5 |
| Clippy warnings | 0 | 0 | 0 |

---

## CURRENT LAYERING

```
L0    nika-core (23k)         Pure types, AST, catalogs. Zero I/O.
L0.5  nika-kernel (717)       10 trait defs. Zero impls.
      nika-kernel-mock (744)  5 hand-written mocks. Dev-dep.
L1    nika-clock              SystemClock (tokio::time)              NEW S3
      nika-fs                 TokioFs (tokio::fs + globset)          NEW S3
      nika-blob               DiskBlobStore (blake3 CAS)             NEW S3
      nika-http               ReqwestClient (SSRF protection)        NEW S3
      nika-exec-runner        TokioShell (command blocklist)         NEW S3
      nika-event (4.5k)       EventLog + EventEmitter trait + blanket impl
      nika-lsp-core (12k)     LSP intelligence
L2    nika-engine (160k)      MONOLITH — extraction source
      nika-media (14k)        CAS store, image ops
      nika-mcp (9k)           MCP client (rmcp)
      nika-vault (1.2k)       Encrypted secrets
      nika-storage (1k)       Storage abstraction
      nika-display (13k)      CLI renderers
L3    nika-daemon (7k)
L4    nika-cli (8k), nika-tui (88k), nika-serve (4k), nika-lsp (2.5k),
      nika-sdk (3k), nika-init (21k)
L5    nika (5.5k)             Binary entry point
```

---

## RULES — NON-NEGOTIABLE

- `cargo test --workspace --lib` after EVERY commit (from `tools/nika/`)
- `cargo clippy --workspace -- -D warnings` — zero warnings
- Never `cargo test` without `--lib` (macOS Keychain popups)
- 1 logical change = 1 commit
- Co-author: `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- **CONNECT, DO NOT DELETE.** Dead scaffolding gets wired, not removed.
- Big-bang cutovers OK (v0 = zero users)

---

## MANDATORY READS

1. `nika/CLAUDE.md` — project overview
2. `tools/nika/CLAUDE.md` — crate architecture
3. `tools/nika-engine/ARCHITECTURE.md` — engine module map
4. `docs/plans/2026-04-08-constellation-v2-mega-plan.md` — THE PLAN (sections 5, 8, 17, 18)
5. Memory: `project_constellation_session3.md`

---

**Start with Phase 11 (Provider trait cutover) — it's the keystone.**
