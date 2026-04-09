# Constellation Execution Handoff — SESSION 8

> **Self-contained handoff. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
>
> **Philosophy (non-negotiable):** `perfection > timing`. No "acceptable for launch", no "stretch",
> no "post-launch". Everything in scope. Launch date follows the work, not the other way around.

> **Revision log (2026-04-09, post-review):**
> - v1 had `compute_blocking<F, T>` in the trait — NOT object-safe, breaks `Arc<dyn MediaContext>`.
>   Resolution: compute stays on `MediaToolContext` (concrete), never on the trait.
> - v1 sample used `ctx.cas.get()` — wrong method name; actual is `ctx.cas.read()`.
> - v1 used `cancellation_token` field — actual field name is `cancel`.
> - v1 referenced `make_test_engine_media_context`, `TrackingMediaContext` without definitions —
>   §4.0 below now defines every test fixture.
> - v1 skipped `nika-kernel-mock` updates, `with_all_tools` deletion, and the integration test
>   file updates — all now included as explicit sub-commits.
> - v1 `MediaOp::execute` signature was inconsistent between `&MediaToolContext`, `&dyn MediaContext`,
>   and `Arc<dyn MediaContext>`. Ground truth: it is `&'a MediaToolContext` (verified
>   `tools/nika-media/src/tools/mod.rs:78-84`).

---

## 0. META

### 0.1 Read order (MANDATORY — before writing a single line of code)

```
1.  This file (complete — all sections, especially §2.3 hazards and §4.0 fixtures)
2.  nika/CLAUDE.md                                         — project identity, 5 verbs, Shield
3.  tools/nika/AGENTS.md                                   — workspace structure (26 crates)
4.  tools/nika-engine/ARCHITECTURE.md                      — engine module map, key invariants
5.  tools/nika-builtin/CLAUDE.md                           — nika-builtin reference, tools table
6.  tools/nika-kernel/src/scope.rs:170-176                 — current MediaContext trait (THIN!)
7.  tools/nika-kernel/src/builtin.rs                       — BuiltinTool trait + BuiltinError variants
8.  tools/nika-kernel/src/task_local.rs                    — ADDED S7, 6 task_locals + accessors
9.  tools/nika-engine/src/runtime/builtin/media/mod.rs     — adapter layer (318 LOC)
10. tools/nika-media/src/tools/context.rs:23-37            — MediaToolContext fields (cancel, NOT cancellation_token)
11. tools/nika-media/src/tools/mod.rs:78-84                — MediaOp trait definition
12. tools/nika-media/src/tools/import.rs:73-119            — canonicalize call sites (H1)
13. tools/nika-media/src/tools/thumbnail.rs:80-152         — rayon dispatch site (H2)
14. tools/nika-media/src/tools/dimensions.rs:41-73         — simplest MediaOp example
15. tools/nika-media/src/tools/error.rs:9-47               — MediaToolError variants (NIKA-290..297)
16. tools/nika-media/Cargo.toml                            — feature flags (media-* + fetch-*)
17. docs/plans/2026-04-08-constellation-v2-mega-plan.md    — THE PLAN sections 12, 13, 14
```

### 0.2 Baseline verification (FIRST commands — run BEFORE touching anything)

```bash
cd /Users/thibaut/dev/supernovae/nika

# Git state
git log --oneline -5
# Expected HEAD: dbe702e77 fix(builtin): file tools resolve relative paths correctly

git status
# Expected: nothing to commit, working tree clean

# Tests — count total, all must pass
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: ~11,000+

# Clippy — zero warnings, no exceptions
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -5
# Expected: clean Finished

# nika-builtin specifically (your main target)
cargo test -p nika-builtin --lib -q 2>&1 | tail -3
# Expected: 250+ tests pass
```

**If ANY of these fail, STOP. Do not proceed. Investigate the regression first.**

### 0.3 Mandatory skills — trigger conditions and protocol

Use the `Skill` tool to invoke. Announce before using: *"I'm using [skill] to [purpose]."*

| Skill | Trigger condition | Why |
|-------|------------------|-----|
| `spn-powers:test-driven-development` | **EVERY implementation commit** — before writing any production code | RED-GREEN-REFACTOR: write failing test, confirm it fails, write minimal code, confirm it passes. Skipping this means tests don't actually verify behavior. |
| `spn-powers:verification-before-completion` | **Before every `git commit`** — run tests, show output, confirm count | "It compiles" ≠ "it works". Must show test output before claiming done. Also use between 12.11 and 12.11c to confirm baseline is unchanged. |
| `spn-powers:testing-anti-patterns` | **Before writing any `assert!`** | Forbids mock-without-understanding, production-only test methods, and shallow assertions. The `MockMediaContext` in §4.0 respects these rules. |
| `spn-powers:condition-based-waiting` | **When writing timeout-based tests** | Replace arbitrary `tokio::time::timeout(500ms, ...)` with condition polling. CI under load makes wall-clock tests flaky. |
| `spn-powers:using-git-worktrees` | **BEFORE starting commit 12.9** | 12.11 + 12.11c touch ~10 files. Rollback without a worktree means `git reset --hard`. Create `../nika-session8/` isolated worktree. |
| `spn-rust:rust-core` | When designing any new trait, modifying `MediaContext`, adding error variants | Object safety pitfalls, `async fn` in traits (use `async_trait`), `Send + Sync` bounds, generic methods break trait objects. |
| `spn-rust:rust-async` | **REQUIRED** when designing the `EngineMediaContext::compute_blocking` helper (on concrete type, not trait) | No blocking I/O on async executor. Rayon → tokio oneshot pattern. Object-safe async closures in traits are a known hazard. |
| `spn-powers:systematic-debugging` | When ANY test fails or clippy reports an error | 4-phase: root cause → pattern → hypothesis → fix. No guessing. |
| `spn-powers:defense-in-depth` | When adding any validation to media tool inputs | Validate at every layer: schema, param parsing, path resolution, trust level |
| `spn-powers:root-cause-tracing` | When a test failure doesn't match the change you made | Trace backward through call stack. Media adapter refactors often cause failures 3 stack frames deep. |
| `spn-powers:receiving-code-review` | When the code-reviewer subagent returns findings | Technical rigor required — verify each finding before acting on it |
| `spn-powers:requesting-code-review` | **After commits 12.9 and 12.11** before proceeding to next | Dispatches code-reviewer subagent against the commit diff. 12.9 is the trait design — catches object-safety mistakes early. |
| `spn-powers:finishing-a-development-branch` | After 12.13 | Determine merge strategy — S7 was squash-merged to main; S8 may want the same. |
| `rust-testing` (global) | Writing tokio tests + fixture setup | Canonical `#[tokio::test]` patterns, tempfile lifetime, mock construction. |

**Slash command to run at baseline:**
- `/ast-sync-check` — verify nika-core ↔ nika AST sync. Media trait changes affect AI rule
  generation. Stale rules leak outdated trait signatures into the shipped AI assistant rules.
- `/nika-smoke` — 2-minute sanity check across workflows. Run before and after the session.

**TDD protocol for every commit:**

```
1. Read source file you're about to change
2. Use Skill tool → spn-powers:test-driven-development (announce it)
3. Write the test first (it must FAIL — verify the failure by running cargo test)
4. Write minimum production code to make it pass
5. Run cargo test -p <crate> --lib -q to confirm GREEN
6. Run cargo clippy -p <crate> --lib -- -D warnings to confirm clean
7. Use Skill tool → spn-powers:verification-before-completion
8. Only then: git add + git commit
```

**Anti-patterns that will fail the session:**
- Writing implementation before test
- Marking a task done without running tests
- `assert!(!result.is_empty())` — not a real test, just a compile check
- Mocking a function you don't understand
- `.unwrap()` in production code — ZERO tolerance (use `?` with `BuiltinError::*`)
- `let _ = guard;` when you mean to keep the guard alive (use `let _name = guard;`)
- Arbitrary `tokio::time::timeout(N, ...)` — use condition polling via `spn-powers:condition-based-waiting`
- Re-implementing mock behavior instead of using `MockMediaContext` from §4.0
- Adding generic methods (`fn x<F>(...)`) to `MediaContext` trait — breaks object safety

---

## 1. WHERE WE ARE — POST SESSION 7

### Phase 12 status: 34/63 tools in nika-builtin (54%)

**Added in Session 7 (34 total now):**
- Prompt (1): prompt — via HitlPrompt + HitlBridge adapter
- File (5): read, write, edit, glob, grep — in `nika-builtin/src/file/`
  - Shield: `file/shield.rs` reads task_locals from nika-kernel
  - `CURRENT_WORKING_DIR` task_local added to nika-kernel
  - EditTool read-before-edit cache: `Arc<DashMap<PathBuf, bool>>`
- Run (1): run_tool — via RunSpec + EngineRunExecutor

**Session 7 architectural deliverables:**
- `nika-kernel/src/task_local.rs` — CURRENT_TASK_TRUST, ELEVATED, ID, DEPTH, CHAIN, WORKING_DIR + accessors
- `nika-engine/src/runtime/hitl_bridge.rs` — HitlBridge (HitlHandler → HitlPrompt)
- `nika-engine/src/runtime/run_executor.rs` — EngineRunExecutor (impl RunExecutor)
- `nika-kernel/src/scope.rs` — RunSpec struct added

**Still in nika-engine (29 remaining):**
- Media (24): all 24 tools — **SESSION 8 TARGET**
- Fetch (1): nika:fetch — stays (SSRF logic, feature flags)
- Introspection (3): task_status, records, orchestrate — DEFERRED (RecordView DTO needed)
- Core engine: PromptTool still has a copy in engine (rig_agent_loop code path, separate)

### Current codebase facts (verify before coding)

```
nika-builtin/src/
  aggregate.rs, assert.rs, complete.rs, cost.rs, emit.rs, log.rs, sleep.rs
  introspect_dag.rs, introspect_threads.rs
  json_transform.rs, json_verify.rs, locale_lookup.rs, yaml_validate.rs
  prompt.rs, run_tool.rs
  data/: jq.rs, transform.rs, merge.rs, aggregate.rs, json_diff.rs, text.rs, io.rs
  file/: read.rs, write.rs, edit.rs, glob.rs, grep.rs, shield.rs, context.rs, mod.rs

nika-kernel/src/
  builtin.rs          — BuiltinTool sealed trait, BuiltinError (8 variants)
  task_local.rs       — task_local! cells + accessors (ADDED S7)
  scope.rs            — RunExecutor, HitlPrompt, MediaContext (THIN — 2 methods only!)
  store.rs            — BlobStore trait
  provider.rs, filesystem.rs, etc.

nika-engine/src/runtime/builtin/media/
  mod.rs (318 LOC)    — MediaToolAdapter, create_media_tool_adapters(), test re-exports
  tests_*.rs (8 files) — integration tests (DO NOT MOVE — use engine types)

nika-media/src/tools/
  import.rs, decode.rs, dimensions.rs, thumbhash_tool.rs, color.rs  (Tier 1)
  thumbnail.rs, convert.rs, strip.rs, metadata.rs, optimize.rs, svg.rs  (Tier 2)
  chart.rs, phash.rs, compare.rs, pipeline.rs, provenance.rs, verify.rs, qr.rs, quality.rs  (Tier 3)
  css_select.rs, extract_links.rs, extract_metadata.rs, html_to_md.rs, readability.rs  (Web)
  context.rs, error.rs, safety.rs, mod.rs
```

### Current MediaContext (the problem to fix this session)

```rust
// nika-kernel/src/scope.rs — CURRENT STATE (insufficient)
pub trait MediaContext: Send + Sync {
    fn blob_store(&self) -> &dyn crate::store::BlobStore;   // read-only CAS access
    fn working_dir(&self) -> &std::path::Path;              // path confinement
}
```

This is a forward declaration. It cannot support real media operations because:
- No `store_blob` (write + budget enforcement) — needed by import, decode, thumbnail, convert
- No `is_cancelled` — every tool needs it for long operations
- No `compute_blocking` — CPU-bound ops (image decode/encode) MUST NOT block tokio thread

---

## 2. ARCHITECTURAL CONTEXT

### 2.1 The media tool architecture (IMPORTANT — differs from other tools)

Media tools do NOT use the `KernelToolAdapter` pattern. They have their own adapter:

```
nika-media/src/tools/X.rs:
  impl MediaOp for XTool { ... returns MediaToolError ... }

nika-engine/src/runtime/builtin/media/mod.rs:
  struct MediaToolAdapter { op: Arc<dyn MediaOp>, ctx: Arc<dyn MediaContext> }
  impl BuiltinTool for MediaToolAdapter { ... converts MediaToolError → NikaError ... }

  fn create_media_tool_adapters(ctx: Arc<dyn MediaContext>) -> Vec<Box<dyn BuiltinTool>>
```

**The goal for Session 8:** The adapter layer already uses `Arc<dyn MediaContext>` (or should
after 12.9). The refactor is to expand the `MediaContext` trait in nika-kernel so it exposes
everything nika-media tools actually need, then wire `EngineMediaContext` as the concrete impl.

**What we are NOT doing:** Moving `MediaOp` implementations from nika-media to nika-builtin.
They stay in nika-media. Only the `MediaContext` trait contract changes.

### 2.2 MediaToolContext ground truth (verified at tools/nika-media/src/tools/context.rs:23-37)

```rust
pub struct MediaToolContext {
    pub cas: CasStore,                           // owned by value, NOT Arc
    pub budget: Arc<MediaBudget>,                // quota tracker (shared)
    pub compute: Arc<ComputePool>,               // rayon wrapper
    pub working_memory: Arc<WorkingMemoryBudget>, // RAM budget for decode buffers
    pub cancel: CancellationToken,               // ← field is `cancel`, NOT `cancellation_token`
    pub working_dir: Option<std::path::PathBuf>,
}
```

**Verified API calls from existing media tools:**
- Read blob: `ctx.read_media(hash).await?` (delegates to `ctx.cas.read(hash).await`)
- Store blob: `ctx.store_media(data, task_id).await?` (budget-aware, auto-rollback on failure)
- Cancellation (Result form): `ctx.check_cancelled()?` — allocates MediaToolError on trigger
- Cancellation (bool form): `ctx.cancel.is_cancelled()` — non-allocating hot-path check
- Working memory guard: `let guard = ctx.working_memory.acquire(bytes)?;`
- Rayon dispatch: `ctx.compute.compute(move || { ... }).await??` (note the DOUBLE ??)

### 2.3 The three async patterns in nika-media (know these before coding)

**Pattern 1 — CAS I/O (already async, uses methods verified in §2.2):**
```rust
// Inside a MediaOp::execute — ctx is &MediaToolContext:
let data = ctx.read_media(hash).await?;
```

**Pattern 2 — CPU-bound work (MUST use compute pool, NEVER block tokio):**
```rust
// WRONG — blocks the tokio thread with ~33MB decode for a 4K image:
let img = decode_image_safe(&data)?;

// CORRECT — rayon bridge via MediaToolContext.compute.compute:
let (decoded, meta) = ctx
    .compute
    .compute(move || -> Result<(DecodedImage, Metadata), MediaToolError> {
        let img = decode_image_safe(&data)?;
        let meta = extract_metadata(&img);
        Ok((img, meta))
    })
    .await??;
//      ^^
//      First ? unwraps the oneshot channel (compute returns Result<T, MediaToolError>).
//      Second ? unwraps the closure's Result<(Decoded, Meta), MediaToolError>.
```

**Pattern 3 — Cancellation check (MANDATORY before/after expensive ops):**
```rust
// Use check_cancelled() for idiomatic error propagation via ?:
ctx.check_cancelled()?;
let data = ctx.read_media(hash).await?;
ctx.check_cancelled()?;
let output = ctx.compute.compute(move || { /* expensive */ }).await??;
ctx.check_cancelled()?;

// Rayon cannot be interrupted mid-closure — checks happen BEFORE dispatch.
// The rayon worker will run the full closure even if cancel fires during.
// If cancellation matters for long operations, split the work across multiple
// compute.compute() calls with check_cancelled() between them.
```

### 2.3 Known async hazards to fix in nika-media (found by architectural analysis)

These MUST be fixed as part of 12.10 before refactoring the adapters:

**Hazard H1 — `import.rs`: `path.canonicalize()` blocks tokio thread**
```rust
// BROKEN (in nika-media/src/tools/import.rs):
let canonical = path.canonicalize()?;  // blocking syscall

// FIX:
let canonical = tokio::fs::canonicalize(&path).await.map_err(|e| ...)?;
```

**Hazard H2 — `thumbnail.rs`: WorkingMemoryBudget not acquired before rayon dispatch**
```rust
// BROKEN — 4K image × concurrency:10 = 330MB of untracked decode buffers:
ctx.compute.compute(|| { decode_image_safe(&data) }).await??;

// FIX — acquire budget guard on tokio side before dispatching:
let estimated_size = data.len() * 4;  // worst-case RGBA decode
let _guard = ctx.working_memory.acquire(estimated_size).map_err(|e| ...)?;
let output = ctx.compute.compute(move || { decode_image_safe(&data) }).await??;
// _guard drops here, releasing budget
```

**Hazard H3 — `ComputePool`: panic handler loses panic message**
```rust
// BROKEN — absorbs panic, logs nothing useful:
.panic_handler(|_info| {})

// FIX — extract and log the message:
.panic_handler(|info| {
    let msg = info.downcast_ref::<&str>().copied()
        .or_else(|| info.downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("unknown");
    tracing::error!(target: "nika_media", "compute thread panicked: {msg}");
})
```

### 2.4 Shield invariants (MUST NOT BREAK)

- Trust via `task_local!` only — never passed as function argument
- `check_path_readable` in `nika-builtin/src/file/shield.rs` must keep working
- Untrusted agents blocked from reading: `nika.toml`, `.mcp.json`, `.env*`, `*.nika.yaml`
- Media operations from untrusted agent input need `is_cancelled` + trust checks

---

## 3. SESSION 8 COMMIT PLAN

> **Strategy for media tools:** The MediaOp impls stay in nika-media. The work is:
> (a) expand MediaContext trait, (b) fix 3 async hazards, (c) wire EngineMediaContext,
> (d) clean up router. LOC reduction from nika-engine: ~4-5k (not 28k — that's Phase 14).

---

### Commit 12.9 — Expand MediaContext + wire EngineMediaContext

**Goal:** Define the complete `MediaContext` API in nika-kernel. Wire `EngineMediaContext`
in nika-engine. No behavioral changes — all existing tests must still pass.

**TDD protocol:**
1. Write tests for `EngineMediaContext` first (trait conformance tests)
2. Expand the trait — this will cause compilation failures in the engine impl
3. Fix the engine impl to satisfy the new contract
4. Run ALL tests — must stay at current count with zero new failures

**Step 0: Object-safety verification (do this FIRST)**

```rust
// Add to nika-kernel/src/scope.rs after the trait definition:
#[cfg(test)]
mod _object_safety_asserts {
    use super::*;
    // Compile-fail guard: if someone adds a generic method to MediaContext,
    // this fn declaration fails to compile.
    fn _assert_object_safe(_: &dyn MediaContext) {}
}
```

> **Why this matters:** A trait with `fn compute_blocking<F, T>(&self, f: F)` is NOT
> object-safe — you cannot call it through `&dyn MediaContext` because the vtable
> doesn't know the concrete `F` and `T`. The v1 handoff proposed such a method and
> it would have broken every `Arc<dyn MediaContext>` in the plan.
>
> **Solution:** Compute-bound closures stay on the concrete `MediaToolContext`, NOT
> on the trait. Media tools (nika-media) already have access to `MediaToolContext`
> directly. The trait only exposes object-safe methods that external consumers
> (nika-builtin, tests, future verb crates) actually need.

**Step 1: Expand `nika-kernel/src/scope.rs` — object-safe methods only**

```rust
// Add new supporting type (before the trait):
#[derive(Debug, Clone)]
pub struct BlobStoreResult {
    pub hash: String,
    pub size: u64,
    pub deduplicated: bool,
    // Note: nika-media's StoreResult has additional fields (path, verified, pipeline_ms)
    // that are intentionally NOT exposed in the kernel abstraction. Consumers needing
    // those fields must downcast to the concrete type.
}

// Replace the existing thin MediaContext (scope.rs:170-176) with:
#[async_trait::async_trait]
pub trait MediaContext: Send + Sync {
    // ── CAS read (object-safe) ──────────────────────────────────
    /// Read a blob by its blake3 hash. Returns raw bytes.
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, crate::builtin::BuiltinError>;

    // ── CAS write with budget enforcement (object-safe) ─────────
    /// Store bytes into CAS with budget enforcement.
    /// `task_id` is used for budget attribution and dedup tracking.
    /// On failure, budget is rolled back automatically.
    async fn store_blob(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<BlobStoreResult, crate::builtin::BuiltinError>;

    // ── Path confinement (object-safe) ──────────────────────────
    /// Working directory boundary for import path validation.
    /// Returns None when no project root is set (REPL mode, tests).
    fn working_dir(&self) -> Option<&std::path::Path>;

    // ── Cancellation (object-safe) ──────────────────────────────
    /// Non-allocating cancellation check. Returns true if the workflow
    /// has been cancelled. Hot path — called in inner loops.
    fn is_cancelled(&self) -> bool;

    // ── Legacy shim (object-safe, kept for pipeline access) ─────
    /// Direct access to the raw blob store. Use `read_blob`/`store_blob` when possible.
    fn blob_store(&self) -> &dyn crate::store::BlobStore;
}

// NOT on the trait (object safety): compute_blocking<F, T>.
// Media tools use ctx.compute.compute(f).await directly via MediaToolContext,
// which is always available inside nika-media. The trait exists for nika-builtin
// and test mocks, neither of which need to dispatch CPU-bound closures.
```

**Design note on cancellation:** The trait uses `is_cancelled() -> bool` (non-allocating,
hot-path friendly) instead of `check_cancelled() -> Result<()>` which allocates a
`MediaToolError` per call. Tool implementations check `if ctx.is_cancelled() { return Err(...); }`
before and after expensive operations. **Rayon closures cannot be cancelled mid-execution** —
a rayon thread decoding a 4K image will run to completion even if the tokio caller is cancelled.
Always check before dispatching to `compute_blocking`.

**Design note on task_local visibility:** `tokio::task_local!` values are NOT visible inside
rayon closures. If a media tool needs to read `CURRENT_TASK_TRUST` during CPU work, it must
capture the value on the tokio side and move it into the closure:

```rust
// WRONG — task_local not visible in rayon closure:
ctx.compute.compute(|| {
    let trust = nika_kernel::task_local::current_task_trust();  // returns default!
    ...
}).await?

// CORRECT — capture before dispatch:
let trust = nika_kernel::task_local::current_task_trust();
ctx.compute.compute(move || {
    // trust is now available inside the rayon closure
    if !trust.is_trusted() { ... }
}).await?
```

**Step 2: Add `EngineMediaContext` in nika-engine**

New file `nika-engine/src/runtime/media_context.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;
use nika_kernel::{
    builtin::BuiltinError,
    scope::{BlobStoreResult, MediaContext},
};
use nika_media::tools::context::MediaToolContext;

/// Engine-side implementation of `MediaContext`.
///
/// Wraps `MediaToolContext` (nika-media's concrete type) and adapts it
/// to the kernel trait. This is the bridge between the kernel trait
/// (consumed by nika-builtin and future verb crates) and the engine's
/// concrete media infrastructure.
///
/// # Compute dispatch
///
/// `compute_blocking<F, T>` is intentionally NOT on the `MediaContext` trait
/// because it is not object-safe (generic methods break vtable dispatch).
/// Media tools (nika-media) access `MediaToolContext.compute` directly.
pub struct EngineMediaContext {
    inner: Arc<MediaToolContext>,
}

impl EngineMediaContext {
    pub fn new(ctx: Arc<MediaToolContext>) -> Self {
        Self { inner: ctx }
    }

    /// Access the underlying concrete context. Use sparingly — prefer the
    /// trait methods when possible.
    pub fn inner(&self) -> &MediaToolContext {
        &self.inner
    }
}

#[async_trait::async_trait]
impl MediaContext for EngineMediaContext {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError> {
        // Verified API: MediaToolContext.read_media delegates to cas.read().
        // cas is a direct field (CasStore), not Arc-wrapped.
        self.inner
            .read_media(hash)
            .await
            .map_err(|e| BuiltinError::Io {
                tool: "media:read".into(),
                reason: format!("CAS read failed for {hash}: {e}"),
            })
    }

    async fn store_blob(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<BlobStoreResult, BuiltinError> {
        // MediaToolContext::store_media handles budget check + rollback on failure.
        // Returns nika_media::store::StoreResult (6 fields). We project to the
        // 3-field kernel subset — consumers needing path/verified/pipeline_ms
        // must access via EngineMediaContext::inner() downcast.
        let result = self.inner.store_media(data, task_id).await.map_err(|e| {
            BuiltinError::Io {
                tool: "media:store".into(),
                reason: format!("CAS store failed: {e}"),
            }
        })?;
        Ok(BlobStoreResult {
            hash: result.hash,
            size: result.size,
            deduplicated: result.deduplicated,
        })
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        // MediaToolContext.working_dir is Option<PathBuf>; as_deref → Option<&Path>
        self.inner.working_dir.as_deref()
    }

    fn is_cancelled(&self) -> bool {
        // Field is `cancel: CancellationToken` (NOT `cancellation_token`).
        // Use is_cancelled() on the token directly (non-allocating, no Result construction).
        self.inner.cancel.is_cancelled()
    }

    fn blob_store(&self) -> &dyn nika_kernel::store::BlobStore {
        &self.inner.cas
    }
}

// Object-safety compile assertion — uncomment if object safety is ever in doubt:
#[cfg(test)]
const _: fn(&dyn MediaContext) = |_| {};
```

**How to construct an `Arc<dyn MediaContext>` at call sites:**

```rust
// Rust unsizing coercion Arc<T> → Arc<dyn Trait> happens at a BINDING site,
// not deep in an expression. Prefer an explicit let binding with type ascription:

let media_ctx: Arc<dyn MediaContext> =
    Arc::new(EngineMediaContext::new(media_tool_ctx));

let router = BuiltinToolRouter::new()
    .with_media(media_ctx)
    .with_file_tools(file_ctx);

// Avoid inline construction — this fails because coercion does not fire
// inside a method call's argument position for generic inference:
// let router = BuiltinToolRouter::new()
//     .with_media(Arc::new(EngineMediaContext::new(media_tool_ctx)));  // ❌ may fail
```

**Step 3: Update `create_media_tool_adapters()` signature**

In `nika-engine/src/runtime/builtin/media/mod.rs`, change:
```rust
// Before:
pub fn create_media_tool_adapters(ctx: Arc<MediaToolContext>) -> Vec<Box<dyn BuiltinTool>>

// After:
pub fn create_media_tool_adapters(ctx: Arc<dyn MediaContext>) -> Vec<Box<dyn BuiltinTool>>
```

All callers pass `Arc::new(EngineMediaContext::new(media_ctx))`.

**Tests to write for 12.9** (see §4.0 for `make_test_engine_media_context` definition):

```rust
// nika-engine/src/runtime/media_context_tests.rs
use super::EngineMediaContext;
use nika_kernel::scope::MediaContext;
use std::sync::Arc;

// Object-safety assertion — compile-time test
const _ASSERT_OBJECT_SAFE: fn(&dyn MediaContext) = |_| {};
const _ASSERT_ARC_DYN: fn() -> Arc<dyn MediaContext> = || -> Arc<dyn MediaContext> {
    unreachable!()
};

#[tokio::test]
async fn engine_media_context_is_not_cancelled_initially() {
    let ctx = make_test_engine_media_context();
    assert!(!ctx.is_cancelled());
}

#[tokio::test]
async fn engine_media_context_store_and_read_roundtrip() {
    let ctx = make_test_engine_media_context();
    let data = b"hello world";
    let result = ctx.store_blob(data, "test_task").await.unwrap();

    // Validate programmatically — type, prefix, size
    assert!(result.hash.starts_with("blake3:"), "hash must have blake3: prefix");
    assert_eq!(result.size, data.len() as u64);

    let retrieved = ctx.read_blob(&result.hash).await.unwrap();
    assert_eq!(retrieved, data);
}

#[tokio::test]
async fn engine_media_context_dedup_on_second_store() {
    let ctx = make_test_engine_media_context();
    let data = b"dedup test bytes";
    let first = ctx.store_blob(data, "task1").await.unwrap();
    let second = ctx.store_blob(data, "task2").await.unwrap();
    assert_eq!(first.hash, second.hash);
    assert!(second.deduplicated, "second store with same bytes must report deduplicated");
}

#[tokio::test]
async fn engine_media_context_read_unknown_hash_is_io_error() {
    let ctx = make_test_engine_media_context();
    let result = ctx.read_blob("blake3:nonexistent0000000000000000000000000000000000000000000000000000000").await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("media:read"), "error must identify the failing tool: {err}");
}

#[tokio::test]
async fn engine_media_context_store_rolls_back_budget_on_cas_failure() {
    // Inject a CAS that always fails (see §4.0 FailingCasMediaContext fixture)
    let ctx = make_failing_cas_context();
    let budget_before = ctx.inner().budget.current_usage();
    let _ = ctx.store_blob(b"data", "task1").await;
    let budget_after = ctx.inner().budget.current_usage();
    assert_eq!(budget_before, budget_after, "budget must be rolled back on CAS failure");
}

#[tokio::test]
async fn engine_media_context_working_dir_matches_inner() {
    let ctx = make_test_engine_media_context();
    let got = ctx.working_dir();
    let expected = ctx.inner().working_dir.as_deref();
    assert_eq!(got, expected);
}

#[tokio::test]
async fn engine_media_context_cancellation_observable_via_trait() {
    let (ctx, cancel_tx) = make_cancellable_engine_media_context();
    assert!(!ctx.is_cancelled());
    cancel_tx.cancel();
    assert!(ctx.is_cancelled());
}

#[tokio::test]
async fn engine_media_context_store_empty_bytes_succeeds() {
    let ctx = make_test_engine_media_context();
    let result = ctx.store_blob(&[], "empty").await.unwrap();
    assert_eq!(result.size, 0);
    assert!(result.hash.starts_with("blake3:"));
}

#[tokio::test]
async fn engine_media_context_blob_store_shim_functions() {
    let ctx = make_test_engine_media_context();
    let store = ctx.blob_store();
    // Exercise the trait method to confirm the shim works
    let _ = store;  // type-check only
}
```

> **Note:** compute_blocking is NOT on the trait (see Step 1). Tests for rayon
> dispatch live in `nika-media/src/tools/context.rs` tests, not here.

**Commit message:**
```
refactor(kernel): expand MediaContext trait — read_blob, store_blob, is_cancelled, blob_store

Wire EngineMediaContext in nika-engine (adapts MediaToolContext → MediaContext).
compute_blocking intentionally NOT on trait (not object-safe — media tools use
MediaToolContext.compute directly). All existing tests must pass — no
behavioral change, only API expansion.
```

---

### Commit 12.9b — Update nika-kernel-mock with MediaContext impl

**What:** Add `nika-kernel-mock/src/media.rs` with `MockMediaContext` (see §4.0 File 2).
nika-kernel-mock holds hand-written mocks for all kernel traits — adding one for
`MediaContext` ensures nika-builtin tests can reference it without pulling nika-engine.

**Step 1:** Create `nika-kernel-mock/src/media.rs` with the `MockMediaContext` from §4.0.

**Step 2:** Re-export from `nika-kernel-mock/src/lib.rs`:
```rust
pub mod media;
pub use media::MockMediaContext;
```

**Step 3:** Update `nika-kernel-mock/Cargo.toml` (see §4.0).

**Step 4:** Verify `nika-kernel-mock` compiles and its own tests pass.

**Tests to add:** MockMediaContext conformance tests — call each trait method, verify
the tracking atomics update correctly (3-5 small tests).

**Commit message:**
```
feat(kernel-mock): add MockMediaContext for testing media tools without nika-engine
```

---

---

### Commit 12.10 — Fix 3 async hazards in nika-media

**Goal:** Fix 3 real bugs before the adapter refactor builds on top of them.

**TDD protocol:**
1. Write a test that demonstrates the bug (use `#[should_panic]` or a timeout test for H1)
2. Confirm the test fails/panics without the fix
3. Apply the fix
4. Confirm the test passes

**Fix H1 — Blocking canonicalize in `nika-media/src/tools/import.rs`:**

Find: `path.canonicalize()` (anywhere in the file)
Replace with: `tokio::fs::canonicalize(&path).await`

Adapt the surrounding error handling since return type changes from sync to async Result.

```rust
// Test that demonstrates H1 indirectly (slow canonicalize on mounted FS would hang):
#[tokio::test]
async fn import_validates_path_without_blocking() {
    // Test that import path validation completes within a tight timeout.
    // Blocking canonicalize on a slow mount would exceed this.
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        validate_import_path_async("/tmp/test.jpg"),
    ).await;
    // Should complete, not timeout — even on slow FS
    assert!(result.is_ok());
}
```

**Fix H2 — Missing WorkingMemoryBudget in `nika-media/src/tools/thumbnail.rs`:**

Current code (verified at `thumbnail.rs:80-152`): `ctx.compute.compute(move || decode_image_safe(&data))`
dispatches directly to rayon without acquiring the working-memory budget first. Under
`concurrency: 10` with 4K images, 10 × ~33 MB decode buffers = 330 MB untracked.

Add budget acquisition BEFORE the rayon dispatch:

```rust
// After reading the blob, before compute.compute():
let estimated_decode_bytes = (data.len() * 4).max(4096); // worst-case RGBA, min 4KB
let budget_guard = ctx
    .working_memory
    .acquire(estimated_decode_bytes)
    .map_err(|e| tool_error("thumbnail", format!("memory budget: {e}")))?;
//  ^^^^^^^^^^^^
//  Important: `let budget_guard = ...` keeps the RAII guard alive until the end of
//  scope. NEVER write `let _ = ...` — the underscore-only binding drops immediately.
//  `let _budget_guard = ...` (underscore + name) is also valid — the name matters.

let output = ctx
    .compute
    .compute(move || decode_image_safe(&data))
    .await
    .map_err(|e| tool_error("thumbnail", e.to_string()))??;

// Explicitly drop the guard AFTER the rayon work completes so its contribution
// stays charged for the full duration of the decode. Without this, it would drop
// at the end of the function — which is actually fine too. We write it explicitly
// to document intent.
drop(budget_guard);
```

> **Rust guard pitfall:** `let _ = expr;` drops `expr` IMMEDIATELY (the result is
> discarded). `let _name = expr;` keeps `expr` alive until end of scope (the binding
> has a name, just starts with `_` to suppress the unused warning). This is one of
> the most common sources of silent behavior changes when refactoring. The test
> `budget_guard_not_dropped_prematurely` in §4.3 guards against regressions here.

```rust
// Test for H2:
#[tokio::test]
async fn thumbnail_respects_memory_budget_under_concurrency() {
    // Create a context with a tight memory budget (1MB)
    let ctx = make_test_context_with_budget(1_000_000);
    let large_image = make_test_image(2048, 2048); // ~16MB decoded
    let hash = ctx.store_blob(&large_image, "test").await.unwrap().hash;

    // Attempting thumbnail with budget exhausted should error gracefully
    let result1 = ThumbnailTool.execute(thumb_args(&hash, 256), &ctx).await;
    // First call may succeed or fail depending on budget
    let result2 = ThumbnailTool.execute(thumb_args(&hash, 256), &ctx).await;
    let result3 = ThumbnailTool.execute(thumb_args(&hash, 256), &ctx).await;
    // At least one should fail with budget error, none should panic
    let errors = [&result1, &result2, &result3].iter().filter(|r| r.is_err()).count();
    assert!(errors > 0 || result1.is_ok(), "budget should either allow or reject cleanly");
}
```

**Fix H3 — ComputePool panic handler in `nika-media/src/tools/context.rs`:**

Find the `.panic_handler(|_| {})` or similar in the rayon pool builder.
Replace:
```rust
.panic_handler(|info| {
    let msg = info.downcast_ref::<&str>().copied()
        .or_else(|| info.downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("unknown panic");
    tracing::error!(target: "nika_media", "compute thread panicked: {msg}");
})
```

```rust
// Test for H3:
#[tokio::test]
async fn compute_pool_panic_returns_error_not_crash() {
    let ctx = make_test_context();
    let result = ctx.compute.compute(|| -> String {
        panic!("deliberate panic for test");
    }).await;
    assert!(result.is_err());
    // The calling thread (tokio) must NOT have panicked
}

#[tokio::test]
async fn compute_pool_subsequent_tasks_work_after_panic() {
    let ctx = make_test_context();
    // First: panic
    let _ = ctx.compute.compute(|| -> u64 { panic!("first panic") }).await;
    // Second: normal work must still complete
    let result = ctx.compute.compute(|| 42u64).await.unwrap();
    assert_eq!(result, 42);
}
```

**Commit message:**
```
fix(media): fix 3 async hazards — blocking canonicalize, memory budget, panic handler

H1: import.rs used std::path::canonicalize() (blocking) in async context →
    replaced with tokio::fs::canonicalize()
H2: thumbnail.rs didn't acquire WorkingMemoryBudget before rayon dispatch →
    under concurrency:10 with 4K images, 330MB of untracked buffers possible
H3: ComputePool panic handler silently discarded panic messages → now logs at error level
```

---

### Commit 12.11 — Refactor MediaToolAdapter to use Arc<dyn MediaContext>

**Revised scope (post-review):** The original plan proposed updating `MediaOp::execute` to
take `&dyn MediaContext`. That requires touching all 24 `impl MediaOp` files AND losing
access to concrete methods (like `ctx.compute.compute()`) because compute is not on the trait.

**Cleaner approach:** `MediaToolAdapter` holds BOTH the trait object AND the concrete type.
`MediaOp::execute` keeps its existing signature `&MediaToolContext`. The trait object exists
only for router injection and future nika-builtin consumers. This avoids rewriting 24 files.

```rust
// nika-engine/src/runtime/builtin/media/mod.rs — revised:
pub(crate) struct MediaToolAdapter {
    op: Arc<dyn MediaOp>,
    // Kept for MediaOp::execute — media tools still use concrete type:
    concrete_ctx: Arc<MediaToolContext>,
    // Trait object for router/test injection (shares the same backing data):
    trait_ctx: Arc<dyn MediaContext>,
    name: &'static str,
    timeout: Duration,
}

impl MediaToolAdapter {
    pub fn new(op: Arc<dyn MediaOp>, engine_ctx: Arc<EngineMediaContext>) -> Self {
        let concrete_ctx = Arc::clone(&engine_ctx.inner_arc());  // helper we add
        let trait_ctx: Arc<dyn MediaContext> = engine_ctx;
        let name = op.name();
        Self { op, concrete_ctx, trait_ctx, name, timeout: DEFAULT_TIMEOUT }
    }
}
```

Add a helper to `EngineMediaContext`:
```rust
impl EngineMediaContext {
    /// Access the backing Arc<MediaToolContext> — used by adapter construction
    /// to share a single context across both the trait object and direct calls.
    pub fn inner_arc(&self) -> &Arc<MediaToolContext> {
        &self.inner
    }
}
```

**BuiltinTool impl for the adapter** — the adapter calls `self.op.execute(args, &self.concrete_ctx)`
(unchanged from today). The trait_ctx field is unused BY the adapter itself; it's kept so that
future tests or extensions can observe/inject via the trait.

**12.11 is now a single commit** (not split into a/b) because only the adapter changes.
The 24 MediaOp implementations are untouched.

**Commit message:**
```
refactor(engine): MediaToolAdapter stores Arc<dyn MediaContext> alongside concrete ctx
```

**If we later want nika-builtin to use media tools directly** (not via the engine adapter),
that would require `execute` to take `&dyn MediaContext`. That decision is deferred to a future
phase — not blocking Session 8.

---

### Commit 12.11c — Update engine integration tests

**What:** The 8 `tests_*.rs` files in `nika-engine/src/runtime/builtin/media/` construct
`MediaToolAdapter` directly in some tests. After 12.11, those constructors take different
arguments. Update them.

**Files to check:**
```
tests_comprehensive.rs    tests_e2e_workflow.rs     tests_import_integration.rs
tests_integration.rs      tests_paranoid.rs         tests_pr3b_tools.rs
tests_pr4_pipelines.rs    tests_pr5_integration.rs  tests_security.rs
```

**Action:** grep for `MediaToolAdapter {` and `MediaToolAdapter::new` in those files.
Update each construction site to use the new constructor via `EngineMediaContext`:

```rust
// Before (pre-12.11):
let adapter = MediaToolAdapter {
    op: Arc::new(ImportTool),
    ctx: Arc::new(make_test_context()),
    name: "import",
    timeout: Duration::from_secs(30),
};

// After (post-12.11):
let engine_ctx = Arc::new(EngineMediaContext::new(Arc::new(make_test_context())));
let adapter = MediaToolAdapter::new(Arc::new(ImportTool), engine_ctx);
```

**Verification:** `cargo test -p nika-engine --lib 'media::tests_' -q` — all must pass.

**Commit message:**
```
test(engine): update media integration tests for new MediaToolAdapter constructor
```

---

**Tests to add for 12.11a:**
```rust
// In nika-media/src/tools/tests_media_context_trait.rs

/// Verify that import uses ctx.store_blob() not ctx.cas.store() directly
#[tokio::test]
async fn import_calls_store_blob_on_context() {
    // Use a MockMediaContext that tracks which methods were called
    let mock = Arc::new(TrackingMediaContext::new());
    let result = ImportTool.execute(
        import_args("test.jpg"),
        mock.clone() as Arc<dyn MediaContext>,
    ).await;
    assert!(mock.store_blob_called());
}

/// import must use tokio canonicalize (async, non-blocking)
#[tokio::test]
async fn import_path_validation_is_async() {
    let ctx = make_mock_context();
    // This test would hang/timeout if canonicalize is sync
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        ImportTool.execute(import_args("/nonexistent/path.jpg"), ctx),
    ).await
    .expect("path validation must not block");
}

/// decode returns proper CAS hash and size
#[tokio::test]
async fn decode_stores_blob_and_returns_hash() {
    let ctx = make_test_media_context();
    let base64_input = base64_encode(b"fake-image-data");
    let result = DecodeTool.execute(
        decode_args(&base64_input, "image/png"),
        ctx,
    ).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
    assert!(parsed["size"].as_u64().unwrap() > 0);
}
```

**Commit messages:**
```
refactor(media): update MediaOp trait to take Arc<dyn MediaContext> — Tier 1 tools (12.11a)
refactor(media): update MediaOp trait to take Arc<dyn MediaContext> — Tier 2-3 tools (12.11b)
```

---

### Commit 12.12 — Router consuming builder + wire EngineMediaContext

**Goal:** `BuiltinToolRouter` uses `Arc<dyn MediaContext>` via `.with_media()` builder method.
Concrete wiring uses `EngineMediaContext`. No new functionality.

**Changes to `nika-engine/src/runtime/builtin/router.rs`:**

```rust
// Add builder method:
pub fn with_media(mut self, ctx: Arc<dyn MediaContext>) -> Self {
    for tool in create_media_tool_adapters(ctx) {
        self.tools.insert(tool.name(), Arc::from(tool));
    }
    self
}

// Keep with_all_tools for backwards compat but implement via builder:
pub fn with_all_tools(file_ctx: Arc<ToolContext>, media_ctx: Arc<MediaToolContext>) -> Self {
    Self::with_file_tools(file_ctx)
        .with_media(Arc::new(EngineMediaContext::new(media_ctx)))
}
```

**The production wiring in `nika-engine/src/runtime/executor/mod.rs` becomes:**
```rust
let router = BuiltinToolRouter::new()
    .with_file_tools(file_ctx)
    .with_media(Arc::new(EngineMediaContext::new(media_ctx)))
    .with_cost_tool(event_log.clone())
    .with_introspection(event_log, Arc::clone(&datastore))
    .with_run(Arc::new(EngineRunExecutor::new()))
    .with_prompt(Arc::new(HitlBridge::new(hitl_handler)));
```

**Tests to add for 12.12:**
```rust
#[tokio::test]
async fn router_with_media_registers_all_24_tools() {
    let media_ctx = make_test_engine_media_context();
    let router = BuiltinToolRouter::new()
        .with_media(Arc::new(media_ctx));

    // Verify all always-on tools are registered
    for tool_name in &["import", "decode", "dimensions", "thumbhash", "dominant_color"] {
        assert!(router.has_tool(tool_name),
            "with_media router must include '{tool_name}'");
    }
}

#[test]
fn router_builder_accepts_dyn_media_context() {
    // Compile-time test: Arc<dyn MediaContext> must be accepted
    let ctx: Arc<dyn MediaContext> = Arc::new(MockMediaContext::new());
    let _router = BuiltinToolRouter::new().with_media(ctx);
}
```

**Commit message:**
```
refactor(engine): router .with_media() accepts Arc<dyn MediaContext> — 12.12
```

---

### Commit 12.13 — Cleanup + delete with_all_tools

**Goal:** Remove dead code now that all adapters use traits + kill the legacy compound
`with_all_tools()` method (zero users rule — `dx/.claude/rules/feedback_no_backward_compat.md`).

**What to delete:**
- `BuiltinToolRouter::with_all_tools(file_ctx, media_ctx)` — superseded by the consuming builder
- Any call site of `with_all_tools` — migrate to `.with_file_tools().with_media()` chain
- `create_media_tool_adapters(ctx: Arc<MediaToolContext>)` old concrete-type signature (if still present)
- `nika-engine/src/tools/` directory contents that were migrated to nika-builtin/src/file/ in 12.6
  (verify: `grep -rn "use crate::tools::" tools/nika-engine/src/` returns zero matches first)

**Call sites of with_all_tools to migrate:**
```bash
grep -rn "with_all_tools" tools/ --include="*.rs"
```
Update each one to the builder chain. Then delete the method.

**What to KEEP (IMPORTANT — do not delete these):**
- `nika-engine/src/runtime/builtin/media/mod.rs` — reduced to adapter + test re-exports
- All `tests_*.rs` files in that module — they test engine integration with concrete types
- `EngineMediaContext` in nika-engine
- `MediaToolAdapter` (now using the new constructor)

**Verification checklist:**
```bash
# 1. No direct MediaToolContext in adapter (only in EngineMediaContext)
grep -r "MediaToolContext" tools/nika-engine/src/runtime/builtin/media/ | grep -v test | grep -v EngineMediaContext
# Expected: no output

# 2. tests_security.rs still exercises media Shield checks
cargo test -p nika-engine --lib tests_security -q
# Expected: all pass

# 3. Total test count at or above target
cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{s+=$4} END{print s}'
# Expected: ≥11,000

# 4. Clippy clean
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: clean Finished
```

**Commit message:**
```
chore(media): cleanup dead code after MediaContext trait refactor — 12.13
```

---

## 4.0 TEST FIXTURES — define these FIRST, before writing any other test

All test cases in §4.1-§4.5 assume these helpers exist. Create them as part of 12.9 (first
commit) so the TDD loop has something to call. Each fixture is a documented Rust definition —
copy-paste into the listed file.

### File 1: `nika-engine/src/runtime/media_context_tests.rs`

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::EngineMediaContext;
use std::sync::Arc;
use nika_media::tools::context::{
    MediaToolContext, ComputePool, WorkingMemoryBudget, MediaBudget,
};
use nika_media::store::CasStore;
use tokio_util::sync::CancellationToken;

/// Build a fully-functional EngineMediaContext backed by a temp directory CAS,
/// a 16 MB working memory budget, and a 2-thread rayon pool.
pub(crate) fn make_test_engine_media_context() -> EngineMediaContext {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cas = CasStore::new(tmp.path().join("cas")).expect("cas");
    let inner = MediaToolContext {
        cas,
        budget: Arc::new(MediaBudget::new(256 * 1024 * 1024)),  // 256 MB
        compute: Arc::new(ComputePool::new(2).expect("rayon pool")),
        working_memory: Arc::new(WorkingMemoryBudget::new(16 * 1024 * 1024)),  // 16 MB
        cancel: CancellationToken::new(),
        working_dir: Some(tmp.path().to_path_buf()),
    };
    // Keep tempdir alive for the test by leaking — acceptable in test-only code
    std::mem::forget(tmp);
    EngineMediaContext::new(Arc::new(inner))
}

/// Same as above but returns the CancellationToken handle so tests can trigger cancel.
pub(crate) fn make_cancellable_engine_media_context()
    -> (EngineMediaContext, CancellationToken)
{
    let tmp = tempfile::tempdir().expect("tempdir");
    let cancel = CancellationToken::new();
    let inner = MediaToolContext {
        cas: CasStore::new(tmp.path().join("cas")).expect("cas"),
        budget: Arc::new(MediaBudget::new(256 * 1024 * 1024)),
        compute: Arc::new(ComputePool::new(2).expect("rayon pool")),
        working_memory: Arc::new(WorkingMemoryBudget::new(16 * 1024 * 1024)),
        cancel: cancel.clone(),
        working_dir: Some(tmp.path().to_path_buf()),
    };
    std::mem::forget(tmp);
    (EngineMediaContext::new(Arc::new(inner)), cancel)
}

/// Build a context with a tight working memory budget for budget-exhaustion tests.
pub(crate) fn make_test_context_with_budget(bytes: usize) -> EngineMediaContext {
    let tmp = tempfile::tempdir().expect("tempdir");
    let inner = MediaToolContext {
        cas: CasStore::new(tmp.path().join("cas")).expect("cas"),
        budget: Arc::new(MediaBudget::new(256 * 1024 * 1024)),
        compute: Arc::new(ComputePool::new(2).expect("rayon pool")),
        working_memory: Arc::new(WorkingMemoryBudget::new(bytes)),
        cancel: CancellationToken::new(),
        working_dir: Some(tmp.path().to_path_buf()),
    };
    std::mem::forget(tmp);
    EngineMediaContext::new(Arc::new(inner))
}
```

### File 2: `nika-kernel-mock/src/media.rs` (NEW file — see commit 12.9b)

`nika-kernel-mock` already holds 5 hand-written mocks for kernel traits (MEMORY.md S2). Add a
sixth for `MediaContext`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later

use async_trait::async_trait;
use nika_kernel::{
    builtin::BuiltinError,
    scope::{BlobStoreResult, MediaContext},
    store::BlobStore,
};
use parking_lot::Mutex;
use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

/// In-memory MediaContext mock with call tracking. Use for unit tests that
/// want to verify which methods a media tool invoked.
pub struct MockMediaContext {
    blobs: Mutex<std::collections::HashMap<String, Vec<u8>>>,
    pub read_count: AtomicUsize,
    pub store_count: AtomicUsize,
    cancelled: std::sync::atomic::AtomicBool,
    working_dir: Option<std::path::PathBuf>,
    blob_store: MockBlobStore,
}

impl Default for MockMediaContext {
    fn default() -> Self {
        Self {
            blobs: Mutex::new(Default::default()),
            read_count: AtomicUsize::new(0),
            store_count: AtomicUsize::new(0),
            cancelled: Default::default(),
            working_dir: None,
            blob_store: MockBlobStore,
        }
    }
}

impl MockMediaContext {
    pub fn new() -> Self { Default::default() }
    pub fn with_working_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.working_dir = Some(dir); self
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn read_count(&self) -> usize { self.read_count.load(Ordering::Acquire) }
    pub fn store_count(&self) -> usize { self.store_count.load(Ordering::Acquire) }
}

#[async_trait]
impl MediaContext for MockMediaContext {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError> {
        self.read_count.fetch_add(1, Ordering::Release);
        self.blobs
            .lock()
            .get(hash)
            .cloned()
            .ok_or_else(|| BuiltinError::Io {
                tool: "mock:read".into(),
                reason: format!("blob not found: {hash}"),
            })
    }

    async fn store_blob(
        &self,
        data: &[u8],
        _task_id: &str,
    ) -> Result<BlobStoreResult, BuiltinError> {
        self.store_count.fetch_add(1, Ordering::Release);
        let hash = format!("blake3:{}", blake3::hash(data).to_hex());
        let mut blobs = self.blobs.lock();
        let deduplicated = blobs.contains_key(&hash);
        blobs.insert(hash.clone(), data.to_vec());
        Ok(BlobStoreResult {
            hash,
            size: data.len() as u64,
            deduplicated,
        })
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        self.working_dir.as_deref()
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn blob_store(&self) -> &dyn BlobStore {
        &self.blob_store
    }
}

// Minimal BlobStore stub — only used so blob_store() returns something.
struct MockBlobStore;
#[async_trait]
impl BlobStore for MockBlobStore {
    async fn get(&self, _hash: &str) -> Result<Vec<u8>, std::io::Error> {
        Err(std::io::Error::other("mock blob_store is stub"))
    }
    async fn put(&self, _data: &[u8]) -> Result<String, std::io::Error> {
        Err(std::io::Error::other("mock blob_store is stub"))
    }
}
```

### File 3: Pre-populated test images for media ops

```rust
// nika-engine/src/runtime/media_context_tests.rs — add below fixtures

/// A tiny 2×2 PNG for tests that need a real image (dimensions, decode).
pub(crate) fn tiny_png_bytes() -> Vec<u8> {
    // 2x2 pixel PNG — hex dump produced via:
    //   magick -size 2x2 xc:red /tmp/tiny.png && xxd -i /tmp/tiny.png
    vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02,
        0x08, 0x02, 0x00, 0x00, 0x00, 0xfd, 0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00,
        0x15, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0xfc, 0xcf, 0xc0, 0xc0,
        0xc0, 0xc0, 0x00, 0xc4, 0x80, 0x04, 0x30, 0x00, 0x00, 0x15, 0x00, 0x03,
        0xaa, 0x4c, 0xbb, 0x72, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
        0xae, 0x42, 0x60, 0x82,
    ]
}
```

### Required Cargo.toml additions

Add to `nika-engine/Cargo.toml` under `[dev-dependencies]`:
```toml
tempfile = "3"
blake3 = { version = "1", features = ["pure"] }
tokio-util = { version = "0.7", features = ["rt"] }
```

Add to `nika-kernel-mock/Cargo.toml`:
```toml
[dependencies]
async-trait = "0.1"
blake3 = { version = "1", features = ["pure"] }
parking_lot = "0.12"
nika-kernel = { path = "../nika-kernel" }
```

### Fixture verification

Before using these fixtures in any §4.1-§4.5 test:
```bash
cargo test -p nika-engine --lib media_context_tests::fixtures_self_test -q
# Must pass — confirms the fixtures themselves compile and work
```

A minimum `fixtures_self_test` module:
```rust
#[cfg(test)]
mod fixtures_self_test {
    use super::*;

    #[tokio::test]
    async fn can_build_engine_media_context() {
        let _ = make_test_engine_media_context();
    }

    #[tokio::test]
    async fn cancellable_context_starts_uncancelled() {
        let (ctx, _tok) = make_cancellable_engine_media_context();
        assert!(!ctx.is_cancelled());
    }

    #[tokio::test]
    async fn mock_media_context_tracks_calls() {
        use nika_kernel_mock::media::MockMediaContext;
        let mock = MockMediaContext::new();
        let r = mock.store_blob(b"test", "task").await.unwrap();
        assert!(r.hash.starts_with("blake3:"));
        assert_eq!(mock.store_count(), 1);
        let _ = mock.read_blob(&r.hash).await;
        assert_eq!(mock.read_count(), 1);
    }
}
```

---

## 4. DETAILED TEST REQUIREMENTS BY AREA

This section is a pre-written test checklist. For each item, write the test BEFORE the code.
Mark with ✓ as you complete them during the session.

### 4.1 EngineMediaContext (commit 12.9)

| # | Test name | What it verifies | Priority |
|---|-----------|------------------|----------|
| 1 | `store_and_read_roundtrip` | store + read returns same bytes | P0 |
| 2 | `store_dedup_same_content` | storing same bytes returns `deduplicated: true` | P0 |
| 3 | `read_unknown_hash_errors` | reading nonexistent hash → BuiltinError::Io | P0 |
| 4 | `compute_blocking_executes` | closure runs and result returns | P0 |
| 5 | `compute_blocking_panic_is_error` | panicking closure → Error (NOT crash) | P0 |
| 6 | `compute_blocking_sequential_after_panic` | next task works after panic | P1 |
| 7 | `is_cancelled_false_initially` | fresh context → not cancelled | P1 |
| 8 | `working_dir_matches_context` | returns correct path or None | P1 |
| 9 | `store_respects_budget` | over-budget store → BuiltinError | P1 |
| 10 | `blob_store_shim_works` | `blob_store()` returns functioning BlobStore | P2 |

### 4.2 Async hazard fixes (commit 12.10)

| # | Test name | What it verifies | Priority |
|---|-----------|------------------|----------|
| 1 | `import_canonicalize_is_async` | uses tokio::fs::canonicalize (not blocking std) — progress observable within N yields | P0 |
| 2 | `compute_panic_returns_error` | panic → Err, not thread crash | P0 |
| 3 | `compute_subsequent_after_panic` | pool functional after panic | P0 |
| 4 | `compute_panic_logs_message` | tracing subscriber captures the panic message (not "unknown") | P1 |
| 5 | `thumbnail_budget_tracked` | working memory acquired before rayon | P1 |
| 6 | `budget_guard_not_dropped_prematurely` | `let _guard = acquire(...)` lives until scope end (NOT `let _ = ...`) | P0 |
| 7 | `thumbnail_concurrent_budget_exhaustion` | 10 concurrent calls → some error gracefully | P1 |
| 8 | `budget_rolled_back_on_cas_failure` | working memory released if store_media fails mid-flight | P1 |

### 4.3 MediaToolAdapter refactor (commit 12.11)

Since 12.11 only changes the adapter construction (MediaOp impls unchanged), tests focus
on the adapter + trait-object usage patterns.

| # | Test name | What it verifies | Priority |
|---|-----------|------------------|----------|
| 1 | `adapter_new_stores_both_contexts` | concrete_ctx and trait_ctx share backing data | P0 |
| 2 | `adapter_routes_to_media_op_execute` | BuiltinTool::call → MediaOp::execute with concrete_ctx | P0 |
| 3 | `adapter_respects_timeout` | 30s timeout enforced around MediaOp::execute | P1 |
| 4 | `adapter_error_maps_to_nika_error` | MediaToolError → NikaError with correct NIKA-29X code | P0 |
| 5 | `trait_ctx_observable_from_adapter` | testing hook: trait_ctx can be queried for mock assertions | P2 |
| 6 | `mock_media_context_conformance` | MockMediaContext satisfies MediaContext trait | P0 |
| 7 | `mock_media_context_tracks_store_count` | store_blob increments store_count atomic | P1 |
| 8 | `mock_media_context_tracks_read_count` | read_blob increments read_count atomic | P1 |

### 4.4 Router changes (commit 12.12)

| # | Test name | What it verifies |
|---|-----------|-----------------|
| 1 | `with_media_registers_tier1_tools` | import/decode/dimensions/thumbhash/color registered |
| 2 | `with_media_accepts_dyn_trait` | Arc<dyn MediaContext> accepted (compile-time) |
| 3 | `with_all_tools_unchanged_behavior` | existing with_all_tools still works |
| 4 | `router_builder_is_chainable` | .with_file().with_media().with_cost() all chain |
| 5 | `router_test_for_26_base_tools` | new() still has 26 base tools (regression) |

### 4.5 Edge cases (write these to prevent regressions)

```rust
// CAS: hash format validation
#[test]
fn store_blob_hash_starts_with_blake3_prefix() {
    // All hashes from store_blob must be "blake3:..." for pipeline compatibility
}

// Cancellation timing
#[tokio::test]
async fn read_blob_after_cancellation_errors_immediately() {
    let ctx = make_cancellable_context();
    ctx.cancel();
    let result = ctx.read_blob("any_hash").await;
    assert!(result.is_err());
}

// Empty data
#[tokio::test]
async fn store_empty_blob_succeeds() {
    let ctx = make_test_context();
    let result = ctx.store_blob(&[], "task").await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().size, 0);
}

// Compute blocking with large output
#[tokio::test]
async fn compute_blocking_handles_large_output() {
    let ctx = make_test_context();
    let result = ctx.compute_blocking(|| vec![0u8; 10 * 1024 * 1024]).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 10 * 1024 * 1024);
}
```

---

## 5. KNOWN ISSUES / DEBT (DO NOT FIX THIS SESSION — unless it's blocking)

These are tracked but intentionally deferred:

| Issue | Blocker for? | Resolution |
|-------|-------------|------------|
| `token_count` model param ignored | Nothing — heuristic works | tiktoken dispatch, post-launch |
| `locale_lookup` returns Err for not-found | Nothing | API design decision needed |
| `nika-engine/src/registry/` (~2870 LOC) | Nothing | Nuke in dedicated commit |
| Introspection tools (task_status, records, orchestrate) | Phase 13+ | RecordView DTO in nika-core |
| Phase 14: nika-runtime extraction | ≤100k LOC target | After Phase 12 completes |
| `IndexedDag` not wired into Runner | Performance | Phase 14 |
| `error_domains.rs` promotion (180 call sites) | None | Phase 6 dedicated session |

---

## 6. INVARIANTS — NEVER BREAK THESE

1. **Trust via task_local only** — never pass `TrustLevel` as a function argument to any `call()` method
2. **All new files: AGPL-3.0-or-later header** — verify with grep before commit
3. **Zero `.unwrap()` in production paths** — use `?` + `BuiltinError::*`
4. **`async fn` in traits needs `#[async_trait]`** — never `#[async_trait(?Send)]`, futures MUST be `Send`
5. **No generic methods on `MediaContext`** — breaks trait object dispatch. If tempted, put helper on `EngineMediaContext` concrete type instead
6. **No blocking I/O on tokio threads** — always `spawn_blocking` or rayon bridge
7. **task_local NOT visible in rayon closures** — capture values on tokio side before dispatching
8. **`let _name = guard;` NOT `let _ = guard;`** — the latter drops immediately
9. **Tests validate values, not existence** — `assert_eq!(parsed["hash"].as_str().unwrap().len(), 71)` not `assert!(!parsed["hash"].is_null())`
10. **Commit co-author:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic
11. **test --lib only** — never `cargo test` without `--lib` (avoids macOS keychain popups)
12. **One commit per logical unit** — no "misc fixes" mega-commits
13. **Zero backward-compat** — delete `with_all_tools` in 12.13, don't deprecate. Zero users rule.
14. **Object-safety compile assertion** — `const _: fn(&dyn MediaContext) = |_| {};` in nika-kernel/src/scope.rs must compile after every trait change

---

## 7. ARCHITECTURE DIAGRAM — MEDIA PIPELINE AFTER SESSION 8

```
nika-kernel (L0.5)
  scope.rs: MediaContext trait {
    read_blob(), store_blob(), working_dir(),
    is_cancelled(), compute_blocking(), blob_store()
  }
  BlobStoreResult struct

nika-media (L2, stays)
  tools/X.rs: impl MediaOp for XTool {
    execute(&self, args, ctx: Arc<dyn MediaContext>)
  }
  tools/context.rs: MediaToolContext (concrete, internal)

nika-engine (L2)
  runtime/media_context.rs: EngineMediaContext
    impl MediaContext for EngineMediaContext (wraps Arc<MediaToolContext>)
  runtime/builtin/media/mod.rs:
    MediaToolAdapter { op, ctx: Arc<dyn MediaContext> }
    create_media_tool_adapters(Arc<dyn MediaContext>) -> Vec<Box<dyn BuiltinTool>>
    [8 integration test files — stay here]
  runtime/builtin/router.rs:
    .with_media(Arc<dyn MediaContext>) → chains adapters
```

---

## 8. QUICK REFERENCE

```bash
# Before every commit — must be green:
cargo test -p nika-builtin --lib -q
cargo test -p nika-engine --lib -q
cargo clippy -p nika-builtin -p nika-engine -p nika-media -p nika-kernel --lib -- -D warnings

# Full workspace count:
cargo test --workspace --lib 2>&1 | grep "^test result" | awk '{s+=$4} END{print "Total:", s}'

# Find blocking I/O (should be zero in async code):
grep -rn "\.canonicalize()" tools/nika-media/src/ | grep -v "tokio::fs"
grep -rn "std::fs::" tools/nika-media/src/tools/ | grep -v "//\|test"

# Check AGPL headers on new files:
git diff --name-only HEAD | xargs grep -L "AGPL-3.0-or-later" 2>/dev/null

# Verify tool registration count after 12.12:
cargo test -p nika-engine --lib test_router_with_all_tools -q -- --nocapture 2>&1 | grep "has tool"

# Check for unwrap in new files:
git diff HEAD -- '*.rs' | grep "^+" | grep -E "\.unwrap\(\)|\.expect\(" | grep -v "//.*REASON"
```

---

## 9. PRE-SESSION CHECKLIST

Before writing ANY code, verify all boxes are checked:

- [ ] Baseline commands in §0.2 all pass
- [ ] Read `tools/nika-kernel/src/scope.rs` — confirm MediaContext is still the thin version
- [ ] Read `tools/nika-engine/src/runtime/builtin/media/mod.rs` — understand current adapter
- [ ] Read `tools/nika-media/src/tools/context.rs` — understand MediaToolContext fields
- [ ] Read `tools/nika-media/src/tools/import.rs` — see one MediaOp impl end-to-end
- [ ] Announced and used `spn-powers:test-driven-development` skill
- [ ] First test written and CONFIRMED FAILING before writing production code
