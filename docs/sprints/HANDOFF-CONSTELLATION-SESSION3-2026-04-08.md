# Constellation Execution Handoff — SESSION 3

> **Copy-paste this ENTIRE file as context for a fresh Claude Code session.**
> **It contains everything needed to continue the Constellation architecture refactor.**

---

## SITUATION

- **Version:** v0.79.0 (unchanged — no version bump yet)
- **Branch:** main
- **Last commit:** `6390be306` refactor(engine): split template.rs
- **Tests:** 10,693 passed, 0 failed, 1 ignored (`cargo test --workspace --lib`)
- **Clippy:** Zero warnings (`cargo clippy --workspace -- -D warnings`)
- **Crates:** 19 workspace members (17 original + nika-kernel + nika-kernel-mock)
- **LOC delta:** +7,872 / -4,679 across 50 files (23 commits this session)
- **Launch:** May 5, 2026 (J-27)
- **Codename:** Constellation v2.1
- **Working directory:** `/Users/thibaut/dev/supernovae/nika/tools/nika`

---

## WHAT WAS DONE (Sessions 1+2, 2026-04-08)

### SESSION 1 — Unblock + Quick Wins (9 commits)

| # | Commit | Type | Description |
|---|--------|------|-------------|
| 1 | `fe0232d86` | fix(build) | ARM64 linker: disable opt-level in test profile (3 html5ever versions conflict) |
| 2 | `6134b073e` | fix(runtime) | Dead MPSC receivers: spawned drain tasks in infer.rs (2 sites) |
| 3 | `7af91c350` | fix(security) | Redact McpInvoke params before trace serialization |
| 4 | `e330ba649` | fix(serve) | Release workers Mutex before async storage update (edition 2021 temporary) |
| 5 | `0de5a3c2f` | refactor(runtime) | `#[must_use]` on 10 Runner builder methods |
| 6 | `574d070cb` | perf(binding) | FxHashSet in template.rs (6 sites, was std HashSet) |
| 7 | `b4fa788fa` | refactor(runtime) | OnceLock for workspace_root (write-once invariant) |
| 8 | `671f3fbef` | perf(runtime) | Hoist Arc::from(task_id) out of streaming loops (5 sites) |
| 9 | `f2f10e581` | docs(engine) | ARCHITECTURE.md for nika-engine (matklad rule, 190 lines) |

### SESSION 2 — Kernel + Foundation (14 commits, 7 ours + 7 hooks/docs)

| # | Commit | Type | Description |
|---|--------|------|-------------|
| 10 | `1facbe6d3` | docs | Fix nika-daemon missing in ARCHITECTURE.md (code review P2) |
| 11 | `8e1d4f2aa` | feat(kernel) | **nika-kernel crate** — 10 trait defs, 717 LOC, L0.5 layer |
| 12 | `36ece3b94` | refactor(core) | rstest pilot: 26 tests → 5 parametrized tables (-77 lines) |
| 13 | `4c2af7fe3` | feat(event) | EventEmitter blanket impl for `Arc<T>` + EventSink alias (+4 tests) |
| 14 | `a98f9409a` | feat(kernel) | **nika-kernel-mock** — 5 hand-written mocks, 23 tests, 744 LOC |
| 15 | `1d55db887` | refactor(core) | **Split transform.rs** (5570→5 files, all source <1500 LOC) |
| 16 | `6390be306` | refactor(engine) | **Split template.rs** (4938→2 files: source 2053 + tests 2887) |

---

## COMPLETED PHASES

| Phase | What | Key Files |
|-------|------|-----------|
| Phase 1 | nika-kernel (10 traits) | `tools/nika-kernel/src/` (8 files, 717 LOC) |
| Phase 2 | nika-kernel-mock (5 mocks) | `tools/nika-kernel-mock/src/` (6 files, 744 LOC) |
| Phase 4 | rstest pilot (transform.rs) | `nika-core/src/binding/transform/tests.rs` |
| Phase 5.1 | EventEmitter blanket impl | `nika-event/src/emitter.rs` (blanket + EventSink alias) |
| Phase 8a | transform.rs split | `nika-core/src/binding/transform/{mod,apply,parser,helpers,tests}.rs` |
| Phase 8b | template.rs split | `nika-engine/src/binding/template/{mod,tests}.rs` |

---

## DEFERRED PHASES (with reasons)

### Phase 5.2: Flip 5 hot sites to EventSink
**Why deferred:** 30+ structs hold `event_log: EventLog` by value. Changing to `Arc<EventLog>`
cascades through TaskExecutor → Runner → RigAgentLoop → 25+ child structs. EventLog is already
cheap to clone (all fields are Arc internally), so the perf win is negligible.
**When to do:** Phase 14 (verb crate extraction) — each verb crate takes `Arc<dyn EventEmitter>`.

### Phase 6: error_domains big-bang promotion
**Why deferred:** 180+ call sites across 30+ files. miette `#[diagnostic]` attributes don't
delegate through `#[error(transparent)]` without adding miette derives to all 4 sub-enums.
`ExecutionError(String)` catch-all has 70 sites needing semantic analysis (each site needs
a specific typed variant, not a blanket rename).
**When to do:** Dedicated session. Start with DagError (0 constructor sites — production code
already uses `DagError::*` with `.into()`). Then ProviderError (15 sites). BindingError (8 sites).
ExecutionError last (70 sites, semantic analysis needed).
**Infrastructure ready:** All 4 From impls exist in `error_domains.rs`, tests pass.

### Phase 8c: runner/mod.rs split
**Why deferred:** 2344 LOC, but `run()` already decomposed into sub-methods (finalize_run, etc.).
Tests already in separate `tests.rs` (4820 LOC). Tight internal coupling means meaningful split
requires Phase 14 VerbExecutor restructure, not just mechanical file extraction.

---

## REMAINING GOD FILES

| File | LOC | Action | When |
|------|-----|--------|------|
| `nika/src/main.rs` | 5,530 | Migrate to nika-cli/verbs/ | Phase 16 |
| `nika-core/src/ast/analyzer/analyze.rs` | 5,531 | Split into 11 files | Phase 17 |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Restructure with VerbExecutor | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Source halves tightly coupled | Phase 14+ |

---

## CONFIRMED NON-ISSUES (investigated, not bugs)

1. **Interner (`util/interner.rs:18`)**: Intentionally trivial `Arc::from()`. DashMap was tried —
   80 bytes/entry overhead exceeded 30 bytes/entry savings. Decision documented in docstring.
2. **Drain task in retry loop (`infer.rs:1077`)**: Each drain task exits cleanly when tx drops
   at await boundary. No accumulation, no leak.
3. **`with_event_log()`/`with_policy()` missing `#[must_use]`**: They return `Result<Self>`.
   `Result` is already `#[must_use]` in Rust.
4. **QW-3 mem::take for StreamChunk::Done**: Clone unavoidable — both channel and result.text
   need the string.

## IMPROVEMENT SCANNER RESULTS (0 critical)

- **0 P1**: No critical production issues found
- **2 P2**: `unwrap()` in template.rs regex LazyLock (justified), lower.rs:950 after is_string (justified)
- **4 P3**: `#[allow(dead_code)]` on runner total_tasks field, builtin/run.rs future-proof methods
- **Zero unsafe** in production code (26 unsafe calls are all test env cleanup)
- **Zero TODO/FIXME** describing real bugs

---

## MANDATORY READS (in order)

1. `nika/CLAUDE.md` — project overview, 5 verbs, testing philosophy
2. `tools/nika/CLAUDE.md` — crate architecture, error codes, testing rules
3. `tools/nika-engine/ARCHITECTURE.md` — **NEW**: module map, invariants, historical scaffolding
4. `docs/plans/2026-04-08-constellation-v2-mega-plan.md` — THE PLAN (1820 lines)
   - Section 5: 10 trait specs (now implemented in nika-kernel)
   - Section 18: v2.1 revisions (big-bang OK, bundles, TaskScope splinters)
5. `docs/sprints/CONSTELLATION-FINDINGS-2026-04-08.md` — Audit results from 11 agents
6. Memory: `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/`
   - `project_constellation_session1_session2.md` — Full session log
   - `project_constellation_findings_log.md` — Running findings/non-issues/debt log

---

## NIKA-KERNEL TRAIT MAP (Phase 1 — DONE)

All traits in `tools/nika-kernel/src/`:

| File | Trait | Status | Wire Point |
|------|-------|--------|------------|
| `clock.rs` | `Clock` | Defined | `Instant::now()` in 19+ runtime files |
| `events.rs` | `EventEmitter` (local) | Defined | Mirrors nika-event trait (unify in Phase 14) |
| `filesystem.rs` | `Filesystem` | Defined | 30+ `std::fs`/`tokio::fs` sites |
| `http.rs` | `HttpClient` + DTOs | Defined | 16+ reqwest sites |
| `shell.rs` | `ShellExecutor` + DTOs | Defined | exec.rs, daemon, doctor |
| `store.rs` | `BlobStore` | Defined | CasStore in nika-media |
| `provider.rs` | `Provider` + full DTOs | Defined | RigProvider 9-variant enum |
| `scope.rs` | `TaskScope` + 6 splinters | Defined | RunContext 1995 LOC god struct |

### nika-kernel-mock (Phase 2 — DONE)

| File | Mock | Tests |
|------|------|-------|
| `clock.rs` | `MockClock` | 5 tests (advance, sleep, clone) |
| `filesystem.rs` | `InMemoryFs` | 5 tests (read/write, seed, metadata, remove) |
| `http.rs` | `MockHttpClient` | 5 tests (enqueue, json, error, tracking) |
| `shell.rs` | `MockShell` | 3 tests (ok, fail, tracking) |
| `store.rs` | `MemoryBlobStore` | 5 tests (put/get, exists, stat, delete) |

---

## NEXT PHASES — EXECUTION ORDER

### SESSION 3 — Wire Dead Scaffolding + Crate Extractions

```
Phase 7:  Absorb nika-engine/lsp/ into nika-lsp-core      [2 commits, ~11k LOC moved]
Phase 9:  Extract nika-clock, nika-fs, nika-blob           [3 parallel new crates]
Phase 10: Extract nika-http, nika-exec-runner              [2 parallel new crates]
Phase 11: Create nika-provider + Provider trait cutover     [big-bang, wraps RigProvider]
```

### SESSION 4 — Verb Crates + Runtime

```
Phase 12: Create nika-builtin + #[builtin_tool] via linkme [40+ tools, sealed]
Phase 13: Create nika-verb-* crates (5 verb crates)        [VerbExecutor trait + impls]
Phase 14: Create nika-runtime, decompose RunContext         [TaskScope splinters, bundles]
Phase 15: Extract nika-cache                                [trust-aware keys]
```

### SESSION 5 — CLI + Polish

```
Phase 16: Migrate main.rs → nika-cli/verbs/                [5530→<500 LOC, 1-3 commits]
Phase 17: Split analyze.rs (11 files)
Phase 18: Split nika-tui + wire dead feature flags
Phase 19: Type system hardening (TaskId, SecretString, sealed traits)
Phase 20: Polish + validation
```

### DEDICATED SESSION — error_domains big-bang

```
Phase 6: DagError promotion (pilot — 0 constructor sites)
Phase 6: ProviderError promotion (15 sites)
Phase 6: BindingError promotion (8 sites)
Phase 6: ExecutionError promotion (70 sites — semantic analysis per-site)
```

---

## KEY NUMBERS

| Metric | Before (v0.79.0) | After SESSION 1+2 |
|--------|-------------------|-------------------|
| Workspace crates | 17 | 19 (+nika-kernel, +nika-kernel-mock) |
| Total tests | 10,666 | 10,693 (+27: 4 blanket + 23 mock) |
| Clippy warnings | 0 | 0 |
| God files (>1500 LOC source) | 5 | 3 (transform.rs split, template.rs split) |
| Traits defined | 0 | 10 (in nika-kernel) |
| Mock implementations | 0 | 5 (in nika-kernel-mock) |
| Dead scaffolding wired | 0/3 | 1/3 (EventEmitter blanket impl) |

---

## ARCHITECTURE REFERENCE

### Current Layering (post SESSION 1+2)

```
L0    nika-core (23k)         Pure types, AST, catalogs. Zero I/O.
L0.5  nika-kernel (717)       10 trait defs. Zero impls.          NEW
L0.5  nika-kernel-mock (744)  5 hand-written mocks. Dev-dep.     NEW
L1    nika-event (4.5k)       EventLog + EventEmitter trait + blanket impl
      nika-lsp-core (12k)     LSP intelligence
L2    nika-engine (160k)      MONOLITH — extraction source for ~15 new crates
      nika-media (14k)        CAS store, image ops
      nika-mcp (9k)           MCP client (rmcp)
      nika-vault (1.2k)       Encrypted secrets
      nika-storage (1k)       Storage abstraction
      nika-display (13k)      CLI renderers
L3    nika-daemon (7k)        Background daemon
L4    nika-cli (8k), nika-tui (88k), nika-serve (4k), nika-lsp (2.5k),
      nika-sdk (3k), nika-init (21k)
L5    nika (5.5k)             Binary entry point
```

### Target (Constellation v2.1 complete)

```
L0    nika-core
L0.5  nika-kernel, nika-kernel-mock
L1    nika-clock, nika-fs, nika-http, nika-exec-runner, nika-blob,
      nika-event, nika-macros, nika-shield, nika-lsp-core
L2    nika-provider, nika-builtin, nika-mcp, nika-media, nika-storage,
      nika-vault, nika-verb-{infer,exec,fetch,invoke,agent}
L3    nika-runtime, nika-daemon, nika-cache
L4    nika-cli, nika-tui-{widgets,core,views,app}, nika-lsp, nika-serve,
      nika-sdk, nika-init, nika-display
L5    nika (<500 LOC)
```

**Target crate count: ~38** (currently 19)

---

## RULES — NON-NEGOTIABLE

### Build
- `cargo test --workspace --lib` after EVERY commit (from `tools/nika/`)
- `cargo clippy --workspace -- -D warnings` — zero warnings
- Never `cargo test` without `--lib` (triggers macOS Keychain popups)

### Git
- 1 logical change = 1 commit
- Format: `refactor(arch): <what>` or `fix(scope): <what>` or `feat(scope): <what>`
- Co-author ALWAYS: `Nika 🦋 <nika@supernovae.studio>` (NEVER Claude/Anthropic)
- Do NOT push unless explicitly asked
- Stage specific files, never `git add -A`

### Architecture
- **CONNECT, DO NOT DELETE.** Dead scaffolding gets wired, not removed.
- Big-bang cutovers OK (v0 = zero users)
- No crate > 15k LOC source (excluding tests)
- No file > 1500 LOC (target; template/mod.rs at 2053 is a known exception)
- Every side effect behind a trait
- Tests move WITH their code

### What You May NOT Touch
- The 5 verbs (infer, exec, fetch, invoke, agent) — NEVER change
- Schema version `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Shield files (merged, stable) — only if directly needed

---

## VERIFICATION COMMAND

```bash
cargo test --workspace --lib 2>&1 | grep "test result" && cargo clippy --workspace -- -D warnings 2>&1 | tail -5
```

Expected: `10,693+ passed; 0 failed` + clean clippy.

---

**Start with Phase 7 (LSP absorption) or Phase 9 (effect crate extractions).**
**The kernel traits are ready — now wire them into real implementations.**
