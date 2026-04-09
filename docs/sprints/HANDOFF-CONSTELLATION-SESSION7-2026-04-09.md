# Constellation Execution Handoff — SESSION 7

> **Self-contained handoff. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
>
> **Philosophy (non-negotiable):** `perfection > timing`. No "acceptable for launch", no "stretch", no "post-launch". Everything in scope, everything done properly. Launch date follows the work, not the other way around.

---

## 0. META — HOW TO USE THIS HANDOFF

### 0.1 Read order (MANDATORY)

```
1. This file (complete)
2. nika/CLAUDE.md                                          — project identity, 5 verbs, Shield
3. tools/nika/CLAUDE.md                                    — crate map, error codes, testing rules
4. tools/nika-engine/ARCHITECTURE.md                       — engine module map, invariants
5. tools/nika-builtin/CLAUDE.md                            — nika-builtin crate reference
6. docs/plans/2026-04-08-constellation-v2-mega-plan.md     — THE PLAN — sections 3, 5, 6, 7, 8, 9, 12
```

### 0.2 Baseline verification (FIRST commands to run)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
git log --oneline -3
# Expected HEAD: bcba44fec fix(builtin): P2 — aggregate sum null, ...

cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: ~10,860+ tests

cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: clean Finished
```

**If baseline is broken, STOP and investigate before touching anything.**

### 0.3 Skills you MUST use

| Skill | When |
|-------|------|
| `spn-powers:test-driven-development` | Before writing ANY implementation code |
| `spn-powers:verification-before-completion` | Before claiming done or committing |
| `spn-rust:rust-core` | When designing traits, error types, ownership |
| `spn-rust:rust-async-expert` | When wiring Arc<dyn Trait> or tokio::task_local |

---

## 1. WHERE WE ARE

### Phase 12 status: 27/63 tools in nika-builtin (43%)

**Done (nika-builtin, Sessions 6):**
- Core (5): sleep, log, emit, assert, complete
- Data (13): jq, map, filter, group_by, chunk, token_count, enrich, zip, set_diff, json_merge, json_diff, tree_data, inject
- Data Sprint 2 (6): json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten
- Introspection (3): cost, dag_info, threads

**Remaining in nika-engine (36):**
- File (5): read, write, edit, glob, grep — **SESSION 7 TARGET**
- Introspection (3): task_status, records, orchestrate — **DEFERRED** (see §3.1)
- Media (24): all — **SESSIONS 8-9**
- Core (2): run, prompt — **SESSION 7 TARGET**
- Fetch (1): nika:fetch — stays in nika-engine (SSRF logic too coupled)

### Code quality baseline (Session 7 starts here)

**7 critical bugs fixed in pre-Session 7 review (9 commits since S6 end):**
- P0: complete.rs schema required fields mismatch (OpenAI rejection)
- P0: is_completion_signal substring false positives
- P0: inject create_dir_all before path traversal check
- P0: jq unbounded iterator OOM (JQ_MAX_RESULTS = 100k)
- P1: set_diff key-order dependent comparison (→ PartialEq)
- P1: json_diff silently accepted non-objects
- P1: chunk_size=0 panic, empty marker corruption, yaml_validate duplicate labels
- P1: json_transform empty separator, sleep sub-ms truncation, threads case filter
- P1: BuiltinError::Denied → task_id "(builtin)" not empty string

---

## 2. ARCHITECTURAL CONTEXT (READ BEFORE CODING)

### 2.1 The KernelToolAdapter pattern (already established)

```
nika-builtin/src/X.rs:
  impl BuiltinTool for XTool { ... returns BuiltinError ... }

nika-engine/src/runtime/builtin/router.rs:
  tools.insert("x", Arc::new(KernelToolAdapter(XTool)));

nika-kernel/src/builtin.rs:
  pub trait BuiltinTool: __sealed::Sealed { ... returns BuiltinError }
  pub struct KernelToolAdapter<T>(pub T);
  // From<BuiltinError> for NikaError in nika-engine/src/error.rs
```

### 2.2 task_local! cells (CRITICAL for file tools + RunTool)

The 4 task_local! cells live in `nika-engine/src/runtime/builtin/run.rs`:

```rust
tokio::task_local! {
    pub(crate) static CURRENT_TASK_TRUST: TrustLevel;
    pub(crate) static CURRENT_TASK_ELEVATED: bool;
    pub(crate) static CURRENT_TASK_ID: Option<Arc<str>>;
    static WORKFLOW_DEPTH: Cell<u32>;
    static PARENT_CHAIN: Vec<PathBuf>;
}
```

**Problem:** File tools (ReadTool) call `check_path_readable()` which reads
`CURRENT_TASK_TRUST` and `CURRENT_TASK_ELEVATED` — both declared in nika-engine.
RunTool reads and writes all 5 cells.

If file tools or RunTool move to nika-builtin without changing where the cells
are declared, we either:
- (a) get a nika-builtin → nika-engine import cycle (won't compile), OR
- (b) lose the Shield path check (catastrophic security regression)

**Solution (commit 12.6-pre):** Move the cell declarations and accessors to
`nika-kernel/src/task_local.rs`. Both nika-engine (to SET them) and nika-builtin
(to READ them) can then import from nika-kernel.

### 2.3 Shield invariant (MUST NOT BREAK)

- Trust propagated via `task_local!` in `runner.rs` — **NEVER passed as a function argument** to `BuiltinTool::call`
- `check_path_readable()` in `nika-engine/src/tools/mod.rs` enforces untrusted agent path restrictions
- After task_locals move to nika-kernel, this function must be ported to `nika-builtin/src/file/shield.rs` so file tools can call it without importing nika-engine

---

## 3. SESSION 7 COMMIT PLAN (REORDERED)

> **CHANGE FROM S5 HANDOFF:** Session 7 was originally 12.5 → 12.6 → 12.7 → 12.8.
> Rust architectural analysis reveals 12.5 is DEFERRED (needs RecordView DTO),
> 12.6+12.7 need a prerequisite commit (12.6-pre), and 12.8 is the cleanest.
>
> New order: **12.8 → 12.6-pre → 12.6 → 12.7 → 12.5-deferred**

---

### Commit 12.8 — PromptTool (START HERE — no blockers)

**What:** Move `PromptTool` from `nika-engine/src/runtime/builtin/prompt.rs` to
`nika-builtin/src/prompt.rs`. Uses `HitlPrompt` trait from nika-kernel (already defined).

**Why first:** No task_local dependency, no ника-engine type deps, cleanest migration.

**Key files to read:**
- `nika-engine/src/runtime/builtin/prompt.rs` — source
- `nika-kernel/src/scope.rs` — HitlPrompt trait
- `nika-engine/src/runtime/hitl.rs` — HitlHandler (stays in engine)

**Implementation:**

1. New file `nika-builtin/src/prompt.rs`:

```rust
// PromptTool in nika-builtin:
pub struct PromptTool {
    handler: Option<Arc<dyn HitlPrompt>>,
}

impl PromptTool {
    pub fn new_headless() -> Self { Self { handler: None } }
    pub fn new(handler: Arc<dyn HitlPrompt>) -> Self { Self { handler: Some(handler) } }
}

impl BuiltinTool for PromptTool {
    fn call(&self, args: String) -> ... {
        // headless: use default or return error
        // with handler: delegate to HitlPrompt::ask()
    }
}
```

2. New bridge in `nika-engine/src/runtime/hitl_bridge.rs`:

```rust
// Adapts HitlHandler (engine) to HitlPrompt (kernel)
pub struct HitlBridge { inner: Arc<dyn HitlHandler> }

impl HitlPrompt for HitlBridge {
    async fn ask(&self, message: &str, default: Option<&str>) -> Result<String, BuiltinError> {
        self.inner.prompt(HitlRequest::new(message).with_default(...))
            .await.map(|r| r.response).map_err(|e| BuiltinError::Other { ... })
    }
}
```

3. Router change (`with_hitl()` builder method):

```rust
// headless by default:
tools.insert("prompt", Arc::new(KernelToolAdapter(PromptTool::new_headless())));

// TUI injects the HITL handler:
pub fn with_hitl(mut self, handler: Arc<dyn HitlHandler>) -> Self {
    self.tools.insert("prompt",
        Arc::new(KernelToolAdapter(PromptTool::new(Arc::new(HitlBridge::new(handler))))));
    self
}
```

**Watch for:** `default_used` field in response — once HitlBridge is involved,
always set `default_used: false` (bridge can't know if user input was used).

**Commit message:**
```
refactor(builtin): migrate PromptTool to nika-builtin via HitlPrompt bridge
```

---

### Commit 12.6-pre — Move task_locals to nika-kernel

**What:** Move `tokio::task_local!` declarations + accessor functions from
`nika-engine/src/runtime/builtin/run.rs` to `nika-kernel/src/task_local.rs`.

**Why:** File tools and RunTool need to read trust state, but can't import nika-engine.

**Implementation:**

1. New file `nika-kernel/src/task_local.rs`:

```rust
use nika_core::trust::TrustLevel;
use std::sync::Arc;
use std::path::PathBuf;
use std::cell::Cell;

tokio::task_local! {
    pub static CURRENT_TASK_TRUST: TrustLevel;
    pub static CURRENT_TASK_ELEVATED: bool;
    pub static CURRENT_TASK_ID: Option<Arc<str>>;
    pub static WORKFLOW_DEPTH: Cell<u32>;
    pub static PARENT_CHAIN: Vec<PathBuf>;
}

pub fn current_task_trust() -> TrustLevel {
    CURRENT_TASK_TRUST.try_with(|t| *t).unwrap_or(TrustLevel::Trusted)
}
pub fn current_task_elevated() -> bool {
    CURRENT_TASK_ELEVATED.try_with(|e| *e).unwrap_or(false)
}
pub fn current_task_id() -> Option<Arc<str>> {
    CURRENT_TASK_ID.try_with(|id| id.clone()).unwrap_or(None)
}
pub fn current_depth() -> u32 {
    WORKFLOW_DEPTH.try_with(|d| d.get()).unwrap_or(0)
}
pub fn current_parent_chain() -> Vec<PathBuf> {
    PARENT_CHAIN.try_with(|c| c.clone()).unwrap_or_default()
}
```

2. `nika-engine/src/runtime/builtin/run.rs` — remove `tokio::task_local!` block,
re-export from nika-kernel:

```rust
pub(crate) use nika_kernel::task_local::{
    CURRENT_TASK_TRUST, CURRENT_TASK_ELEVATED, CURRENT_TASK_ID,
    WORKFLOW_DEPTH, PARENT_CHAIN,
    current_task_trust, current_task_elevated, current_task_id,
    current_depth, current_parent_chain,
};
```

3. All call sites in nika-engine that used `crate::runtime::builtin::run::current_task_trust()`
now use `nika_kernel::task_local::current_task_trust()` (or via the re-export).

4. Add nika-kernel to nika-builtin's Cargo.toml (already a dep, confirm scope includes task_local).

**Test:** `cargo test --workspace --lib` — all tests must still pass.

**Commit message:**
```
refactor(kernel): move task_local declarations to nika-kernel/src/task_local.rs

Enables nika-builtin file tools and RunTool to read trust state without
importing nika-engine (which would create a cycle). nika-engine re-exports
via pub(crate) use nika_kernel::task_local::*.
```

---

### Commit 12.6 — File Tools Migration

**What:** Move 5 file tools from `nika-engine/src/tools/` to `nika-builtin/src/file/`.

**Files in nika-engine/src/tools/:**
- `read.rs`, `write.rs`, `edit.rs`, `glob.rs`, `grep.rs`
- `mod.rs` (contains `check_path_readable`)

**Key constraint:** `check_path_readable` reads task_locals (now in nika-kernel after 12.6-pre).
After 12.6-pre, this function CAN be ported to nika-builtin.

**Implementation:**

1. New file `nika-builtin/src/file/shield.rs`:

```rust
use nika_kernel::task_local::{current_task_trust, current_task_elevated, current_task_id};
use nika_kernel::builtin::BuiltinError;
use std::path::{Path, PathBuf};

// Copy SENSITIVE_FILE_NAMES, SENSITIVE_FILE_SUFFIXES, is_dotenv_family from nika-engine/src/tools/mod.rs
pub const SENSITIVE_FILE_NAMES: &[&str] = &[".mcp.json", "nika.toml"];
pub const SENSITIVE_FILE_SUFFIXES: &[&str] = &[".nika.yaml", ".nika.yml"];

pub fn check_path_readable(path: &Path) -> Result<(), BuiltinError> {
    let trust = current_task_trust();
    let elevated = current_task_elevated();
    if !trust.is_untrusted() || elevated { return Ok(()); }
    // ... rest of check logic (see nika-engine/src/tools/mod.rs)
}
```

2. New files `nika-builtin/src/file/`:
   - `read.rs`, `write.rs`, `edit.rs`, `glob.rs`, `grep.rs`, `mod.rs`, `shield.rs`

Each tool implements `BuiltinTool` (returns `BuiltinError`). `ReadTool::call()` calls
`file::shield::check_path_readable(&path)?` before any I/O.

3. Add `ignore` crate to nika-builtin Cargo.toml for GrepTool (check if needed).

4. `nika-engine/src/tools/mod.rs` keeps `check_path_readable` for remaining call sites
(test files, nika-engine internal use), but now delegates to nika-builtin's version
OR both versions share via nika-kernel. Simplest: keep both for now, delete engine copy in 12.13.

5. Router: file tools now via `KernelToolAdapter`, `with_file_tools()` creates nika-builtin file tools.

**Path validation (EditTool read-before-edit cache):**
EditTool requires a prior ReadTool call on the same path. Today this uses a shared
`DashMap<PathBuf, bool>` in ToolContext. After migration, this state lives in a new
`FileIoState` struct passed via `Arc<FileIoState>` at construction time.

**Working directory boundary:**
Today validated in `ToolContext::validate_path()`. After migration, validation logic
moves into each tool's `call()` using `current_working_dir()` (add to nika-kernel
task_local, or pass via FileToolContext).

**Commit message:**
```
refactor(builtin): migrate 5 file tools to nika-builtin with Shield integration
```

---

### Commit 12.7 — RunTool via EngineRunExecutor + RunSpec

**What:** Move `RunTool` from `nika-engine/src/runtime/builtin/run.rs` to `nika-builtin/src/run_tool.rs`.

**After 12.6-pre:** task_locals now in nika-kernel, so RunTool can READ them.
**Pattern:** RunTool reads trust state and constructs `RunSpec`, passes to `Arc<dyn RunExecutor>`.
`EngineRunExecutor` in nika-engine handles the actual parsing + execution.

**New trait in `nika-kernel/src/scope.rs`:**

```rust
#[derive(Debug)]
pub struct RunSpec {
    pub path: Option<PathBuf>,
    pub yaml_content: Option<String>,
    pub caller_trust: TrustLevel,
    pub parent_context: Option<serde_json::Value>,
    pub depth: u32,
    pub max_depth: u32,
    pub timeout: std::time::Duration,
    pub parent_chain: Vec<PathBuf>,
}

#[async_trait::async_trait]
pub trait RunExecutor: Send + Sync {
    async fn run_workflow(&self, spec: RunSpec) -> Result<serde_json::Value, BuiltinError>;
}
```

**nika-builtin/src/run_tool.rs:**

```rust
pub struct RunTool {
    executor: Arc<dyn RunExecutor>,
}

impl BuiltinTool for RunTool {
    fn call(&self, args: String) -> ... {
        // 1. Parse RunParams
        // 2. Read task_locals from nika-kernel (depth, trust, parent_chain)
        // 3. Shield: capability check (untrusted + not elevated → Denied)
        // 4. Cycle detection (path in parent_chain → NIKA-387)
        // 5. Depth check (depth >= max_depth → NIKA-386)
        // 6. Construct RunSpec
        // 7. self.executor.run_workflow(spec).await
    }
}
```

**nika-engine: new file `runtime/executor_impl/run_executor.rs`:**

```rust
pub struct EngineRunExecutor { /* holds Arc to config */ }

impl RunExecutor for EngineRunExecutor {
    async fn run_workflow(&self, spec: RunSpec) -> Result<Value, BuiltinError> {
        // WORKFLOW_DEPTH.scope(Cell::new(spec.depth + 1), async {
        //   PARENT_CHAIN.scope(updated_chain, async {
        //     parse + build runner + run
        //   })
        // })
    }
}
```

Router: `tools.insert("run", Arc::new(KernelToolAdapter(RunTool::new(Arc::new(EngineRunExecutor::new())))));`

**Commit message:**
```
refactor(builtin): migrate RunTool to nika-builtin via RunSpec + EngineRunExecutor
```

---

### Commit 12.5-deferred — Document introspection tool deferral

**What:** task_status, records, orchestrate CANNOT be migrated yet.

**Blocker:** These tools depend on `Record` struct from `nika-engine::runtime::record`
(has fields: `key_findings`, `compression_ratio()`, etc.). Moving them to nika-builtin
without promoting `Record` to nika-core would create a nika-builtin → nika-engine cycle.

**Resolution:** Define a lightweight `RecordView` DTO in nika-core (L0):
```rust
// nika-core/src/record.rs — future commit
pub struct RecordView {
    pub task_id: String,
    pub summary: String,
    pub key_findings: Vec<String>,
    pub confidence: f64,
    pub tokens_original: u64,
    pub tokens_compressed: u64,
}
```

Then add `impl From<Record> for RecordView` in nika-engine and define
`pub trait RecordQuery` in nika-kernel with methods `has_record`, `get_record`, `iter_records`.

**Action for this commit:** Update `router.rs` comment to say
"3 introspection tools deferred pending RecordView DTO in nika-core (12.5)".
No code change. One commit updating the comment.

---

## 4. TESTS REQUIRED FOR EACH COMMIT

| Commit | Tests to add |
|--------|-------------|
| 12.8 | PromptTool headless mode (no handler), headless with default, headless without default errors, bridge round-trip |
| 12.6-pre | All existing tests still pass (regression only), add task_local accessor smoke test |
| 12.6 | Each file tool: basic read/write/edit/glob/grep, path traversal blocked, Shield check on untrusted, edit-without-read errors |
| 12.7 | RunTool: depth limit (NIKA-386), cycle detection (NIKA-387), untrusted without elevation blocked, RunSpec correctly constructed |

---

## 5. SESSIONS 8-9 PREVIEW (Media tools + router migration)

### Commit 12.9 — MediaContext trait + Tier 1 (5 tools)

**Tier 1 (always-on):** import, decode, dimensions, thumbhash, dominant_color

`MediaContext` trait in `nika-kernel/src/media.rs` needs:
- `store_blob(data: &[u8], mime: &str) -> Result<BlobHash, BuiltinError>`
- `read_blob(hash: &str) -> Result<Vec<u8>, BuiltinError>`
- `blob_size(hash: &str) -> Result<u64, BuiltinError>`
- `detect_mime(data: &[u8]) -> Option<String>`

**Async hazard:** Image dimension detection (imagesize crate) is sync CPU work.
Wrap in `tokio::task::spawn_blocking` inside call() for Tier 1+ tools.

### Commit 12.12 — BuiltinToolRouter Bundle pattern

After all tools are in nika-builtin, the router constructor changes to:

```rust
// Instead of individual registrations, use a Bundle struct:
pub struct BuiltinBundle {
    pub run_executor: Arc<dyn RunExecutor>,
    pub hitl: Option<Arc<dyn HitlPrompt>>,
    pub media: Option<Arc<dyn MediaContext>>,
    pub file_io: Arc<FileIoState>,
}

impl BuiltinToolRouter {
    pub fn new(bundle: BuiltinBundle) -> Self { ... }
}
```

This makes it impossible to forget to inject a dependency — compiler catches it.

### Commit 12.13 — Cleanup

After all tools migrated:
- Delete `nika-engine/src/runtime/builtin/` files that were moved (keep `router.rs`, `trait.rs`)
- Delete `nika-engine/src/tools/` directory (file tools moved to nika-builtin)
- Run `cargo test --workspace --lib` — must reach ≥11,000 tests
- Verify `nika-engine` LOC reduction: ~28k LOC removed

---

## 6. INVARIANTS — NEVER BREAK THESE

1. **Trust via task_local only** — never pass TrustLevel as a function argument to `BuiltinTool::call`
2. **check_path_readable must survive** — Shield path check must work after file tools move
3. **1 test per behavior** — no superficial `!is_empty()` checks, validate types/values
4. **AGPL-3.0-or-later** on all new/modified files
5. **No unwrap in production code** — use `?` with `BuiltinError::*`
6. **commit co-author:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
7. **Commit message format:** `refactor(builtin): <verb> <what>`

---

## 7. KNOWN ISSUES / DEBT (DO NOT FIX THIS SESSION unless blocking)

- token_count model param ignored (always uses heuristic) — needs tiktoken dispatch post-launch
- locale_lookup returns Err for not-found (should be soft {found: false}) — API design decision needed
- `nika-engine/src/registry/` (~2870 LOC) still present — nuke in separate commit after S7
- aggregate.rs tree_data sub_group_by now uses extract_field (P2 fixed) but needs test for nested path
- introspection tools (task_status, records, orchestrate) deferred — RecordView DTO needed

---

## 8. QUICK REFERENCE

```bash
# Test the right crates
cargo test -p nika-builtin --lib -q
cargo test -p nika-engine --lib -q
cargo test --workspace --lib -q

# Clippy
cargo clippy -p nika-builtin -p nika-engine --lib -- -D warnings

# Count tests
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
```
