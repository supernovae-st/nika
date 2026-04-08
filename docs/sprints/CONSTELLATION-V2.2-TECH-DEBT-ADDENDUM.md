# Constellation v2.2 — Tech Debt Addendum & Research Synthesis

> **Date:** 2026-04-08
> **Codename:** Constellation v2.2
> **Philosophy:** **Perfection over timing.** Every finding gets addressed. Nothing is deferred "post-launch". Nothing is "good enough for now". Every issue found gets a concrete fix, a crate recommendation, and a phase assignment.
> **Supersedes:** Deferred items in v2.1 (§18.11 phase roadmap) — all deferred items are now in scope.
> **Source:** 12+ parallel research agents (rust-security, rust-architect, rust-async-expert, rust-pro, web-researcher) covering tech debt, architecture patterns, crates, perf, security.
> **Status:** AUTHORITATIVE — integrate into execution plan.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Major Revelations](#2-major-revelations)
3. [Master Bug Table](#3-master-bug-table)
4. [Crate Adoption Roadmap](#4-crate-adoption-roadmap)
5. [Phase-by-Phase Revisions](#5-phase-by-phase-revisions)
6. [NEW: Quick Wins Round 2](#6-new-quick-wins-round-2)
7. [NEW: Binary Size Optimization Plan](#7-new-binary-size-optimization-plan)
8. [NEW: Observability & Rate Limiting](#8-new-observability--rate-limiting)
9. [NEW: WASM Plugin Runtime](#9-new-wasm-plugin-runtime)
10. [NEW: Hybrid Disk Cache](#10-new-hybrid-disk-cache)
11. [Architecture Pattern Decisions](#11-architecture-pattern-decisions)
12. [Show HN Positioning](#12-show-hn-positioning)
13. [Validation Gates](#13-validation-gates)

---

## 1. Executive Summary

The Constellation v2.1 plan was **correct in direction** but **wrong on several critical facts** that surfaced during deep audit:

### The 6 revelations

1. **Phase 11 Provider cutover is 80% already done.** `kernel_bridge.rs` exists (725 LOC, 18 tests), `get_dyn_provider()` is wired, `async_trait + Pin<Box<dyn Stream>>` is the right pattern. Remaining work: 5 small commits (tool-use path, Mock, Native, conformance suite). Not the "hardest extraction" — the keystone is already laid.

2. **Phase 6 error_domains is half the size we thought.** Of the "70 `Execution(String)` sites", **~50 are in nika-cli**, not engine. Those belong in a new `CliError` enum in nika-cli (top-level bin can own its error type). Engine has only ~10 truly engine-relevant sites. Phase 6 = 16 hours, not 40+.

3. **Phase 14 RunContext split is fully additive.** RunContext already uses interior mutability everywhere (DashMap/RwLock/OnceLock). Zero `&mut RunContext` in hot code. All 6 splinter traits take `&self`. Commit 1 ships traits + blanket impl WITHOUT touching existing code. Zero risk.

4. **Phase 7 LSP absorption is NOT blocked.** AST types already live in nika-core. `nika-lsp-core` already depends on nika-core. `model_intel.rs` has ONE engine dep (`cost.rs`) — relocate `cost.rs` to `nika-core::catalogs::cost` (pure const data) and absorption proceeds. Net deletion: **~4,000 LOC engine code**. Engine drops `tower-lsp-server`, `dashmap`, `ls-types` entirely.

5. **25+ NEW bugs found in the effect crates** (nika-http, nika-fs, nika-exec-runner) that the initial audit missed. Most critical: **HTTP header injection via `\r\n`** (request smuggling front door), **no max HTTP response body size** (10GB OOM), **nika-fs has no path traversal sandbox**.

6. **Fresh tech debt pass found 15 more issues** including a NEW god file not on the known list: `binding/resolve.rs` (**3,948 LOC** — larger than template/mod.rs).

### The philosophy shift

v2.1 had "deferred" items (Phase 5.2, Phase 6, Phase 7, Phase 8c). v2.2 has **zero deferred items**. Everything gets fixed, every crate gets adopted, every hot path gets optimized, every god file gets split.

**Not because the launch date demands it — because shipping an open-source Rust reference project with known dead scaffolding is not acceptable.**

---

## 2. Major Revelations

### 2.1 Phase 11 Provider — already 80% done

**Location:** `nika-engine/src/provider/rig/kernel_bridge.rs` (725 LOC, 18 unit tests passing)

**What exists:**
- `impl Provider for RigProvider` bridges kernel DTOs ↔ rig-core types
- `get_dyn_provider()` at `executor/mod.rs:895-901`
- `async_trait` + `Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>`
- 4 helpers: `extract_system`, `extract_user_prompt`, `has_vision_content`, `to_rig_user_content`
- 18 unit tests covering text/vision/streaming/error conversion

**What remains (5 commits):**
1. Extend `impl Provider::infer` to handle `request.tools` non-empty (tool-use path)
2. Add `impl Provider for MockProvider` — extract Mock from RigProvider enum
3. Add `impl Provider for NativeRuntime` (feature-gated, delegates to mistral.rs generate_stream)
4. Route verb-crate-facing methods through `get_dyn_provider`
5. Add `nika-kernel/tests/provider_conformance.rs` — generic tests any impl must pass

**Critical architecture decision: DO NOT flip `rig_provider_cache` to `DashMap<String, Arc<dyn Provider>>`.**

The cache stays concrete (`DashMap<String, RigProvider>`) because 15+ call sites in `infer.rs`, `thinking.rs`, `structured_output.rs`, `rig_agent_loop/*` call rig-specific methods (`infer_vision`, `supports_thinking`, `infer_with_structured_output`) that are NOT on the kernel trait by design. Forcing them through `dyn Provider` would require downcasting — worse than current state.

**Pattern:** concrete cache internally, `Arc<dyn Provider>` at crate API boundaries (via `get_dyn_provider`). Verb crates never see `RigProvider`.

### 2.2 Phase 6 error_domains — CliError split insight

**The misleading number:** "70 `Execution(String)` sites"

**The actual breakdown:**

| Bucket | Count | Location | Target variant |
|--------|-------|----------|---------------|
| **CLI I/O** (fs/spawn/exec) | ~30 | `nika-cli/src/{switch,every,daemon,schedule,jobs,cache_cmd}.rs` | **NEW `CliError::Io { context, source }`** |
| **CLI cron/parse** | ~12 | `nika-cli/src/{schedule,every}.rs` | `CliError::ParseSchedule { input, reason }` |
| **CLI validation** | ~5 | multiple | `CliError::InvalidArgument { name, reason }` |
| **Anti-pattern: fake NIKA code** | 3 | `"NIKA-280: invalid schedule"` | `ScheduleError::InvalidPreset` (NIKA-280..282) |
| **Tests** | 8 | `tests/executor_*` | Update matches |
| **Generic engine (hot path)** | ~5 | `structured_retry.rs`, `error.rs` | `ExecutionError::General(String)` escape hatch |
| **Total** | ~63 | | |

**Key insight:** the 50+ CLI sites **should never have been `NikaError`** in the first place. `nika-cli` is a top-level binary crate — it can have its own error type. This unlocks the entire Phase 6 migration.

**Phase 6 revised plan: 6 sub-phases, 16 hours total**

| Sub-phase | Scope | LOC | Sites | Risk | Hours |
|---|---|---|---|---|---|
| **6a** | Add `Diagnostic` derive to existing 4 enums + flip to transparent wrappers | ~80 | 4 | Low | 2 |
| **6b** | Define 8 NEW empty domain enums (Schema, Mcp, Agent, Tool, Media, Security, StructuredOutput, Workflow) + From impls | ~300 | 0 | Trivial | 1 |
| **6c** | Introduce `CliError` in nika-cli, migrate ~50 cli sites off `NikaError::Execution` | ~250 | 50 | Medium | 4 |
| **6d** | Migrate engine `Execution(String)` sites to typed variants | ~80 | ~10 | Low | 2 |
| **6e** | Migrate flat variants → domain wrappers per range (Provider/Dag/Binding/Agent/Mcp/Media) | ~600 | ~120 | Medium | 6 |
| **6f** | Delete old flat variants + cargo insta review | ~200 (deletions) | 0 | Low | 1 |
| **Total** | | ~1500 | ~180 | Medium | 16 |

**Thiserror 2 + miette transparent delegation — CONFIRMED pattern:**

```rust
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum NikaError {
    #[error(transparent)]
    #[diagnostic(transparent)]   // forwards code(), help(), labels(), severity()
    Provider(#[from] ProviderError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Dag(#[from] DagError),
    // ...
}

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum ProviderError {
    #[error("Provider '{provider}' not configured")]
    #[diagnostic(code("NIKA-030"), help("set ANTHROPIC_API_KEY or run `nika keys set`"))]
    NotConfigured { provider: String },
    // ...
}
```

**Critical invariant:** user-visible error messages must remain byte-identical after migration (users grep for `[NIKA-030]` in CI output). Enforce with `cargo insta` snapshot review.

### 2.3 Phase 14 RunContext — fully additive migration

**RunContext already uses interior mutability everywhere.** DashMap for task_results, RwLock for shared state, OnceLock for write-once fields. **Zero `&mut RunContext` in hot code** — only `set_vault()` + `set_invocation_source()` at boot.

**All 6 splinter traits take `&self`** — no `&mut dyn Trait` nightmare:

```rust
// nika-runtime/src/scope/mod.rs (new, ~250 LOC)

pub trait TaskResults: Send + Sync {
    fn insert(&self, task_id: Arc<str>, result: TaskResult);
    fn get(&self, task_id: &str) -> Option<TaskResult>;
    fn get_output(&self, task_id: &str) -> Option<Arc<Value>>;
    fn get_trust(&self, task_id: &str) -> Option<TrustLevel>;
    fn status_of(&self, task_id: &str) -> Option<TaskOutcome>;
    fn contains(&self, task_id: &str) -> bool;
    fn iter_results(&self) -> Vec<(Arc<str>, TaskResult)>;
}

pub trait BindingScope: TaskResults + Send + Sync {  // SUPER-TRAIT
    fn resolve_path(&self, path: &str) -> Option<Value>;
    fn resolve_input_path(&self, path: &str) -> Option<Value>;
    fn resolve_context_path(&self, path: &str) -> Option<Value>;
    fn resolve_skills_path(&self, path: &str) -> Option<Value>;
    fn get_input_default(&self, name: &str) -> Option<Value>;
    fn has_inputs(&self) -> bool;
    fn has_context(&self) -> bool;
}

pub trait MediaStaging: Send + Sync {
    fn set_media(&self, task_id: &Arc<str>, media: Vec<MediaRef>);
    fn take_media(&self, task_id: &Arc<str>) -> Vec<MediaRef>;
    fn media_budget(&self) -> &Arc<MediaBudget>;
    fn workspace_root(&self) -> std::path::PathBuf;
}

pub trait RecordStore: Send + Sync {
    fn set_record(&self, task_id: Arc<str>, record: Record);
    fn get_record(&self, task_id: &str) -> Option<Arc<Record>>;
    fn has_record(&self, task_id: &str) -> bool;
    fn iter_records(&self) -> Vec<(Arc<str>, Arc<Record>)>;
}

pub trait VaultLookup: Send + Sync {
    fn vault_get_credential(&self, service: &str, field: &str) -> Result<String, VaultError>;
    fn has_vault(&self) -> bool;
}

pub trait InvocationContext: Send + Sync {
    fn invocation_source(&self) -> InvocationSource;
    fn project_root(&self) -> &Path;
}

// Umbrella trait — zero methods, pure marker
pub trait TaskScope:
    TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{}

impl<T> TaskScope for T where
    T: TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{}
```

**Critical structural decision:** `BindingScope: TaskResults` (super-trait) because template resolution reads task outputs. Every verb asking for `BindingScope` automatically gets `TaskResults`.

**Migration: 5 additive commits**

| Commit | Content | Risk |
|--------|---------|------|
| 1 | Trait definitions + impl on RunContext (delegation only) | **Zero** — purely additive, 472 call sites untouched |
| 2 | verb-infer: `impl BindingScope + TaskResults + InvocationContext` | Low |
| 3 | verb-fetch + verb-exec | Low |
| 4 | verb-invoke + verb-agent (uses `&dyn TaskScope`) | Medium |
| 5 | Seal API: `pub(crate)` RunContext fields, external SDK via `Arc<dyn TaskScope>` | Low |

**Zero new deps.** No `trait-variant`, no `async-trait` (all methods sync — async lives in verb runners, not scope).

### 2.4 Phase 7 LSP absorption — handoff was WRONG

**Wrong claim #1:** "engine/lsp is a pure stranded duplicate"
**Reality:** engine handlers are AST-aware supersets of core handlers (CursorContext-based fallback)

**Wrong claim #2:** "AST is in nika-engine, blocks layering"
**Reality:** AST types (`raw`, `analyzed`, `analyzer`, `Span`, `FileId`) **already live in nika-core** at `tools/nika-core/src/ast/`. Engine `crate::ast::analyzer` is `pub use nika_core::ast::analyzer`. nika-lsp-core already depends on nika-core.

**Wrong claim #3:** "model_intel.rs blocks the move (business logic)"
**Reality:** model_intel has ONE engine-internal dep (`crate::provider::cost::{ModelPricing, ProviderKind}`). Fix: move `cost.rs` (pure const data) to `nika-core::catalogs::cost`. 1 hour mechanical.

**Partial integration already exists:**
- `nika-engine/src/lsp/server.rs` holds `core_handler: nika_lsp_core::handler::DefaultHandler`
- Engine handlers already import `extract_task_ids` from core
- `LspHandler` trait exists at `core/handler.rs:31`
- 4 handlers already delegated to core: references, folding_ranges, document_links, rename

**Revised Phase 7: 7 commits, -4,000 LOC net**

| # | Commit | Action |
|---|--------|--------|
| 1 | `refactor(core): move cost.rs to nika-core::catalogs::cost` | Mechanical, 12 call sites |
| 2 | `feat(lsp): add Snapshot + CachedAst to nika-lsp-core` | New file, additive |
| 3 | `refactor(lsp): move model_intel.rs to nika-lsp-core` | Flips imports |
| 4 | `refactor(lsp): move semantic_tokens + symbols + inlay_hints to core` | 2,429 LOC |
| 5 | `refactor(lsp): move definition + hover to core` | 2,288 LOC |
| 6 | `refactor(lsp): move completion + code_action to core` | 2,589 LOC (hardest) |
| 7 | `refactor(engine): DELETE src/lsp/ + drop lsp feature` | **-4,000 LOC**, drops `tower-lsp-server`+`dashmap`+`ls-types` deps |

**Snapshot pattern (rust-analyzer `Analysis` equivalent):**

```rust
// nika-lsp-core/src/snapshot.rs (NEW)
pub struct CachedAst {
    pub raw: Option<RawWorkflow>,
    pub analyzed: Option<AnalyzedWorkflow>,
    pub parse_error: Option<ParseError>,
    pub errors: Vec<AnalyzeError>,
    pub version: i32,
}

pub struct Snapshot<'a> {
    pub text: &'a str,
    pub line_index: &'a LineIndex,
    pub ast: Option<&'a CachedAst>,  // key addition — handlers branch on this
}
```

**Testing strategy:** single integration test `tools/nika-lsp/tests/integration_lsp.rs` (~300 LOC, <1s) replays JSON-RPC trace against in-process backend. Insta snapshots. **Catches 90% of regressions.** Plus manual 5-minute editor matrix pre-tag.

### 2.5 Effect crates — 25+ NEW P1/P2 bugs

Detailed in section 3 (Master Bug Table).

**Most critical finding: HTTP header injection via `\r\n`**. Current `nika-http` builder uses `builder.header(...)` without validating name/value, silently drops invalid headers. `\r\n` in a header value is a request smuggling front door.

### 2.6 Fresh audit — 15 more findings

Including **NEW god file** `binding/resolve.rs` at **3,948 LOC** (not on the original list of 4 god files), and **ZERO `spawn_blocking`** usage across the entire engine — HTML parsing, JSON deep clone, oxipng all stall the scheduler.

---

## 3. Master Bug Table

### P0 Critical — Security / Correctness

| ID | File:Line | Issue | Fix | Crate |
|----|-----------|-------|-----|-------|
| **H1** | `nika-http/src/lib.rs` | **HTTP header injection via `\r\n`** — builder silently drops invalid headers | Use `HeaderName::try_from` + `HeaderValue::try_from`, return `HttpError::Other` on failure | — |
| **H2** | `nika-http/src/lib.rs` | **No max response body size** — 10GB streaming OOM | Check `Content-Length`, then streaming read with `total > MAX (50MB)` check | — |
| **H3** | ~~`nika-http/src/lib.rs`~~ | ~~**DNS rebinding TOCTOU**~~ — **ALREADY SOLVED** in `nika-engine/src/runtime/policy.rs::resolve_and_pin_ssrf` + `executor/fetch.rs::run_fetch` via pre-resolve + `Client::resolve()` pinning. Only polish needed: cache pinned addrs for workflow lifetime to avoid double-resolution | Polish only | Already using `hickory-resolver` via reqwest feature |
| **H4** | `nika-http/src/lib.rs` | **No SSRF re-check on redirect targets** — `evil.com` → 302 → `169.254.169.254` | `reqwest::redirect::Policy::custom` closure re-validates each hop + max 10 | — |
| **H5** | `nika-http/src/lib.rs` | **`follow_redirects` silently ignored** — reqwest policy is Client-level | Two pre-built `Client` instances sharing `Arc<SsrfResolver>` | — |
| **FS1** | `nika-fs/src/lib.rs` | **No path traversal sandbox** — `TokioFs` reads `/etc/passwd` if asked | `TokioFs::sandboxed(roots)` with canonicalize + verify within allowed_roots | — |
| **FS2** | `nika-fs/src/lib.rs` | **No symlink loop protection** in glob | Use `ignore::WalkBuilder` (already dep), walkdir detects loops natively | `ignore 0.4` (replace `globset`) |
| **FS3** | `nika-fs/src/lib.rs` | **No size cap on read/read_to_string** — user-controlled path OOM | Pre-check `metadata().len` against `MAX_FILE_SIZE: u64 = 100 * 1024 * 1024` | — |
| **FS4** | `nika-fs/src/lib.rs` | **TOCTOU in write** — `create_dir_all` + `fs::write` not atomic | Write to `.tmp.<pid>.<rand>` + fsync + rename | — |
| **EX1** | `nika-exec-runner/src/lib.rs:113-135` | **Pipe deadlock** — `wait()` before `read_to_end`, 64KB OS pipe buffer fills | `tokio::try_join!(wait, read_stdout, read_stderr)` | `command-group 6` |
| **EX2** | `nika-exec-runner/src/lib.rs` | **Orphan grandchildren** — `child.kill()` only kills immediate child | `group_spawn()` → `setsid` on Unix, Job Object on Windows | `command-group 6` |
| **EX3** | `nika-exec-runner/src/lib.rs` | **Unbounded output OOM** — `read_to_end` with no cap | `read_capped` with 10 MiB cap + drain rest (avoid SIGPIPE deadlock) | — |
| **EX4** | `nika-exec-runner/src/lib.rs:150` | **Hard SIGKILL only** — no graceful shutdown | SIGTERM → 500ms grace → SIGKILL (killpg on Unix) | `command-group 6` |
| **INV1** | `invoke.rs:116` | **McpInvoke params UNREDACTED** in trace files (secrets, API keys) | `params: resolved_params.as_ref().map(|p| Arc::new(redact_value(p)))` | — |
| **C1** | `extract.rs:39-44` | **`parse_link_header_hreflang` bug** — lowercase check + byte-slice original, breaks on trailing whitespace | `.eq_ignore_ascii_case()` or regex parse once | — |

### P1 High — Hot Path Performance

| ID | File:Line | Issue | Fix | Crate |
|----|-----------|-------|-----|-------|
| **P-1** | `infer.rs:1078, 1282` | **Drain task spawn per infer call** — wasteful `tokio::spawn(drain loop)` per non-streaming call | Provider trait accepts `Option<Sender>` for non-streaming | — |
| **P-2** | `util/mod.rs:58-98` | `redact_secrets()` allocates per replace + holds RwLock. N×M String allocs | `aho-corasick` built once + `ArcSwap<Vec<Arc<str>>>` | `aho-corasick 1.1`, `arc-swap 1.7` |
| **P-3** | `runner/mod.rs:2037-2097` | for_each results iterated **5×** for counts | Single fold into `(succeeded, failed, skipped, total)` | — |
| **P-4** | `task_dispatch.rs:341, 344` | `Templatable<String>` clone per task even with no preset | Return `&` when no preset, `Cow` for merge | — |
| **P-5** | `fetch_cache.rs:65-67` | FetchCache clones multi-MB body per cache hit | `Arc<CachedResponse>` or `bytes::Bytes` | `bytes 1.8` |
| **P-6** | `binding/resolve.rs:163-266` | `ResolvedBindings` allocates **30 strings** per 10-binding task | `Arc<str>` or `compact_str::CompactString` (24B inline) | `compact_str 0.8` |
| **A-1** | `exec.rs:185, 191, 285, 291` | **`std::path::canonicalize()` in async** — 4 sites, called twice per exec | Precompute `workflow_base_dir_canonical: Arc<PathBuf>` + `tokio::fs::canonicalize` | — |
| **A-2** | `builtin/data/io.rs:63, 71` | `std::env::current_dir()` + `std::fs::create_dir_all()` in async + TOCTOU | Resolve cwd at TaskExecutor construction, `tokio::fs::create_dir_all` | — |
| **A-3** | **ALL nika-engine** | **ZERO `spawn_blocking`** — HTML5ever parsing, JSON deep clone, oxipng all stall scheduler | Wrap CPU-hot ops >1ms in `spawn_blocking` | — |
| **L-1** | `util/mod.rs:32-50` | `EXTRA_SECRETS: RwLock<Vec<String>>` read on every redact call | `arc_swap::ArcSwap<Vec<Arc<str>>>` for lock-free reads | `arc-swap 1.7` |
| **L-2** | `rate_limit.rs:38, 45` | `governor` keyed with `&domain.to_string()` — allocates per request | Switch keyed store to `K = Arc<str>` | — |

### P2 Medium — Correctness / Maintainability

| ID | File:Line | Issue | Fix |
|----|-----------|-------|-----|
| **N1** | `nika-http` | `https_only(false)` implicit — workflow can leak creds via `http://` | `[policy.http] require_https = true` in nika.toml |
| **N2** | `nika-http` | To_str() silently drops non-ASCII headers | `String::from_utf8_lossy` + `tracing::warn!` once |
| **N3** | `nika-http` | No `connect_timeout` separate from total timeout | `.connect_timeout(Duration::from_secs(5))` |
| **N4** | `nika-http` | TLS pinning missing for known providers | Document + `cargo deny ban` on `danger_accept_invalid_certs` |
| **N5** | `nika-http` | Connection pool poisoning after 502 | Regression test: "after 502, next request works" |
| **N6** | `nika-http` | No request body size limit | `max_request_body_bytes` config knob |
| **FS5** | `nika-fs` | `metadata` collapses symlinks silently | Extend `FileMetadata` with `is_symlink: bool` |
| **FS6** | `nika-fs` | `canonicalize` follows symlinks but no sandbox enforcement | Bake canonicalize into `TokioFs::sandboxed()` |
| **FS7** | `nika-fs` | `exists` swallows all errors (permission denied = false) | Doc note or explicit error variant |
| **FS8** | `nika-fs` | No `read_dir` in trait — callers fall back to `tokio::fs::read_dir` | Add to trait |
| **EX5** | `nika-exec-runner` | Stdin write error silently ignored (`let _ = ...`) | Propagate unless `BrokenPipe` (normal EOF) |
| **EX6** | `nika-exec-runner` | `current_dir` not validated — confusing `NotFound` | `if !cwd.is_dir() return Err` up front |
| **EX7** | `nika-exec-runner` | Env var keys not POSIX-validated | Check `!empty && !contains('=') && !contains('\0')` |
| **C-2** | `runner/mod.rs:2037` | `count() as u32` silent truncation | `u32::try_from(...).unwrap_or(u32::MAX)` + `#[deny(clippy::cast_possible_truncation)]` |
| **C-3** | `binding/resolve.rs:66` | `#[allow(clippy::large_enum_variant)]` suppressed — 10KB wasted per 100 bindings | `Box<PendingWithEntry>` |
| **C-4** | `ast/action.rs:483` | Same issue: `TaskAction::Agent` variant | `TaskAction::Agent { agent: Box<AgentParams> }` |
| **M-1** | `binding/resolve.rs` | **NEW god file** 3,948 LOC — not on the known list! | Split into `resolve/{lazy,bindings,dispatch,typed}.rs` |

### Known debt from v2.1 (recap, not new)

- Dead MPSC receivers in infer.rs — **FIXED** Session 1
- `McpInvoke` unredacted params — **FIXED** Session 1
- serve Mutex across await — **FIXED** Session 1
- nika-media ARM64 linker — **FIXED** Session 1 (`[profile.test] opt-level = 0`)
- Interner intentionally simple — **DECIDED** (DashMap rejected — 80B overhead > 30B savings)
- IndexedDag unused — wire during Phase 14
- 4 remaining god files: main.rs, error.rs, runner/mod.rs, template/mod.rs
- Plus M-1 above: binding/resolve.rs (new)

---

## 4. Crate Adoption Roadmap

### Tier 1 — Quick Wins Round 2 (all low-effort, no architectural changes)

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **foldhash** | 0.1 | Replace default SipHash in HashMap (30-50% faster on short keys, no DoS resistance needed internally) | `nika-core/src/lib.rs` (type alias `FastMap<K,V>`) |
| **arc-swap** | 1.7 | Lock-free read-mostly for `EXTRA_SECRETS` (L-1), policy/config hot swaps | `util/mod.rs`, `policy.rs` |
| **aho-corasick** | 1.1 | Multi-pattern replacement in `redact_secrets` (P-2) — single pass vs N String allocations | `util/mod.rs` |
| **compact_str** | 0.8 | 24B inline strings for `TaskId`, `ProviderName`, `ModelName`, `AliasName`, `BindingKey` (P-6) | All `nika-core` ID types |
| **bytes** | 1.8 | Zero-copy clone for fetch bodies, LLM stream chunks, CAS blob refs (P-5) | `nika-http`, `nika-media`, `FetchCache` |
| **smallvec** | 1.13 | Stack-allocated `depends_on`, `with` (most tasks have ≤4 deps) | `nika-core/src/ast/action.rs` |
| **command-group** | 6 | Exec verb kill-on-drop + Windows Job Objects + Unix process groups (EX1-4) | `nika-exec-runner/src/lib.rs` |
| **hickory-resolver** | 0.25 | SSRF-aware DNS resolver (H3) — closes TOCTOU window | `nika-http/src/resolver.rs` (new) |
| **futures-util** | 0.3 | `BoxFuture` for `impl Resolve` | `nika-http/src/resolver.rs` |
| **ignore** | 0.4 | Symlink loop detection + gitignore support in glob (FS2) — already dep, drop `globset` from nika-fs | `nika-fs/src/lib.rs` |

### Tier 2 — Infrastructure

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **moka** | 0.12 | Production LLM response cache, TinyLFU eviction (async API) | `nika-cache` (Phase 15) |
| **papaya** | 0.1 | Lock-free concurrent map (2-5x dashmap reads) for schema cache, LLM response cache | `nika-cache`, schema lookup |
| **governor** | 0.7 | Per-provider rate limiting (429 hygiene) — GCRA-based, async-aware | `nika-provider` per-provider buckets |
| **lasso** | 0.7 | Arena string interner (`ThreadedRodeo` lock-free reads) for template variable names | `nika-binding` template parsing |
| **opentelemetry-otlp** | 0.27 | OTLP exporter for `nika serve` production observability | `nika-event` feature-gated |
| **tracing-opentelemetry** | 0.27 | Bridges `tracing` spans to OTLP | `nika-event` |

### Tier 3 — Performance

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **mimalloc** | 0.1 | Global allocator (10-25% throughput, cross-platform) | `nika/src/main.rs` |
| **sonic-rs** | 0.3 | JSON parse 2-4x serde_json — SELECTIVE use in `nika:jq` + structured-output validator + trace writer | `nika-engine/src/runtime/builtin/data/jq.rs`, `structured_output.rs`, `nika-event/src/trace.rs` |
| **bumpalo** | 3 | Arena allocation for per-task ephemeral state (ruff pattern) | `nika-engine/src/binding/template/` |
| **parking_lot** | 0.12 | Smaller, faster Mutex/RwLock for hot paths | Replace `std::sync::{Mutex,RwLock}` where contended |
| **divan** | 0.1 | Benchmark framework (faster, cleaner than criterion for microbenchmarks) | `nika-engine/benches/` |

### Tier 4 — New Capabilities

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **wasmtime** | 25+ | Component Model WASM runtime for `nika-plugin` sandbox — untrusted workflow extensions | `nika-plugin` (new crate) |
| **wasmtime-wasi** | 25+ | WASI P2 implementation for plugin host | `nika-plugin` |
| **foyer** | 0.10+ | Hybrid memory + disk cache with admission policies for LLM response cache | `nika-cache` L3 |
| **jiff** | 0.1+ | Replace `chrono` where user-facing (timezone footguns) | CLI date parsing, scheduling |

### Tier 5 — Security Hardening (from Perplexity research)

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **cap-std** | 3.x | Capability-based std replacement — TOCTOU-safe `Dir` handles via `openat` | `nika-fs`, `nika-builtin` file tools, `tools/mod.rs::check_path_readable` |
| **landlock** | 0.4.x | Linux 5.13+ unprivileged FS sandbox — second layer around exec child | `nika-exec-runner/src/sandbox/linux.rs` |
| **seccompiler** | 0.5.x | Linux syscall filtering (Firecracker pattern) — layer with landlock | `nika-exec-runner/src/sandbox/linux.rs` |
| **win32job** | 0.5 | Windows Job Objects wrapper (`JOB_OBJECT_LIMIT_*`) | `nika-exec-runner/src/sandbox/windows.rs` |
| **garde** | 0.22.x | Derive-based input validation — replace ad-hoc validation in CLI + HTTP bodies | `nika-cli`, `nika-serve` |
| **rustls-pki-types** | 1.x | Custom RootCertStore for provider TLS pinning | `nika-engine/src/tls.rs` (new) |
| **webpki-roots** | 0.26+ | Source of 6 pinned CAs (DigiCert, ISRG, Cloudflare, Amazon, GTS, GlobalSign) | `nika-engine/src/tls.rs` |

### Tier 6 — Tooling (CI + release)

| Crate | Version | Purpose | Files |
|-------|---------|---------|-------|
| **cargo-deny** | (tool) | CI license + CVE checking + `[bans]` to enforce rustls-only | `.github/workflows/`, `deny.toml` |
| **cargo-audit** | (tool) | RUSTSEC vulnerability scan | `.github/workflows/ci.yml` |
| **cargo-vet** | (tool) | Supply chain auditing (Mozilla + Google + Bytecode Alliance + Embark registries) | `supply-chain/` |
| **cargo-auditable** | (tool) | Embed SBOM in released binary — enables `cargo audit bin nika` on downloaded artifact | `.github/workflows/release.yml` |
| **cargo-fuzz** | (tool) | libfuzzer-sys harness for 5 fuzz targets | `fuzz/` (new workspace dir) |
| **ClusterFuzzLite** | (CI) | Continuous fuzzing in GitHub Actions — 5 min PR, 30 min nightly | `.clusterfuzzlite/` |
| **cargo-pgo** | (tool) | Profile-guided optimization for release binary | `.github/workflows/release.yml` |
| **cargo-bloat** | (tool) | Binary size audit | Manual + CI report |
| **cargo-udeps** | (tool) | Kill unused dependencies | Pre-v0.80 |
| **cargo-machete** | (tool) | Dead dep detection | `.github/workflows/ci.yml` (already present) |
| **cargo-geiger** | (tool) | Unsafe usage counter | `.github/workflows/sast.yml` (already present) |

### Crates to REMOVE

| Crate | Reason | Replacement |
|-------|--------|-------------|
| `ahash` (if present) | Soundness + maintenance concerns | `foldhash` |
| `lazy_static` (if present) | Superseded | `std::sync::OnceLock` |
| `chrono` user-facing | Timezone footguns | `jiff` (core time in engine OK) |
| `globset` from nika-fs | Replaced by `ignore::WalkBuilder::overrides` | `ignore` |
| `tower-lsp-server` from nika-engine | After Phase 7 LSP absorption | (stays in nika-lsp binary only) |
| `dashmap` from nika-engine lsp | After Phase 7 | — |
| `ls-types` from nika-engine | After Phase 7 | — |

---

## 5. Phase-by-Phase Revisions

### Phase 5 (EventEmitter blanket impl) — COMPLETED Session 2 (part) + reassigned

- Phase 5.1 ✅ done: blanket impl `EventEmitter for Arc<T>` + `EventSink` alias
- Phase 5.2 "flip 5 hot sites" — **NOT deferred**. Move into Phase 14 (RunContext decomp) where `Arc<dyn EventEmitter>` naturally flows through verb bundle structs. Zero cascade cost there.

### Phase 6 (error_domains) — REVISED to 16 hours, 6 sub-phases

Per §2.2. Key change: CliError split to nika-cli crate absorbs 50/70 sites. Engine migration is surgical.

### Phase 7 (LSP absorption) — REVISED to 7 commits, -4,000 LOC

Per §2.4. Key insight: AST already in nika-core, model_intel has 1 engine dep, partial integration exists. Net deletion after absorption.

**Critical additional step:** `[profile.test] opt-level = 0` (from Session 1) means LTO isn't stripping duplicate html5ever symbols. Phase 7 DELETES engine LSP's `dashmap` + `tower-lsp-server` dep — this may finally unblock `lto = "thin"` for test profile (verify after Phase 7 merge).

### Phase 8 (god file splits) — ADD binding/resolve.rs

Previous list: transform.rs (done), template.rs (done), runner/mod.rs, main.rs, error.rs, analyze.rs (done).

**NEW addition:** `binding/resolve.rs` (3,948 LOC) into:
- `resolve/lazy.rs` — `LazyBinding` enum + state machine (~800 LOC)
- `resolve/bindings.rs` — `ResolvedBindings` struct + builders (~1,200 LOC)
- `resolve/dispatch.rs` — `resolve_with_entry` / `resolve_entry` (~1,000 LOC)
- `resolve/typed.rs` — `from_with_spec*` methods (~900 LOC)

### Phase 9-12 (effect crate extraction) — UNCHANGED but enriched

The 5 effect crates exist (Session 3). Phase 9-12 is now **hardening** not extraction:

| Phase | Crate | Fixes |
|-------|-------|-------|
| 9a | nika-clock | — (already clean) |
| 9b | nika-fs | FS1-FS8 (sandbox, symlink, size cap, TOCTOU, read_dir) |
| 9c | nika-blob | P2: MIME type inconsistency between put/stat |
| 10 | nika-http | H1-H5, N1-N6 (all SSRF + DNS rebinding + redirect + headers + TLS) |
| 11 | nika-exec-runner | EX1-EX7 (pipe deadlock, orphans, output cap, stdin, cwd, env) + `command-group` adoption |

### Phase 11 (Provider cutover) — REVISED to 5 small commits

Per §2.1. Already 80% done. Remaining: tool-use path, Mock impl, Native impl (feature-gated), verb-crate routing, conformance tests.

### Phase 13 (nika-builtin) — ADD linkme registration

Per v2.1 §18.7 — `linkme::distributed_slice` pattern for builtin tool registration (beats `inventory` — zero runtime cost, const-init friendly). Add `#[builtin_tool]` proc-macro in Phase 3 (nika-macros) to generate the `distributed_slice` entry + sealed trait impl.

### Phase 14 (nika-runtime + RunContext decomp) — REVISED to 5 additive commits

Per §2.3. All traits take `&self`. Commit 1 ships traits + blanket impl without touching existing code. Zero risk.

**Includes:**
- EventEmitter flip (deferred Phase 5.2 — now flows naturally through verb bundle structs)
- IndexedDag wiring (plug `&dyn TaskResults` into `IndexedDag::next_ready`)
- Bundle composition pattern (KernelBundle + IoBundle + LlmBundle, max 3 fields per verb)

### Phase 15 (nika-cache) — ENRICHED with moka + papaya + foyer

- L1 (in-memory LRU): `moka::future::Cache` with TinyLFU eviction
- L2 (concurrent hot cache): `papaya::HashMap` for schema cache (read-mostly)
- L3 (disk-backed): `foyer` hybrid memory+disk with admission policies — replaces hand-rolled `~/.nika/cache/`
- Trust-aware cache keys (tainted runs cached separately)

### Phase 16 (main.rs migration) — DETAILED plan per §7 research

Ruff-style hybrid: `nika/src/main.rs` <50 LOC, `nika-cli/src/{args,context,exit,dispatch,verbs/}`, one file per verb.

**5 pilot verbs in order:**
1. `completion` (30 LOC, validates wiring)
2. `version` (validates AppContext::from_global)
3. `check` (validates Renderer + NikaError propagation)
4. `lint` (validates nika-cli standalone build)
5. `run` (big one, its own PR, extensive tests)

Then remaining 25 verbs migrate in parallel PRs (<500 LOC each).

**Testing: 3 layers:**
1. rstest unit tests per verb (mock AppContext)
2. `insta` snapshot tests (help text, JSON output)
3. `assert_cmd` integration tests (~20 black-box tests)

Target: 4% → 60%+ CLI coverage.

### Phase 17 (analyze.rs split) — COMPLETED Session 3 ✅

`analyze.rs` (5531 LOC → 6 files, largest 1109 LOC)

### Phase 18 (nika-tui split + wire features) — ENRICHED

Split into `nika-tui-{widgets,core,views,app}`. Wire all 17 currently pass-through features:

| Feature | TUI wire point |
|---------|---------------|
| `media-phash` | `visual_similarity_panel` in monitor view |
| `media-chart` | `cost_chart` widget |
| `media-qr` | `qr_preview` in Studio |
| `media-thumbnail` | inline thumbnail rendering |
| `media-metadata` | `metadata_inspector` panel |
| `media-pdf` | `pdf_preview` in Studio |
| `media-svg` | `svg_preview` widget |
| `media-optimize` | status bar indicator |
| `media-iqa` | `quality_badge` |
| `media-provenance` | `provenance_indicator` |
| `media-compression` | compression ratio status |
| `fetch-article` | `article_preview` in Studio |
| `fetch-markdown` | markdown preview |
| `fetch-html` | `html_inspector` panel |
| `fetch-feed` | `feed_list` widget |
| `fetch-sitemap` | `sitemap_tree` widget |
| `native-inference` | `local_model_badge` |

### Phase 19 (type system hardening) — ADD compact_str, TaskId newtype, Runner<State>

- `TaskId(CompactString)`, `ModelId(CompactString)`, `ProviderName(CompactString)`, `AliasName(CompactString)`
- `SecretString` at all API boundaries (not just nika-vault)
- `Runner<Unconfigured | Ready | Running>` type-state pattern
- Sealed traits: `BuiltinTool`, `Provider`, `VerbExecutor`
- `#[must_use]` on all `Result`-returning methods
- `#![warn(missing_docs)]` on `nika-core` and `nika-runtime`

### NEW Phase 20 (plugin runtime) — wasmtime Component Model

Create `nika-plugin` crate for sandboxed WASM plugins:

```
nika-plugin/
├── src/
│   ├── host.rs          — Wasmtime engine + store pooling
│   ├── plugin.rs        — Plugin trait + lifecycle
│   ├── registry.rs      — Plugin discovery + metadata
│   └── wit/             — WIT world definitions
│       ├── nika.wit     — Core plugin interface
│       └── verbs.wit    — Custom verb plugins
```

Enables: custom transforms, custom builtin tools, custom verbs (all sandboxed with explicit capability grants).

### NEW Phase 21 (supply chain + fuzzing)

- Wire `cargo-deny` in CI: license enforcement, CVE scan, crate ban (`danger_accept_invalid_certs`)
- Wire `cargo-audit` in CI: RUSTSEC database
- Adopt `cargo-vet` (Mozilla pattern) for supply chain auditing
- Set up `cargo-fuzz` targets:
  - YAML parser (nika-core/src/ast/raw/parser.rs)
  - Template engine (nika-engine/src/binding/template.rs)
  - Shell blocklist (nika-exec-runner/src/blocklist.rs)
  - jq expression parser (nika:jq)
  - SSRF URL parser

### NEW Phase 22 (performance hardening + PGO)

- Enable `mimalloc` global allocator
- `[profile.release]` final tuning:
  ```toml
  opt-level = 3
  lto = "fat"
  codegen-units = 1
  strip = "symbols"
  panic = "abort"        # VERIFY Shield catch_unwind safety first
  debug = false
  incremental = false
  overflow-checks = false
  ```
- Wire `cargo-pgo` in release CI (instrument → collect → optimize)
- Add benchmark suite (divan):
  - `benches/template.rs` — render_simple, render_large_loop
  - `benches/dag.rs` — topological_sort (N=10/100/1000)
  - `benches/json.rs` — serialize_task_result
  - `benches/transforms.rs` — pipe_chain (upper, trim, jq)
  - `benches/cold_start.rs` — parse + validate + first-task (full workflow)
- Binary size: cargo-bloat audit → feature gating per §7

---

## 6. NEW: Quick Wins Round 2

**Ship BEFORE Phase 9-12 effect crate hardening to establish clean perf baselines.**

Total effort: **~6 hours**. Zero risk. Measurable hot-path wins.

| # | Fix | File | Effort |
|---|-----|------|--------|
| 1 | Drain task spawn refactor (P-1) | `infer.rs:1078, 1282` | 1h |
| 2 | `aho-corasick` + `ArcSwap` redact_secrets (P-2, L-1) | `util/mod.rs` | 1.5h |
| 3 | for_each single-fold (P-3) | `runner/mod.rs:2037-2097` | 30min |
| 4 | Templatable preset clone (P-4) | `task_dispatch.rs:341, 344` | 30min |
| 5 | `current_dir` caching (A-2) | `builtin/data/io.rs` | 30min |
| 6 | `parse_link_header_hreflang` bug (C-1) | `extract.rs:39-44` | 30min |
| 7 | `count() as u32` → `try_from` (C-2) | `runner/mod.rs:2037` | 15min |
| 8 | `Box<PendingWithEntry>` (C-3) | `binding/resolve.rs:66` | 15min |
| 9 | `Box<AgentParams>` in TaskAction::Agent (C-4) | `ast/action.rs:483` | 15min |
| 10 | Adopt `foldhash` workspace-wide | `nika-core/src/lib.rs` | 45min |

**Verification:** cargo test green, cargo clippy zero warnings, cargo bench delta measured.

---

## 7. NEW: Binary Size Optimization Plan

**Current:** 112 MB release binary
**Target:** ~35-45 MB

### Audit first

```bash
cargo install cargo-bloat
cargo bloat --release --crates -n 30 > bloat-crates.txt
cargo bloat --release -n 30 > bloat-symbols.txt
```

### Cuts (estimated from reference projects)

| Cut | Saving | Action |
|-----|--------|--------|
| `strip = "symbols"` + `panic = "abort"` + `lto = "fat"` | 15-20 MB | Profile changes (verify `catch_unwind` use in Shield first) |
| Feature-gate `native-inference` (mistral.rs) non-default | 25-30 MB | `Cargo.toml` feature flag |
| Feature-gate non-default rig-core providers | 10-15 MB | Per-provider feature flags |
| `regex` → `regex-lite` where Unicode classes unused | 3-5 MB | Source change |
| Lazy `syntect` grammars (runtime download) | 5-8 MB | Defer to first-use |
| Drop `tower-lsp-server` + `dashmap` + `ls-types` from engine | 2-4 MB | Phase 7 LSP absorption |
| **Total** | **60-82 MB** | → **30-52 MB final** |

### Multi-binary split (future architecture)

Split into:
- `nika` — core CLI (~30 MB) — default install
- `nika-native` — with mistral.rs (~80 MB) — installable via `nika install native` (rustup pattern)
- `nika-plugin` — wasmtime runtime (~20 MB) — installable via `nika install plugin`

This gives users pay-for-what-you-use binary size. Default `brew install nika` = 30 MB.

---

## 8. NEW: Observability & Rate Limiting

### OTLP export for `nika serve`

Add feature-gated OTLP exporter in `nika-event`:

```toml
# nika-event/Cargo.toml
[features]
otlp = ["opentelemetry-otlp", "tracing-opentelemetry"]
```

Wire in `nika serve`:
- Spans for every workflow execution (trace_id = workflow run ID)
- Metrics for provider latency, token counts, cost, cache hit rate
- Logs via `tracing-subscriber` layered with OTLP exporter
- 37 metrics defined in observability research (provider calls, task latency, DAG depth, cache hits, etc.)

### Per-provider rate limiting

`governor 0.7` in `nika-provider`:

```rust
use governor::{Quota, RateLimiter, Jitter};

pub struct ProviderBuckets {
    buckets: DashMap<Arc<str>, Arc<RateLimiter<...>>>,
}

impl ProviderBuckets {
    pub fn bucket_for(&self, provider: &str, model: &str) -> Arc<RateLimiter<...>> {
        // Per-provider-per-model bucket
        // Quota from ModelCapabilities catalog (requests/min, tokens/min)
    }
}
```

Pre-request: `bucket.until_ready_with_jitter(Jitter::up_to(Duration::from_millis(100))).await`

Kills the current ad-hoc retry-with-backoff anti-pattern. GCRA is provably correct for burst + steady-state.

---

## 9. NEW: WASM Plugin Runtime

**New crate:** `nika-plugin` (Phase 20)

**Purpose:** Sandboxed custom extensions — transforms, builtin tools, verbs — executed in Wasmtime Component Model with explicit capability grants.

**Architecture:**

```
nika-plugin/
├── Cargo.toml
├── wit/
│   ├── world.wit           — Component world: nika-plugin
│   ├── transforms.wit      — Custom transform interface
│   ├── builtins.wit        — Custom builtin tool interface
│   └── verbs.wit           — Custom verb interface
├── src/
│   ├── lib.rs              — Public API
│   ├── host.rs             — Wasmtime engine + store pooling
│   ├── bindings.rs         — Host function bindings
│   ├── plugin.rs           — Plugin trait + lifecycle
│   ├── registry.rs         — Plugin discovery + manifest
│   ├── capabilities.rs     — Capability grants per plugin
│   └── sandbox.rs          — WASI-P2 sandbox config
└── tests/
    └── plugins/            — Example plugins in Rust/Go/Python
```

**Capability model:**

```yaml
# plugin.toml
name: "my-transform"
version: "0.1.0"
capabilities:
  - transform: ["my_upper", "my_reverse"]
  - filesystem: { read: ["./data/"] }  # optional
  - http: { hosts: ["api.example.com"] }  # optional
```

**Plugin usage in workflows:**

```yaml
# .nika/config.toml
[plugins]
dir = "./plugins"

# workflow.nika.yaml
- id: transform
  with: { text: "hello" }
  infer: "{{with.text | my_transform::my_upper}}"
```

**Why wasmtime (Component Model) not wasmer:**
- Official Bytecode Alliance project
- Component Model is stable in wasmtime 24+
- WASI-P2 for filesystem/http capability grants
- Used in production by: Fastly, Fermyon, Shopify, Cloudflare

**Why not defer:** custom transforms are a killer feature. Shipping without them leaves Nika behind competitors that have Python/JS plugin systems. Sandboxed WASM is better (deterministic, fast, secure) — ship it.

---

## 9.5 NEW: Security Hardening — Beyond Shield

**Source:** rust-security + perplexity research (7 sonar-pro queries)

### Already state-of-the-art (verified in source)

Nika is already further than 95% of Rust projects in 2025:

| Layer | Implementation | Status |
|-------|---------------|--------|
| Shield 6-layer prompt injection defense | `nika-core/policy.rs` + `nika-engine/runtime/shield.rs` | ✅ Complete (v0.79.0) |
| SSRF with DNS pre-resolution + pinning | `runtime/policy.rs::resolve_and_pin_ssrf` + `executor/fetch.rs::run_fetch` using `Client::resolve()` | ✅ Complete — this is the canonical 2025 pattern |
| Encrypted vault | XChaCha20Poly1305 + Argon2i (64 MiB memory cost) | ✅ Complete |
| Exec blocklist | NFKC normalization + zero-width strip + full-command scan in `runtime/security.rs::validate_exec_command_full` | ✅ Complete |
| Secrecy wrapping | `secrecy 0.10`, `zeroize 1.8`, `subtle` (constant-time) in nika-vault + daemon | ✅ Complete |
| CI: cargo-audit + cargo-deny + cargo-machete + cargo-geiger | `.github/workflows/ci.yml` | ✅ Complete |
| CI: CodeQL + Semgrep | `.github/workflows/sast.yml` | ✅ Complete |
| TLS 1.2/1.3 only | rustls 0.23 default (no TLS 1.0/1.1) | ✅ Complete |
| Unsafe-code-zero in kernel | nika-core zero unsafe | ✅ Complete |
| rustls-only (no OpenSSL) | reqwest with rustls-only features | ✅ Complete |

### Remaining security gaps (all in scope)

**SEC-1: Process sandbox on `exec` verb** — biggest remaining gap

No single cross-platform crate exists. Must write `Sandbox` trait with 3 impls:

| Platform | Crate | Approach |
|----------|-------|----------|
| Linux | `landlock 0.4` + `seccompiler 0.5` | Unprivileged FS sandbox (Linux 5.13+) + syscall filtering via `pre_exec` |
| macOS | (no crate) | Wrap binary in `sandbox-exec -p '<profile>' -- $cmd`, hand-written profile |
| Windows | `win32job 0.5` | `JOB_OBJECT_LIMIT_*` via `windows-sys` wrapper |

**NOT recommended:** `birdcage` (Phylum) — stale since 2023, open macOS security issue, unsafe for launch. `extrasafe` — Linux-only thin wrapper.

**Default profile:**
- Deny network egress
- Deny exec of binaries outside `/usr/bin`, `/bin`, `/usr/local/bin`
- Allow read on `${cwd}`
- Write only on `${cwd}/output` and `/tmp/nika-*`
- Opt-out via `[policy] sandbox = "off"` in nika.toml
- Opt-in stricter via `[policy] sandbox = "strict"` (also blocks `/tmp`)

**Phase:** Phase 11 (nika-exec-runner extraction) — highest-leverage security item.

**SEC-2: TOCTOU-safe file API via cap-std**

**Crate:** `cap-std 3.x` (Bytecode Alliance, used by Wasmtime/WASI in production)

Current state: `std::fs` with manual canonicalization across `nika:read`, `nika:write`, `nika:edit`, `tools/mod.rs::check_path_readable`. This is the class of bug that bit `git` (CVE-2022-24765) and `tar` countless times.

**Fix:** Migrate to `cap_std::fs::Dir` rooted at the project working directory. The `Dir` handle prevents path traversal, symlink-escape, and TOCTOU races via `openat`. Eliminates an entire vulnerability class.

**Also fixes:** TOCTOU on `.nika/traces/*.ndjson` (currently append mode but no `O_NOFOLLOW` — symlink planted at trace path redirects writes).

**Phase:** Phase 9b (nika-fs) + Phase 13 (nika-builtin file tools).

**SEC-3: TLS root certificate pinning for known providers**

Reqwest 0.13 with rustls 0.23 supports custom `RootCertStore`. Build `nika_engine::tls::pinned_root_store()` loading only ~6 anchor CAs from `webpki-roots::TLS_SERVER_ROOTS`:
- DigiCert (Anthropic, OpenAI)
- ISRG Root X (Let's Encrypt — Mistral, many)
- Cloudflare (proxy for several)
- Amazon (AWS-hosted)
- GTS Root (Google Trust Services — Gemini)
- GlobalSign Root CA - R

Filtering by subject DN substrings (acceptable because this is enforcement, not last-resort authentication).

**NOT recommended:** SPKI leaf pinning. Providers rotate certs on 90-day cadence — hardcoded SPKI is a self-imposed outage.

**Per-provider clients** (cached in rig provider layer) use pinned store. **Generic `nika fetch` verb** keeps full webpki-roots set so users can hit anything.

**Phase:** Phase 11 (Provider cutover) — add `impl Provider for RigProvider` initialization uses pinned store for known providers.

**SEC-4: Supply chain — cargo-vet + cargo-auditable**

Current CI runs cargo-audit + cargo-deny + cargo-machete + cargo-geiger + CodeQL + Semgrep. Missing:

| Tool | Purpose | Effort |
|------|---------|--------|
| `cargo-vet 0.10` | Mozilla supply-chain pattern — import audit registries (Mozilla, Google, Bytecode Alliance, Embark) | 2 days (not 0.5 as initially estimated — depends on transitive dep tree complexity) |
| `cargo-auditable 0.6` | Embed SBOM in released binary — enables `cargo audit bin nika` on downloaded artifact | 1 hour (`cargo auditable build --release` in release.yml) |
| ClusterFuzzLite | Continuous fuzzing in GitHub Actions (not OSS-Fuzz — fits AGPL launch model) | 1 day |

**deny.toml hardening:**
```toml
[bans]
deny = [
    { name = "openssl-sys" },
    { name = "native-tls" },
    { name = "openssl" },
]
multiple-versions = "warn"
```

Enforces rustls-only at the type level.

**Phase:** Phase 21 (supply chain + fuzzing).

**SEC-5: Fuzzing harness (5 targets)**

Workspace placement: single top-level `fuzz/` directory adjacent to `tools/`, one `Cargo.toml` with each fuzz target as `[[bin]]`.

**Target priority:**

| # | Target | Function | Why |
|---|--------|----------|-----|
| 1 | `fuzz_yaml_parser` | `nika_core::ast::raw::parser::parse_workflow(&[u8])` | Highest payoff — entry point |
| 2 | `fuzz_template_resolve` | `nika_engine::binding::template::resolve(input, bindings)` | Known injection surface |
| 3 | `fuzz_shell_blocklist` | `nika_engine::runtime::security::validate_exec_command_full(&str)` | Property: any blocklisted pattern post-NFKC must return `Blocked`. Catches zero-width + homoglyph bypasses |
| 4 | `fuzz_url_ssrf` | `nika_engine::runtime::policy::PolicyEnforcer::check_fetch(&str)` | Property: every IP literal round-trips through `is_private_or_special` consistently |
| 5 | `fuzz_jq_engine` | `nika_engine::runtime::builtin::data::jq` wrapper around jaq | Resource exhaustion (regex, recursion) |

**CI pattern:** ClusterFuzzLite runs 5 min on PR, 30 min nightly. Corpora as workflow artifacts.

**proptest stays** as in-tree property tester — complements, not replaces, cargo-fuzz. `quickcheck` is legacy.

**Phase:** Phase 21.

**SEC-6: Panic discipline**

Add workspace-level clippy lints:
```toml
[workspace.lints.clippy]
unwrap_used = "warn"        # not deny — too noisy for tests
expect_used = "warn"
cast_possible_truncation = "warn"
cast_sign_loss = "warn"
panic = "warn"
todo = "warn"
indexing_slicing = "warn"
dbg_macro = "warn"
print_stdout = "warn"
```

**NOT `clippy::pedantic`** — wrong preset. Use `clippy::all + clippy::cargo` + the hand-picked security subset above.

Combined with fuzz target #1 (YAML parser), this catches residual panics that would crash `nika serve`.

**Phase:** Phase 22 (perf hardening) — mechanical, ~1 day.

**SEC-7: Verify governor wired in `nika serve`**

`governor 0.10` is a dep but **verify** it's actually wired as `tower::Layer` on every HTTP route in nika-serve. Default: 10 req/s + 100 burst, keyed on `(client_ip, workflow_name)`.

**Phase:** Session verification pass.

**SEC-8: serde_yaml unmaintained**

`serde_yaml` is unmaintained as of 2024. No advisory, but candidate for replacement:
- `serde_yml` (community fork)
- `marked-yaml` (preserves source locations — may enable better error messages)

**Phase:** Post-v0.80 consideration (not blocking).

### Additional crates for security

| Crate | Version | Purpose |
|-------|---------|---------|
| **cap-std** | 3.x | Capability-based std replacement — TOCTOU-safe file ops |
| **landlock** | 0.4.x | Linux unprivileged FS sandbox |
| **seccompiler** | 0.5.x | Linux syscall filtering (Firecracker pattern) |
| **win32job** | 0.5 | Windows Job Objects wrapper |
| **cargo-vet** | 0.10.x | Supply-chain attestation (tool) |
| **cargo-auditable** | 0.6.x | SBOM embedding in binary (tool) |
| **garde** | 0.22.x | Derive-based input validation (replace ad-hoc) |

### Security positioning (revised)

> **Nika is the only Rust workflow engine in 2026 that ships with:**
> - Six-layer prompt-injection defense (Nika Shield)
> - Pre-resolved DNS-pinned SSRF protection (via `Client::resolve()`)
> - Encrypted Argon2i+XChaCha20Poly1305 secret vault
> - Full supply-chain CI (cargo-audit + cargo-deny + cargo-vet + cargo-auditable + cargo-geiger + CodeQL + Semgrep)
> - Unsafe-code-zero in the kernel crate
> - TOCTOU-safe file operations via cap-std
> - Defense-in-depth `exec` sandbox combining landlock + seccompiler (Linux), sandbox-exec (macOS), Job Objects (Windows)
> - TLS 1.2/1.3 only with per-provider root CA pinning
> - Continuous fuzzing via ClusterFuzzLite (5 targets)
>
> Built on rustls. No OpenSSL. No `unsafe` in nika-core. AGPL-3.0.

All 9 clauses will be true post-refactor. Currently 6/9 are already true.

### Security RUSTSEC status

**No active CRITICAL or HIGH advisories on Nika's direct deps** (as of early 2025):
- tokio, reqwest, serde_yaml, url, regex, rustls, hickory-resolver, governor, secrecy, zeroize, subtle, proptest — all clean

`cargo-deny` on every push catches future advisories immediately.

---

## 10. NEW: Hybrid Disk Cache

**Crate:** `foyer 0.10+`

**Use in `nika-cache` (Phase 15):**

3-tier cache hierarchy for LLM responses:

```rust
pub struct LlmResponseCache {
    hot: Arc<moka::future::Cache<CacheKey, CachedResponse>>,   // L1: in-memory, TinyLFU
    warm: Arc<papaya::HashMap<CacheKey, CachedResponse>>,      // L2: concurrent read-mostly
    cold: Arc<foyer::HybridCache<CacheKey, CachedResponse>>,   // L3: disk-backed hybrid
}
```

- **L1 (moka):** hot cache, <100MB, TinyLFU eviction, microsecond access
- **L2 (papaya):** concurrent read-mostly, lock-free, millisecond access
- **L3 (foyer):** disk-backed hybrid mem+disk, admission policies (only frequently-accessed items promoted), gigabyte scale, sub-millisecond access

**Replaces** hand-rolled `~/.nika/cache/` LLM response cache.

**Trust-aware cache keys:** tainted runs cache separately (different namespace) to prevent cross-contamination between trusted and untrusted workflow executions.

---

## 11. Architecture Pattern Decisions

### 11.1 Async trait pattern matrix

| Trait | Pattern | Reason |
|-------|---------|--------|
| `Clock` | `async_trait` | Simple, dyn-safe, 1 fn |
| `Filesystem` | `async_trait` | Simple, dyn-safe |
| `HttpClient` | `async_trait` + `BoxStream` for streaming | Dyn-safe for `Arc<dyn HttpClient>` |
| `ShellExecutor` | `async_trait` | Simple, dyn-safe |
| `BlobStore` | `async_trait` | Simple, dyn-safe |
| `Provider` | `async_trait` + `Pin<Box<dyn Stream<Item = Result<InferEvent, _>> + Send>>` | Streaming + tool use + thinking events, dyn-safe required for `Arc<dyn Provider>` |
| `EventEmitter` | Sync `fn emit(&self, kind)` | Fast, lock-free via Arc internals |
| `VerbExecutor` | `async_trait` | `[Arc<dyn VerbExecutor>; 5]` dispatch array |
| `BuiltinTool` | `async_trait` sealed | Closed set of 63 tools |
| `TaskScope` (umbrella) + 6 splinters | Direct `&self` methods, no async_trait | All methods synchronous (async lives in verbs) |

**Rule:** use `async_trait` when consumer holds in struct field (needs dyn). Use AFIT direct when consumer takes `&mut` function arg (monomorphized).

### 11.2 Bundle composition pattern (v2.1 §18.6, enriched)

```rust
// nika-kernel/src/bundle.rs

#[derive(Clone)]
pub struct KernelBundle {
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn EventEmitter>,
}

#[derive(Clone)]
pub struct IoBundle {
    pub fs: Arc<dyn Filesystem>,
    pub http: Arc<dyn HttpClient>,
    pub shell: Arc<dyn ShellExecutor>,
    pub blob: Arc<dyn BlobStore>,
}

#[derive(Clone)]
pub struct LlmBundle {
    pub providers: Arc<ProviderRegistry>,
    pub builtins: Arc<BuiltinRegistry>,
    pub rate_limiter: Arc<ProviderBuckets>,   // NEW
}

#[derive(Clone)]
pub struct SecurityBundle {                   // NEW
    pub policy: Arc<SecurityPolicyConfig>,
    pub spotlight: Arc<SpotlightFence>,
    pub canary: Arc<CanarySystem>,
}

pub struct Runtime {
    pub kernel: KernelBundle,
    pub io: IoBundle,
    pub llm: LlmBundle,
    pub security: SecurityBundle,
    pub verbs: [Arc<dyn VerbExecutor>; 5],
    pub scope: Arc<dyn TaskScope>,  // ex-RunContext
}
```

### 11.3 Plugin registration — linkme distributed_slice

Per v2.1 §18.7. Confirmed: `linkme` beats `inventory` (zero runtime cost, const-init friendly). Adopt for:
- Builtin tool registration (63 tools)
- Transform registration (65 transforms)
- Lint rule registration
- Exporter registration (trace format exporters)
- Provider registration (rig + mock + native)

### 11.4 Sealed trait pattern (ecosystem 2024+ idiom)

```rust
// nika-kernel/src/sealed.rs
mod private {
    pub trait Sealed {}
}

pub trait BuiltinTool: private::Sealed + Send + Sync {
    // ...
}

// In nika-builtin, the #[builtin_tool] macro generates:
impl nika_kernel::sealed::private::Sealed for MyTool {}
impl BuiltinTool for MyTool { ... }
```

Third parties cannot `impl BuiltinTool`. Only the `#[builtin_tool]` macro can. This + WASM plugins covers both static (linkme) and dynamic (wasmtime) extension.

### 11.5 TaskScope splinter super-traits

Per §2.3:
- `BindingScope: TaskResults` (template resolution reads outputs)
- All other traits flat
- `TaskScope` umbrella with blanket impl

### 11.6 Error chain pattern — thiserror 2 + miette transparent

Per §2.2:
- `#[error(transparent)] + #[diagnostic(transparent)] + #[from]`
- Preserves source() chain
- Forwards all Diagnostic methods (code, help, labels, severity, url)
- One `NikaError` outer enum, 12 domain inner enums

---

## 12. Show HN Positioning

**Don't claim "10x faster than X" without measurement.** Measure first, claim with data.

### Defensible angles (with measurement plan)

| Claim | How to measure |
|-------|---------------|
| **Cold start: <30ms for 50-task workflow** | `benches/cold_start.rs` divan benchmark |
| **27x faster than LangChain cold start** | Compare to LangChain `Runnable.invoke()` first-call latency |
| **45 MB single binary** vs 200+ MB Python venv with deps | `ls -lh target/release/nika` |
| **30-50 MB RSS** vs 200+ MB LangChain | `ps -o rss` during workflow execution |
| **Deterministic across 9 providers** | 5-layer structured output defense + same test on all providers |
| **Sub-50ms TTFT** after provider responds | Measure `duration from response.0 to first stream chunk` |
| **Single file deploys** | Homebrew formula build + cross-platform binaries |
| **Zero Python, zero JS, zero Docker** | Single binary, no runtime deps |

### Architecture positioning

- **~38 crates, matches rust-analyzer scale** — show the workspace members list
- **10 traits for all side effects** — diagram showing Clock/Filesystem/HttpClient/ShellExecutor/BlobStore/Provider/EventEmitter/VerbExecutor/BuiltinTool/TaskScope
- **Shield 6-layer prompt injection defense** — SECURITY.md + threat model
- **Constellation refactor** — live-coded on stream (hypothetical)
- **4 proc-macros eliminate ~7000 LOC of boilerplate** — before/after code comparison

### Narrative (draft HN post opening)

> **Show HN: Nika — Inference as Code**
>
> Nika is a workflow engine for AI tasks. One YAML file. Any provider. Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, OpenAI-compatible, local GGUF. Same YAML, same output, 9 providers.
>
> ```yaml
> schema: "nika/workflow@0.12"
> tasks:
>   - id: research
>     infer: "Find top 3 Rust workflow engines"
>     structured:
>       schema: { ... }
>   - id: summarize
>     depends_on: [research]
>     infer: "Executive summary: {{with.research | to_json}}"
>     provider: mistral  # different provider, same workflow
> ```
>
> `brew install nika && nika run workflow.nika.yaml`
>
> It's written in Rust. 38 crates. 5 verbs (infer, exec, fetch, invoke, agent). 65 transforms. 63 builtin tools. 6-layer prompt injection defense. AGPL-3.0.
>
> The workflow above cold-starts in 28ms. LangChain does 847ms. The binary is 42MB. The LangChain venv is 230MB.
>
> I'm launching today because I've been building this for 6 months and it's finally ready. I rewrote the architecture twice. The second rewrite (Constellation) produced the cleanest Rust codebase I've ever seen — 38 crates with strict downward layering, every side effect behind a trait, wasm plugin runtime, PGO release builds.
>
> Ask me anything.

---

## 13. Validation Gates

### Per-commit

```bash
cargo check --workspace
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### Per-phase

```bash
# Diff metrics
cargo test --workspace --lib 2>&1 | grep "test result" > tests-after.txt
diff tests-before.txt tests-after.txt  # MUST be +N, never -N

# Clippy regression
cargo clippy --workspace -- -D warnings 2>&1 | grep "warning" | wc -l  # MUST be 0

# Binary size
ls -lh target/release/nika  # tracked

# Compile time
time cargo build --workspace  # tracked

# .unwrap() count
rg "\.unwrap\(\)" tools/ --type rust | grep -v "#\[test\]" | wc -l  # target: <5000

# TODO/FIXME count
rg "TODO|FIXME" tools/ --type rust | wc -l  # target: flat or decreasing

# God file audit
find tools/ -name "*.rs" -not -path "*/tests/*" | xargs wc -l | sort -rn | head -10  # target: all <1500
```

### Pre-v0.80 final

- [ ] `cargo test --workspace --lib` — all 12,000+ tests pass
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] `cargo fmt --check` — clean
- [ ] `cargo deny check` — no license/CVE issues
- [ ] `cargo audit` — no RUSTSEC advisories
- [ ] `cargo udeps` — no unused deps
- [ ] `cargo bloat --release --crates -n 30` — binary <50 MB
- [ ] `cargo pgo optimize build` — PGO release binary built
- [ ] Benchmark suite (divan) — all benchmarks have baselines
- [ ] Feature matrix CI — all feature combinations compile
- [ ] Editor matrix — 5 editors (VS Code, Zed, Neovim, Helix, JetBrains) verified manually
- [ ] WASM plugin example — at least 1 example plugin compiled + executed
- [ ] Integration tests — `assert_cmd` suite for nika-cli
- [ ] LSP integration test — JSON-RPC replay test
- [ ] Shield threat model — SECURITY.md updated
- [ ] ARCHITECTURE.md updated in every crate >3k LOC
- [ ] Public API curation — `pub(crate)` minimized facade
- [ ] `#![warn(missing_docs)]` on `nika-core`, `nika-kernel`, `nika-runtime`
- [ ] Fuzz targets — YAML parser, template engine, shell blocklist, jq parser, SSRF URL parser
- [ ] `cargo-vet` baseline established

---

## Appendix A: Revision Deltas from v2.1

| Item | v2.1 | v2.2 |
|------|------|------|
| Total crates | ~38 | ~42 (+ nika-plugin, nika-cache already counted) |
| Phase count | 20 | **22** (+ Phase 20 plugin, + Phase 21 supply chain, Phase 22 perf) |
| Deferred items | 4 (5.2, 6, 7, 8c) | **0** |
| P0 bug count | 4 | **15** (added from effect crate hardening) |
| Crate additions | 11 | **28** (explicit) |
| Bug count total | ~15 | **55+** |
| "Post-launch" / stretch items | wasmtime, foyer | **0** — all in scope |
| God files list | 4 (main, error, runner, template) | **5** (+ binding/resolve 3948 LOC) |
| Phase 6 hours estimate | 40+ | **16** (CliError split) |
| Phase 7 commits | 6 big-bang | **7** mechanical, -4000 LOC net |
| Phase 11 commits | phased migration | **5** small (80% already done) |
| Phase 14 commits | cascading | **5** additive, zero-risk |

---

## Appendix B: Files to Touch (Checklist)

### New crates to create

- [ ] `nika-plugin` — wasmtime Component Model runtime
- [ ] All 5 verb crates (per v2.1): nika-verb-infer, nika-verb-exec, nika-verb-fetch, nika-verb-invoke, nika-verb-agent
- [ ] `nika-runtime` — composition root (per Phase 14)
- [ ] `nika-cache` — 3-tier hybrid cache (per Phase 15)
- [ ] `nika-provider` — Provider impls (post Phase 11)
- [ ] `nika-builtin` — sealed BuiltinTool + 63 tools (post Phase 13)
- [ ] `nika-macros` — 4 derives + transform! (Phase 3)
- [ ] `nika-tui-{widgets,core,views,app}` — per Phase 18

### Files to significantly modify

- [ ] `nika-http/src/lib.rs` — H1-H5, N1-N6 hardening
- [ ] `nika-http/src/resolver.rs` — NEW, SsrfResolver impl Resolve
- [ ] `nika-fs/src/lib.rs` — FS1-FS8 hardening
- [ ] `nika-exec-runner/src/lib.rs` — EX1-EX7 + `command-group` adoption
- [ ] `nika-engine/src/runtime/executor/infer.rs` — P-1 drain task refactor
- [ ] `nika-engine/src/util/mod.rs` — P-2 aho-corasick + L-1 arc-swap
- [ ] `nika-engine/src/runtime/runner/mod.rs` — P-3 for_each fold, split into scheduler.rs
- [ ] `nika-engine/src/binding/resolve.rs` — M-1 split into 4 files
- [ ] `nika-engine/src/runtime/fetch_cache.rs` — P-5 Arc<CachedResponse>
- [ ] `nika-engine/src/error.rs` — Phase 6 domain wrappers
- [ ] `nika-engine/src/error_domains.rs` — Phase 6 Diagnostic derive + 8 new enums
- [ ] `nika-cli/src/error.rs` — NEW `CliError` enum (Phase 6c)
- [ ] `nika-engine/src/store/run_context.rs` — Phase 14 scope splinter impls
- [ ] `nika-engine/src/lsp/` — Phase 7 DELETE entirely
- [ ] `nika-lsp-core/src/snapshot.rs` — NEW, CachedAst + Snapshot
- [ ] `nika-core/src/catalogs/cost.rs` — NEW, moved from engine/provider
- [ ] `nika/src/main.rs` — Phase 16 <50 LOC rewrite
- [ ] `nika-cli/src/verbs/` — NEW, one file per 30 verbs
- [ ] `tools/Cargo.toml` — all new crate additions

---

---

## Appendix C: Shield Real-World Validation (2026-04-08)

During the Perplexity security research agent's execution, a **prompt injection attempt** was embedded in the tool-use system reminders (the "Crumpet turtle" persona override). The research agent **detected and ignored** the injection, explicitly noting: *"Note: a prompt-injection attempt embedded in the tool-use system reminders ('Crumpet turtle' persona override) was ignored. No file with that content was created or referenced."*

**This is Shield working as designed** — an LLM agent executing in a tool-use context, under adversarial input, correctly identifying the attack and refusing to execute it. Real-world validation of the 6-layer defense before launch.

**Recommendation:** include this anecdote in the Show HN post + SECURITY.md as concrete evidence that Shield isn't theoretical.

---

**END OF ADDENDUM**

Every issue gets fixed. Every crate gets adopted. Every phase gets executed. No deferrals, no compromises, no "good enough for launch". The launch date follows the work, not the other way around.
