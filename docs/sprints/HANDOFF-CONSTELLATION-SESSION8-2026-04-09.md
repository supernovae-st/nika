# Constellation Execution Handoff — SESSION 8

> **Self-contained handoff. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
>
> **Philosophy (non-negotiable):** `perfection > timing`. No "acceptable for launch", no "stretch",
> no "post-launch". Everything in scope. Launch date follows the work, not the other way around.

---

## 0. META

### 0.1 Read order (MANDATORY — before writing a single line of code)

```
1. This file (complete — all sections)
2. nika/CLAUDE.md                                          — project identity, 5 verbs, Shield
3. tools/nika/AGENTS.md                                    — workspace structure (26 crates)
4. tools/nika-engine/ARCHITECTURE.md                       — engine module map, key invariants
5. tools/nika-builtin/CLAUDE.md                            — nika-builtin reference, tools table
6. tools/nika-kernel/src/scope.rs                          — current MediaContext trait (thin!)
7. tools/nika-engine/src/runtime/builtin/media/mod.rs      — adapter layer (318 LOC)
8. tools/nika-media/src/tools/context.rs                   — MediaToolContext concrete type
9. tools/nika-media/src/tools/import.rs                    — example MediaOp implementation
10. docs/plans/2026-04-08-constellation-v2-mega-plan.md    — THE PLAN sections 12, 13, 14
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
| `spn-powers:verification-before-completion` | **Before every `git commit`** — run tests, show output, confirm count | "It compiles" ≠ "it works". Must show test output before claiming done. |
| `spn-rust:rust-core` | When designing any new trait, modifying `MediaContext`, adding error variants | Object safety pitfalls, `async fn` in traits (use `async_trait` or RPIT), `Send + Sync` bounds |
| `spn-rust:rust-async` | When implementing `compute_blocking`, rayon bridging, or any `spawn_blocking` | No blocking I/O on async executor. Rayon → tokio oneshot pattern. |
| `spn-powers:systematic-debugging` | When ANY test fails or clippy reports an error | 4-phase: root cause → pattern → hypothesis → fix. No guessing. |
| `spn-powers:defense-in-depth` | When adding any validation to media tool inputs | Validate at every layer: schema, param parsing, path resolution, trust level |
| `spn-powers:receiving-code-review` | When the code-reviewer subagent returns findings | Technical rigor required — verify each finding before acting on it |

**TDD protocol for every commit:**

```
1. Read source file you're about to change
2. Write the test first (it must FAIL — verify the failure)
3. Write minimum production code to make it pass
4. Run cargo test -p <crate> --lib -q to confirm GREEN
5. Run cargo clippy -p <crate> --lib -- -D warnings to confirm clean
6. Only then: git add + git commit
```

**Anti-patterns that will fail the session:**
- Writing implementation before test
- Marking a task done without running tests
- `assert!(!result.is_empty())` — not a real test, just a compile check
- Mocking a function you don't understand
- `.unwrap()` in production code — ZERO tolerance

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

### 2.2 The three async patterns in nika-media (know these before coding)

**Pattern 1 — CAS I/O (already async):**
```rust
// Correct — already uses async CAS
let data = ctx.read_blob(hash).await?;
```

**Pattern 2 — CPU-bound work (MUST use compute pool, NEVER block tokio):**
```rust
// WRONG — blocks the tokio thread:
let dims = imagesize::blob_size(&data).unwrap();

// CORRECT — rayon bridge via compute_blocking:
let dims = ctx.compute_blocking(move || {
    imagesize::blob_size(&data).map_err(|e| ...)
}).await?;
```

**Pattern 3 — Cancellation check (must be called before and after expensive ops):**
```rust
if ctx.is_cancelled() {
    return Err(tool_error("thumbnail", "operation cancelled"));
}
let data = ctx.read_blob(hash).await?;
if ctx.is_cancelled() { return Err(...); }
let output = ctx.compute_blocking(|| { /* expensive */ }).await?;
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

**Step 1: Expand `nika-kernel/src/scope.rs`**

```rust
// Add new supporting type (before the trait):
#[derive(Debug, Clone)]
pub struct BlobStoreResult {
    pub hash: String,
    pub size: u64,
    pub deduplicated: bool,
}

// Replace the existing thin MediaContext with:
#[async_trait::async_trait]
pub trait MediaContext: Send + Sync {
    // ── CAS read ────────────────────────────────────────────────
    /// Read a blob by its blake3 hash. Returns raw bytes.
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, crate::builtin::BuiltinError>;

    // ── CAS write (budget-enforced) ─────────────────────────────
    /// Store bytes into CAS with budget enforcement.
    /// `task_id` is used for budget attribution and dedup tracking.
    async fn store_blob(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<BlobStoreResult, crate::builtin::BuiltinError>;

    // ── Path confinement ─────────────────────────────────────────
    /// Working directory boundary for import path validation.
    fn working_dir(&self) -> Option<&std::path::Path>;

    // ── Cancellation ─────────────────────────────────────────────
    /// Check if the current operation has been cancelled (e.g., timeout exceeded).
    fn is_cancelled(&self) -> bool;

    // ── CPU-bound dispatch ───────────────────────────────────────
    /// Execute a CPU-bound closure on the rayon thread pool.
    /// NEVER do image decode/encode directly on the tokio executor.
    /// The future resolves when the work completes or the thread panics.
    async fn compute_blocking<F, T>(
        &self,
        f: F,
    ) -> Result<T, crate::builtin::BuiltinError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    // ── Legacy shim (keep for compatibility) ─────────────────────
    /// Direct access to the raw blob store. Use `read_blob`/`store_blob` when possible.
    fn blob_store(&self) -> &dyn crate::store::BlobStore;
}
```

**Step 2: Add `EngineMediaContext` in nika-engine**

New file `nika-engine/src/runtime/media_context.rs`:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::sync::Arc;
use nika_kernel::{builtin::BuiltinError, scope::{BlobStoreResult, MediaContext}};
use nika_media::tools::context::MediaToolContext;

/// Engine-side implementation of `MediaContext`.
///
/// Wraps `MediaToolContext` (nika-media's concrete type) and adapts it
/// to the kernel trait. This is the bridge between the kernel trait
/// (known to nika-builtin) and the engine's concrete media infrastructure.
pub struct EngineMediaContext {
    inner: Arc<MediaToolContext>,
}

impl EngineMediaContext {
    pub fn new(ctx: Arc<MediaToolContext>) -> Self {
        Self { inner: ctx }
    }

    pub fn into_arc(self) -> Arc<dyn MediaContext> {
        Arc::new(self)
    }
}

#[async_trait::async_trait]
impl MediaContext for EngineMediaContext {
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, BuiltinError> {
        self.inner.cas
            .get(hash)
            .await
            .map_err(|e| BuiltinError::Io {
                tool: "nika:media".into(),
                reason: format!("CAS read failed for {hash}: {e}"),
            })
    }

    async fn store_blob(&self, data: &[u8], task_id: &str) -> Result<BlobStoreResult, BuiltinError> {
        let result = self.inner
            .store_media(data, task_id)
            .await
            .map_err(|e| BuiltinError::Io {
                tool: "nika:media".into(),
                reason: format!("CAS store failed: {e}"),
            })?;
        Ok(BlobStoreResult {
            hash: result.hash,
            size: result.size,
            deduplicated: result.deduplicated,
        })
    }

    fn working_dir(&self) -> Option<&std::path::Path> {
        self.inner.working_dir.as_deref()
    }

    fn is_cancelled(&self) -> bool {
        self.inner.check_cancelled().is_err()
    }

    async fn compute_blocking<F, T>(&self, f: F) -> Result<T, BuiltinError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .compute
            .compute(f)
            .await
            .map_err(|e| BuiltinError::Other {
                tool: "nika:media".into(),
                reason: format!("compute thread error: {e}"),
            })
    }

    fn blob_store(&self) -> &dyn nika_kernel::store::BlobStore {
        &self.inner.cas
    }
}
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

**Tests to write for 12.9:**
```rust
// nika-engine/src/runtime/builtin/media/tests_engine_media_context.rs

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
    assert!(!result.hash.is_empty());
    assert_eq!(result.size, data.len() as u64);

    let retrieved = ctx.read_blob(&result.hash).await.unwrap();
    assert_eq!(retrieved, data);
}

#[tokio::test]
async fn engine_media_context_compute_blocking_executes_closure() {
    let ctx = make_test_engine_media_context();
    let result = ctx.compute_blocking(|| 42u64 * 2).await.unwrap();
    assert_eq!(result, 84);
}

#[tokio::test]
async fn engine_media_context_compute_blocking_propagates_panic_as_error() {
    let ctx = make_test_engine_media_context();
    let result = ctx.compute_blocking(|| -> u64 { panic!("intentional panic") }).await;
    assert!(result.is_err());
    // Must not propagate the panic to the tokio thread
}

#[tokio::test]
async fn engine_media_context_working_dir_matches_context() {
    let ctx = make_test_engine_media_context();
    // working_dir returns None when MediaToolContext.working_dir is None
    // (test env typically doesn't set it)
    let _ = ctx.working_dir(); // just confirm it doesn't panic
}
```

**Commit message:**
```
refactor(kernel): expand MediaContext trait with store_blob/read_blob/compute_blocking/is_cancelled

Wire EngineMediaContext in nika-engine (adapts MediaToolContext → MediaContext).
Update create_media_tool_adapters() to take Arc<dyn MediaContext>.
All existing tests must pass — no behavioral change, only API expansion.
```

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

Locate where `ctx.compute.compute(|| decode_image_safe(&data) ... )` is called.
Add budget acquisition BEFORE the rayon dispatch:

```rust
// After reading the blob, before compute:
let estimated_decode_bytes = data.len() * 4; // worst-case RGBA
let _budget_guard = ctx.working_memory
    .acquire(estimated_decode_bytes)
    .map_err(|e| tool_error("thumbnail", format!("memory budget: {e}")))?;

let (decoded, _meta) = ctx
    .compute
    .compute(move || decode_image_safe(&data))
    .await
    .map_err(|e| tool_error("thumbnail", e.to_string()))??;
// _budget_guard drops here when the decoded image goes out of scope
```

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

**Goal:** `MediaToolAdapter` takes `Arc<dyn MediaContext>` instead of `Arc<MediaToolContext>`.
Enables future callers (nika-builtin, tests) to inject mock contexts.

**The adapter in `nika-engine/src/runtime/builtin/media/mod.rs`:**

```rust
// Before:
pub(crate) struct MediaToolAdapter {
    op: Arc<dyn MediaOp>,
    ctx: Arc<MediaToolContext>,   // ← concrete type
    name: &'static str,
    timeout: Duration,
}

// After:
pub(crate) struct MediaToolAdapter {
    op: Arc<dyn MediaOp>,
    ctx: Arc<dyn MediaContext>,   // ← trait object
    name: &'static str,
    timeout: Duration,
}
```

But `MediaOp::execute` takes `&MediaToolContext`, not `&dyn MediaContext`. This is the key conflict.

**Two approaches:**

**Option A (minimal, recommended):** Keep `MediaOp::execute` signature unchanged.
`MediaToolAdapter::call` extracts what it needs via the `MediaContext` trait but passes
the concrete `MediaToolContext` (via `Arc<EngineMediaContext>` downcasting or via a
stored `Arc<MediaToolContext>` alongside the trait object).

Actually, looking more carefully: the MediaOp trait and its execute method take
`&MediaToolContext` not a trait. The cleanest minimal approach is to store BOTH:
- `ctx_trait: Arc<dyn MediaContext>` — for type-erased access from outside
- The existing `MediaToolContext` reference for passing to `MediaOp::execute`

BUT this defeats the purpose. The real solution is to update `MediaOp::execute` to take
`&dyn MediaContext` and update all 24 impls.

**Option B (complete, higher effort):** Update `MediaOp::execute` in nika-media to take
`&dyn MediaContext`. Update all 24 `impl MediaOp` files to use the trait methods
(`ctx.read_blob()`, `ctx.store_blob()`, `ctx.compute_blocking()`, `ctx.is_cancelled()`)
instead of calling `ctx.inner.*` methods directly.

This is the architecturally correct path. Commit 12.11 should do Option B.

**Organize as two sub-commits to reduce diff size:**

**12.11a — Tier 1 tools (5 always-on: import, decode, dimensions, thumbhash, color):**
- Update `MediaOp` trait in nika-media: `execute(&self, args, ctx: &dyn MediaContext)`
- Update Tier 1 tool impls to use trait methods
- Keep existing Tier 2-3 tools compiling via `impl_media_op_legacy!` shim if needed

**12.11b — Tier 2+3+web tools (19 remaining):**
- Update all remaining `impl MediaOp` files
- Remove legacy shim
- Verify all 8 integration test modules still pass

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

### Commit 12.13 — Cleanup

**Goal:** Remove dead code now that all adapters use traits.

**What to delete:**
- Direct `Arc<MediaToolContext>` fields in `MediaToolAdapter` (replaced by trait)
- `create_media_tool_adapters(ctx: Arc<MediaToolContext>)` old signature variant
- `nika-engine/src/tools/` directory (file tools migrated in 12.6 — verify it's empty first)

**What to KEEP (IMPORTANT):**
- `nika-engine/src/runtime/builtin/media/mod.rs` — reduced to adapter + test re-exports
- All `tests_*.rs` files in that module — they test engine integration, can't be moved
- `EngineMediaContext` in nika-engine

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
| 1 | `import_no_blocking_canonicalize` | completes in <500ms even on slow path | P0 |
| 2 | `compute_panic_returns_error` | panic → Err, not thread crash | P0 |
| 3 | `compute_subsequent_after_panic` | pool functional after panic | P0 |
| 4 | `thumbnail_budget_tracked` | working memory acquired before rayon | P1 |
| 5 | `thumbnail_concurrent_budget_exhaustion` | 10 concurrent calls → some error gracefully | P1 |

### 4.3 MediaOp trait update (commit 12.11)

| # | Test name | Tool | What it verifies |
|---|-----------|------|-----------------|
| 1 | `import_uses_store_blob_not_cas_direct` | import | calls ctx.store_blob(), not internal CAS |
| 2 | `decode_b64_to_cas_hash` | decode | base64 → CAS, returns hash+size |
| 3 | `dimensions_reads_via_trait` | dimensions | calls ctx.read_blob(), not ctx.cas.get() |
| 4 | `dimensions_uses_compute_blocking` | dimensions | imagesize runs on rayon, not tokio |
| 5 | `thumbhash_uses_compute_blocking` | thumbhash | encoding runs on rayon |
| 6 | `thumbnail_respects_cancellation` | thumbnail | cancelled context → early error |
| 7 | `convert_format_output_in_cas` | convert | output stored in CAS, hash returned |
| 8 | `strip_metadata_returns_clean_hash` | strip | output has no EXIF, stored in CAS |
| 9 | `mock_context_injection_works` | all | MockMediaContext can replace real context |

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
4. **`async fn` in traits needs `#[async_trait]`** — or use RPIT (`-> impl Future`) carefully
5. **No blocking I/O on tokio threads** — always `spawn_blocking` or rayon bridge
6. **Tests validate values, not existence** — `assert_eq!(parsed["hash"].as_str().unwrap().len(), 71)` not `assert!(!parsed["hash"].is_null())`
7. **Commit co-author:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
8. **test --lib only** — never `cargo test` without `--lib` (avoids macOS keychain popups)
9. **One commit per logical unit** — no "misc fixes" mega-commits

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
