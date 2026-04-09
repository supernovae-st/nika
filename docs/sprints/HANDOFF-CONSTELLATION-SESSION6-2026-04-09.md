# Constellation Execution Handoff — SESSION 6

> **This is a self-contained handoff. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
>
> **Philosophy (non-negotiable):** `perfection > timing`. No "acceptable for launch", no "stretch", no "post-launch". Everything in scope, everything done properly.

---

## 0. META — HOW TO USE THIS HANDOFF

### 0.1 Read order (MANDATORY)

```
1. This file (complete)
2. nika/CLAUDE.md                                          — project identity, 5 verbs, Shield
3. tools/nika/CLAUDE.md                                    — crate map, error codes, testing rules
4. tools/nika-engine/ARCHITECTURE.md                       — engine module map, invariants
5. docs/plans/2026-04-08-constellation-v2-mega-plan.md     — THE PLAN — read sections 3, 5, 6, 7, 8, 9, 12, 17
6. docs/sprints/HANDOFF-CONSTELLATION-SESSION5-2026-04-08.md  — previous session (Phase 15 plan + Phase 12 full plan)
```

### 0.2 Baseline verification (FIRST commands to run)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
git status                                  # clean tree expected
git log --oneline -5                        # should show 5a74f644d at top (nika-macros builtin_tool)
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10833
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: clean Finished
```

**If baseline is broken, STOP and investigate before touching anything.**

### 0.3 Skills you MUST use

| Skill | When | Why |
|-------|------|-----|
| `spn-powers:test-driven-development` | Before writing implementation code | RED-GREEN-REFACTOR |
| `spn-powers:verification-before-completion` | Before claiming "done" | Evidence before assertions |
| `spn-powers:systematic-debugging` | On any compile/test failure | Root cause, not guessing |
| `spn-rust:rust-core` | When designing traits, error types | Senior-level Rust patterns |
| `spn-rust:rust-async-expert` | When wiring Arc<dyn EventEmitter> | Send+Sync correctness |
| `spn-rust:rust-architect` | Before creating nika-builtin crate | Sealed trait, cycle breaking |

### 0.4 Agents you MUST delegate to

| Agent | When | What to ask |
|-------|------|-------------|
| `spn-rust:rust-architect` | Before Phase 12 commit 1 | Trait boundary: sealed BuiltinTool, BuiltinError placement, RunTool cycle breaking |
| `spn-rust:rust-core` | Before committing any new trait | Object safety, Send+Sync bounds, ergonomics review |
| `spn-rust:rust-async-expert` | Before wiring Arc<dyn EventEmitter> | No locks across .await, no blocking I/O |
| `feature-dev:code-reviewer` | After each commit | Diff review with specific context |

---

## 1. SITUATION

- **Version:** v0.79.0 (no bump yet)
- **Branch:** main
- **Last commit:** `5a74f644d` feat(macros): add #[builtin_tool] attribute macro
- **Tests:** **10,833 passed, 0 failed**
- **Clippy:** Zero warnings
- **Crates:** **25** workspace members (was 24 before Phase 3)
- **Launch target:** May 5, 2026
- **Codename:** Constellation v2.1
- **Working directory:** `tools/nika/` (within `/Users/thibaut/dev/supernovae/nika/`)

---

## 2. WHAT CAME BEFORE — CUMULATIVE CONTEXT

### 2.1 Phase progression (all sessions)

| Phase | Title | Session | Commits | Status |
|-------|-------|---------|---------|--------|
| S1 bugs | ARM64 linker, dead MPSC, param redaction | S1 | 4 | ✅ |
| Quick wins | `#[must_use]`, FxHashSet, OnceLock | S1 | 4 | ✅ |
| Pre-0 | ARCHITECTURE.md | S1 | 2 | ✅ |
| 1 | `nika-kernel` crate (10 traits, L0.5) | S2 | 1 | ✅ |
| 2 | `nika-kernel-mock` (5 mocks, 23 tests) | S2 | 1 | ✅ |
| **3** | **`nika-macros` crate (3 derives, 1 attr macro)** | **S5** | **7** | **✅ NEW** |
| 4 partial | rstest pilot on transform.rs | S3 | 1 | ✅ |
| 5.1 | `EventEmitter` blanket impl for `Arc<T>` | S3 | 1 | ✅ |
| 8a | transform.rs split (5570→5 files) | S3 | 1 | ✅ |
| 8b | template.rs split (4938→2 files) | S3 | 1 | ✅ |
| 9+10 | 5 L1 effect crates (75 tests) | S3 | 2 | ✅ |
| 11 | Provider trait bridge + `get_dyn_provider` | S4 | 1 | ✅ |
| **15** | **main.rs → nika-cli (5530→2043 LOC)** | **S5** | **8** | **✅ NEW** |
| 16 partial | analyze.rs split (5531→6 files) | S3 | 1 | ✅ |

### 2.2 Current layering (25 crates)

```
L0    nika-core (23k)         AST, types, catalogs, trust, capabilities, policy — zero I/O
L0.5  nika-kernel (717)       10 trait defs — zero impls
      nika-kernel-mock (744)  5 hand-written mocks — dev-dep
      nika-macros (554)       3 derives + 1 attr macro (NikaErrorCode, EventTaskId, builtin_tool)
L1    nika-clock, nika-fs, nika-blob, nika-http, nika-exec-runner  — 5 L1 effect crates
      nika-event (4.5k)       EventLog + EventEmitter blanket impl
      nika-lsp-core (12k)     LSP intelligence (pure functions)
L2    nika-engine (160k)      MONOLITH — providers, builtins, runtime, http, exec
      + kernel_bridge.rs      impl Provider for RigProvider (S4)
      + get_dyn_provider()    Arc<dyn Provider> keystone (S4)
      nika-display (13k), nika-media (14k), nika-mcp (9k)
      nika-daemon (7k), nika-storage (1k), nika-vault (1.2k)
L3    nika-cli (12.5k)        CLI handlers — expanded in S5 (+9 new modules)
      nika-tui (88k), nika-serve (4k), nika-lsp (2.5k), nika-sdk (3k), nika-init (21k)
L5    nika (2k)               Binary entry point — 2,043 LOC (was 5,530)
```

### 2.3 Phase 15 recap (Session 5 — what just happened)

**main.rs decomposed from 5,530 to 2,043 LOC** (63% reduction). 9 new modules:

| Module | LOC | Functions |
|--------|-----|-----------|
| `check.rs` | 1,072 | validate_workflow, validate_schema_file, validate_workflow_strict |
| `eval.rs` | +130 | eval_workflow (appended to existing) |
| `run.rs` | 621 | run_workflow, dry_run_workflow |
| `bench.rs` | 520 | run_bench, evaluate_quality, aggregate_bench_stats, percentile |
| `inputs.rs` | 350 | parse_input_value, parse_cli_inputs, load_input_file, simple_input_resolve |
| `test_cmd.rs` | 334 | test_workflow, normalize_golden, compare_golden |
| `discover.rs` | 298 | resolve_workflow_path, download_remote_workflow, count_nika_workflows, etc. |
| `demo.rs` | 194 | run_demo, print_agent_presets |
| `explain.rs` | 133 | explain_workflow |
| `task_filter.rs` | 114 | filter_tasks_for_target, filter_tasks_from |

**What remains in main.rs (irreducible):**
- `Cli` struct + `Commands` enum (~800 LOC) — clap derives, binary-specific
- `async fn main()` match dispatch (~600 LOC) — TUI arms use `nika::tui::*`
- `print_features/count_features` (~190 LOC) — `cfg!(feature = "tui")` binary-only
- `print_env_info` + `long_version` (~90 LOC) — build.rs env vars
- Dispatch helpers (~100 LOC) — `handle_result` (miette), `is_tui_mode` (Commands)

**Key pattern:** `nika::*` → `nika_engine::*` for all code in nika-cli. TUI-related match arms stay in main.rs (nika::tui only exists via binary re-export).

### 2.4 Phase 3 recap (nika-macros — also Session 5)

A parallel session created `nika-macros` (L0.5, 554 LOC):

| Macro | Type | Saves | Used by |
|-------|------|-------|---------|
| `#[derive(NikaErrorCode)]` | derive | ~110 LOC | error.rs code() methods |
| `#[derive(EventTaskId)]` | derive | ~110 LOC | event task_id() methods |
| `#[builtin_tool]` | attribute | ~8,800 LOC projected | BuiltinTool impls (Phase 12) |
| `dispatch_helpers!` | - | refactored 22 transforms | transform dispatch |

### 2.5 Known god files remaining

| File | LOC | Target phase |
|------|-----|--------------|
| `nika-engine/src/error.rs` | 2,874 | Phase 6 |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Phase 14+ |
| `nika-engine/src/binding/resolve.rs` | 3,948 | Post-launch |

---

## 3. THE PHASE — Phase 12: nika-builtin extraction

### 3.0 Goal

Extract all **63 builtin tools** + sealed `BuiltinTool` trait from nika-engine into a dedicated L2 crate `nika-builtin`.

```
Before:
  nika-engine/src/runtime/builtin/     ~31,000 LOC (with tests)
  
After:
  nika-builtin/                        ~28,000 LOC
  nika-engine/src/runtime/builtin/     ~3,000 LOC (glue only: router + engine bridges)
```

### 3.1 Full tool inventory (63 tools)

| Tier | Count | Tools |
|------|-------|-------|
| Core | 7 | sleep, log, emit, assert, prompt, run, complete |
| Data | 13 | jq, tree_data, inject, map, filter, group_by, enrich, json_merge, json_diff, set_diff, zip, chunk, token_count |
| Data Sprint 2 | 6 | json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten |
| File | 5 | read, write, edit, glob, grep |
| Introspection | 6 | cost, records, dag_info, task_status, threads, orchestrate |
| Media T1 (always-on) | 5 | import, decode, dimensions, thumbhash, dominant_color |
| Media T2 (core) | 6 | thumbnail, convert, strip, metadata, optimize, svg_render |
| Media T3 (opt-in) | 13 | phash, compare, pdf_extract, chart, provenance, verify, qr_validate, quality, html_to_md, css_select, extract_metadata, extract_links, readability, pipeline |
| Adapters | 2 | rig_adapter (NikaBuiltinToolAdapter), file_adapter |

### 3.2 Blocker analysis (full resolution paths)

| ID | Blocker | Resolution |
|----|---------|------------|
| **B1** | Every builtin uses `NikaError` | **Define `BuiltinError` in nika-kernel** with variants: InvalidArgs, Io, Parse, Timeout, Schema, Denied, Other. Add `impl From<BuiltinError> for NikaError` in nika-engine. |
| **B2** | `run.rs` depends on `Runner` | **Define `RunExecutor` trait in nika-kernel**: `async fn run_workflow(path, inputs, depth) -> Result<Value, BuiltinError>`. nika-engine provides concrete impl via newtype. Cycle broken. |
| **B3** | `records.rs`, `router.rs` take `Arc<RunContext>` | **Use splinter traits already in nika-kernel**: `TaskResults`, `RecordStore`, `BindingScope`, etc. Each tool consumes only what it needs. |
| **B4** | `rig_adapter.rs`, cost take owned `EventLog` | **Use `Arc<dyn EventEmitter>`** — blanket impl shipped in S3. |
| **B5** | Media tools take `Arc<MediaToolContext>` | **Define `MediaContext` trait in nika-kernel**: `fn cas() -> &dyn BlobStore`, `fn compute_pool()`, `fn budget()`. |
| **B6** | File tools use `ToolContext` | **Rewrite to use `Arc<dyn Filesystem>`** from nika-fs. Move `ToolContext` to nika-builtin. |
| **B7** | `prompt.rs` depends on `HitlHandler` | **Define `HitlPrompt` trait in nika-kernel**. |
| **B8** | `data/transform.rs` uses `TransformExpr` | No blocker — already in nika-core::binding::transform. |
| **B9** | `rig_adapter.rs` hardcodes orchestrate constant | Define in nika-core::catalogs::builtins. |
| **B10-B12** | Various import path fixes | Direct deps, not blockers. |

**Every blocker has a resolution. Nothing skipped.**

### 3.3 Commit plan — 13 commits across 4 sessions

#### SESSION 6 — Foundation (4 commits)

**Commit 12.1 — nika-kernel additions**
- `nika-kernel/src/builtin.rs`: `BuiltinError` enum
- `nika-kernel/src/scope.rs`: `RunExecutor`, `HitlPrompt`, `MediaContext` traits
- `impl From<BuiltinError> for NikaError` in nika-engine
- **TDD:** BuiltinError::Display, From roundtrip, trait object-safety
- **Agent:** spawn `spn-rust:rust-architect` BEFORE writing traits

**Commit 12.2 — Create nika-builtin crate skeleton**
- `tools/nika-builtin/Cargo.toml` with deps
- Sealed `BuiltinTool` trait (private-mod pattern)
- Add to workspace members
- **TDD:** `assert_send_sync::<Arc<dyn BuiltinTool>>()`

**Commit 12.3 — Move 5 pure core tools (sleep, log, emit, assert, complete)**
- Zero coupling beyond BuiltinError
- Each tool → pub mod in nika-builtin
- **Verify:** test count unchanged

**Commit 12.4 — Move 13 data tools (entire data/ directory)**
- jq, json_diff, merge, set_diff, zip, map, filter, group_by, chunk, token_count, enrich, inject, tree_data
- Fix import: `crate::binding::TransformExpr` → `nika_core::binding::transform::TransformExpr`

#### SESSION 7 — Sprint 2 + Introspection + File (4 commits)

**Commit 12.5** — 6 Sprint 2 data tools
**Commit 12.6** — 6 introspection tools (use Arc<dyn EventEmitter> + Arc<dyn RecordStore>)
**Commit 12.7** — 5 file tools (rewrite to Arc<dyn Filesystem>)
**Commit 12.8** — RunTool: Arc<dyn RunExecutor> impl in nika-engine breaks cycle

#### SESSION 8 — Media (3 commits)

**Commit 12.9** — MediaContext trait + 5 Tier 1 tools
**Commit 12.10** — 6 Tier 2 media tools
**Commit 12.11** — 13 Tier 3 media tools + rig_adapter + file_adapter

#### SESSION 9 — Router + cleanup (2 commits)

**Commit 12.12** — Migrate BuiltinToolRouter to nika-builtin
**Commit 12.13** — Cleanup + re-exports, verify engine shrunk by ~28k LOC

### 3.4 #[builtin_tool] macro (Phase 3 shipped)

The `#[builtin_tool]` attribute macro from nika-macros generates the `BuiltinTool` impl automatically:

```rust
#[builtin_tool(name = "sleep", category = "core")]
pub struct SleepTool;

impl SleepTool {
    pub async fn execute(&self, params: Value) -> Result<Value, BuiltinError> { ... }
}
// Macro generates: impl BuiltinTool for SleepTool { ... }
```

**USE THIS** when moving tools in commits 12.3-12.11. It saves ~140 LOC per tool (63 tools × ~140 = ~8,800 LOC).

---

## 4. CROSS-CUTTING RULES

### 4.1 Test-driven development — NON-NEGOTIABLE
Every commit starts with a test that fails. RED-GREEN-REFACTOR.

### 4.2 Verification before completion — NON-NEGOTIABLE
`cargo test --workspace --lib` + `cargo clippy --all-targets --all-features` before every commit. Show output.

### 4.3 Git discipline
- 1 logical change = 1 commit
- `type(scope): description` + `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
- `git add <specific files>` — NEVER `git add -A`
- Do NOT push unless explicitly asked
- Pre-commit hooks: rustfmt + clippy (fix issues, NEVER `--no-verify`)

### 4.4 What you may NOT touch
- 5 verbs, schema @0.12, AGPL license — NEVER change
- Shield files — only if directly required
- nika-cli Phase 15 modules — stable, don't refactor

### 4.5 Architecture decisions — DELEGATE
New traits → `spn-rust:rust-architect`. Error types → `spn-rust:rust-core`. Async wiring → `spn-rust:rust-async-expert`.

---

## 5. NUMBERS

| Metric | S0 | S4 end | **S5 end (now)** | Target S9 |
|--------|-----|--------|------------------|-----------|
| Crates | 17 | 24 | **25** | **26** (+nika-builtin) |
| Tests | 10,666 | 10,790 | **10,833** | ~10,900 |
| God files >1500 LOC | 5 | 2 | **2** | 1 |
| Traits defined | 0 | 10 | **10** | **14** (+4 builtin) |
| Production trait impls | 0 | 6 | **6** | **7** (+RunExecutor) |
| `nika/src/main.rs` LOC | 5,530 | 5,530 | **2,043** | 2,043 |
| `nika-engine/` LOC | 160k | 160k | **160k** | **~132k** |
| `nika-builtin/` LOC | — | — | — | **~28k** |
| `nika-cli/` LOC | 8k | 8k | **12.5k** | 12.5k |
| `nika-macros/` LOC | — | — | **554** | ~600 |

---

## 6. TL;DR FOR THE NEXT AGENT

> **Start with baseline verification (section 0.2).** 10,833 tests, 25 crates, HEAD at `5a74f644d`.
>
> **Read** this handoff, then `nika/CLAUDE.md`, `tools/nika/CLAUDE.md`, `tools/nika-engine/ARCHITECTURE.md`.
>
> **Execute Phase 12 starting with SESSION 6** — 4 foundation commits:
> 1. BuiltinError + splinter traits in nika-kernel
> 2. nika-builtin crate skeleton with sealed trait
> 3. Move 5 pure core tools
> 4. Move 13 data tools
>
> **Use `#[builtin_tool]` macro** from nika-macros to generate impls (saves ~140 LOC/tool).
>
> **Delegate trait design** to `spn-rust:rust-architect` before writing any trait.
>
> **TDD every commit.** Verification before every commit. 10,833 baseline, zero clippy warnings.
>
> **Perfection > timing.** No shortcuts, no "defer". Every blocker has a resolution path above.
>
> GOOO 🚀
