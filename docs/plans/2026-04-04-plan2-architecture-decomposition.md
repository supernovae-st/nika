# Plan 2: Architecture Decomposition

**Date**: 2026-04-04 | **Version**: v0.68.0 (feature freeze)
**Priority**: POST-LAUNCH — These are structural improvements for long-term velocity
**Source**: 7-agent mega audit (Rust Architect, Rust Pro, Code Explorer)

---

## Overview

| ID | Finding | File(s) | Effort | Impact |
|----|---------|---------|--------|--------|
| ARCH-1 | `runner.rs` 8,252 lines — split into modules | `runtime/runner.rs` | 4h | Maintainability |
| ARCH-2 | `NikaError` 103 variants — complete domain migration | `error.rs` + `error_domains.rs` | 6h | Extensibility |
| ARCH-3 | Extract `nika-provider` crate from nika-engine | `engine/src/provider/` | 8h | Compile time, modularity |
| ARCH-4 | Unify dual LSP implementations | `engine/src/lsp/` + `nika-lsp-core/` | 6h | Remove 12K LOC duplication |
| ARCH-5 | Decompose `RunContext` god object | `store/run_context.rs` | 4h | Testability |
| ARCH-6 | `pub` → `pub(crate)` on 46 runtime re-exports | `runtime/mod.rs` | 1h | API hygiene |
| ARCH-7 | Move `NikaConfig` out of nika-engine | `engine/src/config.rs` | 2h | Decoupling |
| ARCH-8 | Rename `engine/src/core/` → `catalog/` | `engine/src/core/` | 30m | Clarity |
| ARCH-9 | Fix `nika-media` → `nika-mcp` inversion | `media/src/processor.rs:10` | 30m | Clean layers |

**Total estimated**: ~32 hours (spread across multiple sprints)

---

## ARCH-1: Split `runner.rs` into Module Directory

### Problem

`runner.rs` is 8,252 lines with a single `impl Runner` block spanning lines 247-3541.
The `run()` method alone is ~2,000 lines. This makes navigation, code review, and
targeted testing extremely difficult.

### Existing Structure (logical sections in runner.rs)

```
Lines    1-160    : Imports + LockfileGuard RAII
Lines  160-246    : Runner struct definition
Lines  247-565    : Constructor methods (new, with_event_log, with_policy, builders)
Lines  567-750    : DAG query methods (get_ready_tasks, all_done, find_root_failure, write_trace)
Lines  750-1077   : Retry logic (get_retry_config, execute_with_retry, build_retry_prompt)
Lines 1078-1587   : Task execution core (execute_task_iteration — 5 verb dispatch)
Lines 1588-3541   : run() method — DAG loop, for_each, result collection, artifacts
Lines 3543-end    : Tests (~4,700 lines)
```

### Proposed Split

Keep `Runner` as a single struct, but split the impl across files:

```
tools/nika-engine/src/runtime/runner/
├── mod.rs              ← Runner struct + constructors (~400 lines)
├── builders.rs         ← Builder pattern methods (with_*, quiet, etc.) (~200 lines)
├── dag_query.rs        ← get_ready_tasks, all_done, find_root_failure (~200 lines)
├── retry.rs            ← get_retry_config, execute_with_retry, build_retry_prompt (~350 lines)
├── task_dispatch.rs    ← execute_task_iteration — 5 verb dispatch (~500 lines)
├── run.rs              ← run() method — the main DAG execution loop (~800 lines)
├── for_each.rs         ← for_each parallel loop handling (~600 lines)
├── result_collect.rs   ← Result collection, aggregation, final output (~400 lines)
├── artifacts.rs        ← Artifact processing integration (~100 lines)
├── trace.rs            ← Trace management, write_trace, verify_media (~100 lines)
└── tests/
    ├── mod.rs
    ├── test_constructors.rs
    ├── test_dag_query.rs
    ├── test_for_each.rs
    ├── test_events.rs
    └── test_exec.rs
```

### Step-by-step

#### Step 1: Convert `runner.rs` to `runner/` directory

```bash
mkdir -p tools/nika-engine/src/runtime/runner
mv tools/nika-engine/src/runtime/runner.rs tools/nika-engine/src/runtime/runner/mod.rs
```

#### Step 2: Extract builders

Move all `with_*` and builder methods to `builders.rs`:
- `quiet()` (line 391)
- `with_detail_level()` (line 400)
- `with_classic_renderer()` (line 411)
- `with_initial_context()` (line 444)
- `with_custom_endpoints()` (line 459)
- `with_permission_mode()` (line 470)
- `with_base_path()` (line 486)
- `with_project_root()` (line 494)
- `with_working_dir_mode()` (line 504)
- `with_cancel_token()` (line 509)

```rust
// runner/builders.rs
use super::Runner;

impl Runner {
    pub fn quiet(mut self) -> Self { ... }
    pub fn with_detail_level(mut self, detail: ...) -> Self { ... }
    // ...
}
```

#### Step 3: Extract DAG query methods

Move to `dag_query.rs`:
- `get_ready_tasks()` (line 567)
- `all_done()` (line 612)
- `get_pending_tasks()` (line 622)
- `find_root_failure()` (line 632)
- `get_final_output()` (line 649)

#### Step 4: Extract retry logic

Move to `retry.rs`:
- `get_retry_config()` (line 746)
- `execute_with_retry()` (line 802)
- `build_retry_prompt()` (line 984)

#### Step 5: Extract task dispatch

Move to `task_dispatch.rs`:
- `execute_task_iteration()` (line 1078) — the massive 500-line function that
  dispatches to infer/exec/fetch/invoke/agent

#### Step 6: Split `run()` method

The `run()` method (line 1588) is ~2,000 lines. Split into:

- `run.rs` — main `pub async fn run()` that calls helpers:
  - `for_each.rs` — for_each loop setup, iteration spawning, result collection
  - `result_collect.rs` — JoinSet drain, result aggregation, dependency chain tracking
  - `artifacts.rs` — `process_task_artifacts` and `write_artifact_manifest` calls

#### Step 7: Move tests to subdirectory

Move the ~4,700 lines of tests (lines 3543-8252) to `runner/tests/`:
- Group by feature area (constructors, DAG, for_each, events, exec)
- Each test file is `#[cfg(test)]`

#### Step 8: Update `runtime/mod.rs`

```rust
mod runner;
pub use runner::Runner;
```

No change needed — the module path stays the same.

### Verification

```bash
cargo test --workspace --lib -p nika-engine -- runner
# All existing runner tests must pass with zero changes to test code
```

### Risk Assessment

- **Zero public API change** — `Runner` struct and all methods keep same signatures
- **Git blame preserved** — use `git mv` for the initial move
- **Safe refactor** — only moving code between files, no logic changes

---

## ARCH-2: Complete NikaError Domain Migration

### Problem

`error.rs` has 103 variants in a flat enum. Three methods (`code()`, `is_recoverable()`,
`fix_suggestion()`) each have 103-arm match blocks. `error_domains.rs` has the right
architecture (4 domain enums) but only covers ~30 variants with manual `From` impls.

### Current State

```
error.rs          : 103 variants, 2,802 lines
error_domains.rs  : 4 domain enums (Provider, Dag, Execution, Binding), 250 lines
                    Manual From<SubEnum> for NikaError impls
```

### Target State

```
error.rs          : ~15 variants (one per domain + a few cross-cutting)
error_domains.rs  : 12 domain enums, each owning its code()/is_recoverable()/fix_suggestion()
```

### Domain Groups (from error_domains.rs header)

| Range | Domain | Enum | Variant Count |
|-------|--------|------|--------------|
| 001-009 | Workflow | `WorkflowError` | ~8 |
| 010-019 | Schema | `SchemaError` | ~10 |
| 020-029 | DAG | `DagError` | 3 (DONE) |
| 030-039 | Provider | `ProviderError` | 7 (DONE) |
| 040-049 | Binding/Template | `BindingError` | 3 (DONE) |
| 050-059 | Path/Security | `SecurityError` | ~8 |
| 060-069 | Output | `OutputError` | ~5 |
| 090-099 | Execution | `ExecutionError` | 6 (DONE) |
| 100-109 | MCP | `McpError` | ~8 |
| 110-119 | Agent | `AgentError` | ~8 |
| 200-219 | File/Builtin tools | `ToolError` | ~15 |
| 250-299 | Media | `MediaError` | ~10 |
| 300-319 | Structured output | `StructuredOutputError` | ~8 |

### Step-by-step

#### Phase 1: Add remaining domain enums (no migration yet)

Add to `error_domains.rs`:
- `SchemaError` (NIKA-010 through NIKA-019)
- `SecurityError` (NIKA-050 through NIKA-059)
- `McpError` (NIKA-100 through NIKA-109)
- `AgentError` (NIKA-110 through NIKA-119)
- `ToolError` (NIKA-200 through NIKA-219)
- `MediaError` (NIKA-250 through NIKA-299)
- `StructuredOutputError` (NIKA-300 through NIKA-319)
- `WorkflowError` (NIKA-001 through NIKA-009)
- `OutputError` (NIKA-060 through NIKA-069)

Each domain enum implements:
```rust
impl SchemaError {
    pub fn code(&self) -> &str { ... }
    pub fn is_recoverable(&self) -> bool { ... }
    pub fn fix_suggestion(&self) -> Option<String> { ... }
}
```

#### Phase 2: Make NikaError delegate to domain enums

```rust
pub enum NikaError {
    #[error(transparent)]
    Workflow(#[from] WorkflowError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error(transparent)]
    Dag(#[from] DagError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Binding(#[from] BindingError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    Execution(#[from] ExecutionError),
    #[error(transparent)]
    Mcp(#[from] McpError),
    #[error(transparent)]
    Agent(#[from] AgentError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error(transparent)]
    StructuredOutput(#[from] StructuredOutputError),

    // Cross-cutting (kept directly on NikaError)
    #[error("[NIKA-096] {0}")]
    Internal(String),
}

impl NikaError {
    pub fn code(&self) -> &str {
        match self {
            Self::Workflow(e) => e.code(),
            Self::Schema(e) => e.code(),
            Self::Dag(e) => e.code(),
            // ... delegates to each domain
            Self::Internal(_) => "NIKA-096",
        }
    }
}
```

#### Phase 3: Migrate call sites (incremental, per-module)

For each module in nika-engine, change:
```rust
// Before:
Err(NikaError::ProviderNotConfigured { provider: name.into() })

// After:
Err(ProviderError::NotConfigured { provider: name.into() }.into())
// or better:
Err(ProviderError::NotConfigured { provider: name.into() })?
```

**Module order** (least coupling first):
1. `provider/` → `ProviderError`
2. `dag/` → `DagError`
3. `binding/` → `BindingError`
4. `runtime/security.rs` → `SecurityError`
5. `ast/` → `SchemaError`
6. `runtime/executor/` → `ExecutionError`
7. `runtime/builtin/` → `ToolError`
8. `mcp/` → `McpError` (cross-crate: nika-mcp)
9. `runtime/rig_agent_loop.rs` → `AgentError`
10. `media/` → `MediaError` (cross-crate: nika-media)
11. `runtime/structured_output.rs` → `StructuredOutputError`

### Verification per phase

```bash
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
```

---

## ARCH-3: Extract `nika-provider` Crate

### Problem

`nika-engine/src/provider/` (8,907 lines) wraps rig-core, manages cost tracking,
and implements native inference. It has zero dependency on the rest of nika-engine
(no AST, no DAG, no runtime).

### Benefit

- nika-tui can use providers without pulling full engine
- SDK can embed just the provider layer
- Compile time reduction (parallel compilation)
- Feature flags become crate-level

### New Crate Structure

```
tools/nika-provider/
├── Cargo.toml
├── src/
│   ├── lib.rs           ← pub use exports
│   ├── rig/             ← RigProvider enum + per-provider clients
│   │   ├── mod.rs
│   │   ├── anthropic.rs
│   │   ├── openai.rs
│   │   ├── stream.rs
│   │   └── ...
│   ├── native/          ← GGUF/mistral.rs inference
│   ├── mock.rs          ← Mock provider for tests
│   ├── cost.rs          ← Cost tracking
│   ├── endpoints.rs     ← Custom endpoint resolution
│   └── error.rs         ← ProviderError (from error_domains.rs)
```

### Dependencies to resolve

The provider module currently uses:
- `crate::core::ProviderName` → move to nika-core or nika-provider
- `crate::config::NikaConfig` → pass config as parameter, not import
- `crate::error::NikaError` → use ProviderError directly
- `crate::event::EventLog` → pass as parameter

### Migration strategy

1. Create `nika-provider` crate with provider code
2. Re-export from nika-engine: `pub use nika_provider::*`
3. Update nika-tui to depend on nika-provider directly
4. Remove re-export from nika-engine (breaking internal change only)

---

## ARCH-4: Unify Dual LSP

### Problem

Two LSP codebases maintain parallel handler sets:
- `nika-engine/src/lsp/` — 11,923 lines (original, AST-based)
- `nika-lsp-core/` — 11,816 lines (newer, tree-sitter-based)

### Solution

Complete migration to `nika-lsp-core` and delete `nika-engine/src/lsp/`.

### Steps

1. Audit: List all LSP features in engine that are NOT in lsp-core
2. Port missing features to lsp-core
3. Update nika-lsp to use lsp-core exclusively
4. Remove `nika-engine/src/lsp/` (delete ~12K lines)
5. Remove `lsp` feature flag from nika-engine

### Verification

```bash
# Before: test both
cargo test --workspace --lib -p nika-lsp-core
cargo test --workspace --lib -p nika-engine --features lsp

# After: only lsp-core
cargo test --workspace --lib -p nika-lsp-core
# engine lsp feature should not exist
```

---

## ARCH-5: Decompose RunContext

### Problem

`RunContext` has 50 pub methods and 1,851 lines. It manages task results, media budgets,
for_each aggregation, workspace state, vault access, context files, inputs, and skills.

### Solution: Extract sub-services

```rust
pub struct RunContext {
    results: TaskResultStore,      // task outcomes (DashMap)
    media: MediaManager,           // media refs + budget
    context: ContextStore,         // files, skills, inputs
    workspace: WorkspaceConfig,    // root path, vault
    records: RecordStore,          // execution records
}
```

Each sub-service has its own focused API. RunContext delegates.

### Steps

1. Create `TaskResultStore` — `insert()`, `get()`, `status_of()`, `is_success()`, `iter_results()`
2. Create `MediaManager` — `set_media()`, `take_media()`, `media_budget()`
3. Create `ContextStore` — `set_context()`, `set_skills()`, `set_inputs()`, `resolve_*_path()`
4. Create `WorkspaceConfig` — `workspace_root()`, `vault_get_credential()`
5. Create `RecordStore` — `set_record()`, `get_record()`, `iter_records()`
6. Delegate from RunContext (keeps backward compat)

---

## ARCH-6: `pub` → `pub(crate)` on Runtime Re-exports

### Problem

`runtime/mod.rs` re-exports 46 items, many of which are internal implementation details.

### Items to make `pub(crate)`

```rust
// These should NOT be public API:
pub(crate) use security::{
    check_blocklist, check_shell_data_injection, check_shell_mode_blocklist,
    validate_command_string, validate_exec_command, validate_exec_command_with_shell,
};
pub(crate) use artifact_processor::{process_task_artifacts, ArtifactProcessResult};
pub(crate) use structured_output::{
    validate_structured_output, InferCallback, StructuredOutputEngine, StructuredOutputResult,
};
pub(crate) use limit_tracker::LimitTracker;
pub(crate) use submit_tool::DynamicSubmitTool;
pub(crate) use partial::{PartialCheckpoint, PartialResult, StopReason};
```

### Items to keep `pub`

```rust
// These are the real public API:
pub use runner::Runner;
pub use chat_workflow::{ChatMessage, ChatWorkflow, Role};
pub use boot::{BootContext, BootPhase, BootSequence, ...};
pub use executor::TaskExecutor;
pub use hitl::{HitlHandler, HitlRequest, HitlResponse, ...};
pub use resolver::{resolve_assets, ResolvedAssets, ...};
pub use skill_injector::SkillInjector;
```

### Verification

```bash
cargo check --workspace
# Fix any "private type in public interface" errors
# These reveal actual public API boundaries
```

---

## ARCH-7, ARCH-8, ARCH-9: Quick Architecture Fixes

### ARCH-7: Move NikaConfig

Move `engine/src/config.rs` to `nika-core/src/config.rs`. Update imports.
Remove plaintext ApiKeys struct — vault-only.

### ARCH-8: Rename `engine/src/core/` → `engine/src/catalog/`

```bash
mv tools/nika-engine/src/core tools/nika-engine/src/catalog
# Update mod.rs: pub mod catalog;
# Replace all `crate::core::` with `crate::catalog::` in engine
```

### ARCH-9: Move ContentBlock to nika-core

Move `nika_mcp::ContentBlock` to `nika_core::ContentBlock`.
Update nika-media import to use nika-core (already a dependency).

---

## Execution Order (Recommended)

```
Sprint 1 (Quick wins — 1 day):
├── ARCH-6  pub(crate) cleanup (1h)
├── ARCH-8  Rename core/ → catalog/ (30m)
├── ARCH-9  Move ContentBlock (30m)
└── ARCH-7  Move NikaConfig (2h)

Sprint 2 (runner.rs — 1 day):
└── ARCH-1  Split runner.rs into modules (4h)

Sprint 3 (NikaError — 2 days):
└── ARCH-2  Complete domain error migration (6h)

Sprint 4 (Extraction — 2 days):
├── ARCH-3  Extract nika-provider crate (8h)
└── ARCH-5  Decompose RunContext (4h)

Sprint 5 (LSP cleanup — 2 days):
└── ARCH-4  Unify LSP implementations (6h)
```

## Invariants (Must Hold After Each Sprint)

- [ ] `cargo test --workspace --lib` passes (9800+ tests)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo check --workspace` with all feature combinations
- [ ] No new `pub` items added to `runtime/mod.rs`
- [ ] No new `NikaError` variants — use domain enums only
- [ ] No new `unwrap()` in production code
