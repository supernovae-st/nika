# Constellation Execution Handoff — SESSION 5 (Enriched)

> **This is a self-contained handoff. Copy-paste the ENTIRE file as context for a fresh Claude Code session.**
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
5. docs/plans/2026-04-08-constellation-v2-mega-plan.md     — THE PLAN — read sections 3, 5, 6, 7, 8, 9, 17
6. docs/sprints/HANDOFF-CONSTELLATION-SESSION4-2026-04-08.md  — previous session
```

### 0.2 Baseline verification (FIRST commands to run)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
git status                                  # clean tree expected (minus .cursor/mcp.json delete)
git log --oneline -10                       # should show 8fcb9b400 at top (or newer)
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10790  (if different, investigate BEFORE starting work)
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: no warnings, clean Finished
```

**If baseline is broken, STOP and investigate before touching anything.**

### 0.3 Skills you MUST use (trigger conditions)

| Skill | When to use | Why |
|-------|-------------|-----|
| `spn-powers:test-driven-development` | **Before writing any implementation code.** Every commit starts with a failing test. | Ensures tests actually verify behavior by requiring failure first. |
| `spn-powers:verification-before-completion` | **Before claiming any work "done", committing, or moving to next commit.** | Evidence before assertions always. Run the tests, show the output. |
| `spn-powers:systematic-debugging` | **When encountering any compile error, test failure, or unexpected behavior.** 4-phase framework. | Forbids guess-fixing. Requires understanding root cause first. |
| `spn-powers:root-cause-tracing` | **When errors happen deep in execution and you need to trace backward.** | Systematic trace-back through call stack to find source. |
| `spn-powers:defense-in-depth` | **When data validation happens at multiple layers.** Relevant for Phase 12 builtin tool args. | Validate at every layer to make bugs structurally impossible. |
| `spn-rust:rust-core` | **When designing any trait, error type, or ownership pattern in Phase 12.** | Senior-level Rust patterns for fundamental work. |
| `spn-rust:rust-async` | **When wiring `Arc<dyn EventEmitter>` or spawning tasks in router migrations.** | All concurrent Rust patterns. |
| `spn-powers:frontend-design` | N/A for this session | — |
| `spn-writing:markdown` | **When writing ARCHITECTURE.md updates or ADRs.** | CommonMark, GFM, lint rules. |

**Protocol for skill usage:**
1. Announce it: *"I'm using the test-driven-development skill to …"*
2. Follow the skill exactly (RED-GREEN-REFACTOR)
3. Do not rationalize away the discipline

### 0.4 Agents you MUST delegate to (trigger conditions)

| Agent | When to spawn | Prompt pattern |
|-------|---------------|----------------|
| `spn-rust:rust-architect` | **Before Phase 12 commit 1.** For trait boundary decisions (sealed `BuiltinTool`, error type placement, how to break `run.rs → Runner` cycle). | *"I'm extracting `nika-builtin` from `nika-engine`. The `BuiltinTool` trait currently lives in `nika-engine/src/runtime/builtin/trait.rs` and depends on `NikaError`. I need your architectural verdict on: (1) Should `BuiltinError` go in nika-kernel and get a `From` impl for NikaError, or stay in nika-engine? (2) Should `BuiltinTool` be sealed via private-mod pattern, or via #[builtin_tool] proc-macro from nika-macros? (3) RunTool depends on Runner — how do I break the cycle without pulling nika-runtime as a dep of nika-builtin? Give me a verdict with reasoning, not options."* |
| `spn-rust:rust-core` | **Before committing any new trait** (sealed BuiltinTool, BuiltinError, splinter traits). | *"Review this trait definition for soundness, object safety, Send+Sync bounds, and ergonomics. Here's the file: <paste>. Report issues, don't ask questions."* |
| `spn-rust:rust-async-expert` | **Before Phase 12 commits that wire `Arc<dyn EventEmitter>`** through the builtin router. | *"I'm migrating `BuiltinToolRouter` to take `Arc<dyn EventEmitter>` instead of owned `EventLog`. Here's the before and after: <paste>. Check: (1) no locks held across .await, (2) no blocking I/O, (3) Send+Sync correct, (4) no use of std::sync::Mutex for Arc content. Report violations."* |
| `feature-dev:code-reviewer` | **After each commit's implementation is in place (before commit itself).** | *"Review this diff for Phase 15 commit N (run_workflow extraction to nika-cli). Context: we're moving function bodies from tools/nika/src/main.rs to tools/nika-cli/src/run.rs. No behavior should change. Check: (1) visibility changes correct (pub fn where needed), (2) imports moved, (3) no copy-paste bugs, (4) no accidental behavior drift. Report real issues only, no style nits."* |
| `feature-dev:code-architect` | **Before Phase 12 commit 8 (router migration).** For wiring plan. | *"Design the new BuiltinToolRouter API that takes Arc<dyn EventEmitter> + Arc<dyn TaskResults> + Arc<dyn RecordStore> instead of owned EventLog + Arc<RunContext>. I want a concrete blueprint: field types, constructor signature, builder methods, and call-site adaptation in TaskExecutor."* |
| `general-purpose` (Explore) | **When the question spans 3+ files and you aren't sure where to look.** | *"Find every call site of BuiltinToolRouter::with_* methods across the workspace. Report path:line:context for each. Under 300 words."* |

**Protocol for agent delegation:**
- Every agent starts with ZERO context about this conversation. Prompts must be self-contained.
- Include file paths, line numbers, exact snippets to review.
- Never write "based on your findings, do X" — synthesize yourself.
- Cap long reports: *"Under 200 words"*.

---

## 1. SITUATION

- **Version:** v0.79.0 (unchanged — no version bump yet)
- **Branch:** main
- **Last commit:** `8fcb9b400` feat(arch): Phase 11 — wire kernel Provider trait via RigProvider bridge
- **Tests:** **10,790 passed, 0 failed**
- **Clippy:** Zero warnings
- **Crates:** 24 workspace members
- **Launch target:** May 5, 2026 (but: target, not constraint — scope wins)
- **Codename:** Constellation v2.1
- **Working directory:** `tools/nika/` (within `/Users/thibaut/dev/supernovae/nika/`)

---

## 2. WHAT CAME BEFORE — CONTEXT

### 2.1 Phase progression (cumulative)

| Phase | Title | Commits | Status |
|-------|-------|---------|--------|
| S1 bugs | ARM64 linker, dead MPSC, param redaction, Mutex-before-await | 4 | ✅ |
| Quick wins | `#[must_use]`, FxHashSet, OnceLock, Arc hoist | 4 | ✅ |
| Pre-0 | ARCHITECTURE.md | 2 | ✅ |
| 1 | `nika-kernel` crate (10 traits, L0.5) | 1 | ✅ |
| 2 | `nika-kernel-mock` crate (5 mocks, 23 tests) | 1 | ✅ |
| 4 (partial) | rstest pilot on transform.rs | 1 | ✅ |
| 5.1 | `EventEmitter` blanket impl for `Arc<T>` | 1 | ✅ |
| 8a | transform.rs split (5570→5 files) | 1 | ✅ |
| 8b | template.rs split (4938→2 files) | 1 | ✅ |
| 9+10 | 5 L1 effect crates (75 tests) — clock, fs, blob, http, exec-runner | 2 | ✅ |
| 16 partial | analyze.rs split (5531→6 files) | 1 | ✅ |
| **11** | **Provider trait bridge + `get_dyn_provider`** | **1** | **✅ S4** |

### 2.2 Current layering

```
L0    nika-core (23k)         AST, types, catalogs, trust, capabilities, policy — zero I/O
L0.5  nika-kernel (717)       10 trait defs — zero impls
      nika-kernel-mock (744)  5 hand-written mocks — dev-dep
L1    nika-clock              SystemClock (tokio::time, ZST)
      nika-fs                 TokioFs (tokio::fs + globset, ZST)
      nika-blob               DiskBlobStore (blake3 CAS)
      nika-http               ReqwestClient (SSRF: IPv4/v6/CGN/meta)
      nika-exec-runner        TokioShell (100+ pattern blocklist + NFKC)
      nika-event (4.5k)       EventLog + EventEmitter blanket impl
      nika-lsp-core (12k)     LSP intelligence (pure functions)
L2    nika-engine (160k)      MONOLITH — 6 production trait impls live here
                              + Phase 11 bridge (RigProvider → Provider)
      nika-media (14k), nika-mcp (9k), nika-vault (1.2k), nika-storage (1k), nika-display (13k)
L3    nika-daemon (7k)
L4    nika-cli (8k), nika-tui (88k), nika-serve (4k), nika-lsp (2.5k),
      nika-sdk (3k), nika-init (21k)
L5    nika (5.5k)             Binary entry point — STILL 5530 LOC (target: <500)
```

### 2.3 Known god files remaining

| File | LOC | Target phase |
|------|-----|--------------|
| `nika/src/main.rs` | **5,530** | **Phase 15 (this session)** |
| `nika-engine/src/error.rs` | 2,874 | Phase 6 |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Phase 14+ |

### 2.4 SESSION 4 — Phase 11 recap (what the bridge does)

**File created:** `tools/nika-engine/src/provider/rig/kernel_bridge.rs` (~550 LOC, 19 tests)

```rust
#[async_trait::async_trait]
impl nika_kernel::provider::Provider for RigProvider {
    fn name(&self) -> &str { RigProvider::name(self) }
    fn capabilities(&self, model: &str) -> Option<ModelCapabilities> { … }
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> { … }
    async fn infer_stream(&self, request: InferRequest) -> Result<InferStream, ProviderError> { … }
}
```

**TaskExecutor method added:** `get_dyn_provider(name) -> Arc<dyn Provider>` (mod.rs:876-902)

**Not yet wired through the bridge** (deliberate, to be finished in later phases, NOT skipped):
- `InferRequest.tools` / `ToolDef` / `tool_choice` → wire when Phase 12 absorbs `DynamicSubmitTool`
- `InferRequest.extra` (ProviderExtras) → wire when Phase 12 needs provider-specific params
- `InferResponse.usage` → wire when Phase 12 absorbs the tool-injection path (`raw_chat_completion` has the data)

These three items are **tracked for completion in Phase 12 commits 6-7**.

---

## 3. THE TWO PHASES — DETAILED PLAN

SESSION 5 executes **Phase 15 in full**. Phase 12 is planned here for continuity but executes across SESSIONS 6-9 (separate sessions for depth).

---

## PHASE 15 — main.rs → nika-cli/verbs/ — FULL PLAN

### 15.0 Goal

```
tools/nika/src/main.rs    5,530 LOC    →    <500 LOC
tools/nika-cli/src/       8,000 LOC    →    ~14,000 LOC
```

Match the rust-analyzer pattern: main.rs contains only the `Cli` clap struct, `async fn main`, and a `match cli.command { … }` that dispatches to `nika-cli` handlers. Zero business logic in main.

### 15.1 Current main.rs anatomy (read ONCE, internalize)

Function map with line numbers and LOC estimates:

| Lines | Function | LOC | Destination |
|-------|----------|-----|-------------|
| 57-90 | `long_version()` | 34 | **keep in main** (uses `env!()`) |
| 92-106 | `cli_styles()` | 15 | **keep in main** (clap hook) |
| 108-894 | `Cli` struct + `Commands` enum + `*Action` enums | 787 | **keep in main** (clap derive) |
| 897-953 | `print_env_info()` | 57 | → `nika-cli/src/env.rs` NEW |
| 955-1065 | `print_features()` | 111 | → `nika-cli/src/features.rs` NEW |
| 1067-1081 | `print_feature()` | 15 | → `nika-cli/src/features.rs` |
| 1083-1145 | `count_features()` | 63 | → `nika-cli/src/features.rs` |
| **1147-1935** | **`async fn main()` (THE GIANT)** | **789** | **shrink to ~80 LOC; extract match-arm bodies** |
| 1936-1950 | `is_tui_mode()` | 15 | → `nika-cli/src/dispatch.rs` NEW |
| 1952-1985 | `should_skip_auto_setup()` | 34 | → `nika-cli/src/dispatch.rs` |
| 1987-2006 | `maybe_run_auto_setup()` | 20 | → `nika-cli/src/dispatch.rs` |
| 2008-2015 | `is_nika_workflow()` | 8 | → `nika-cli/src/dispatch.rs` |
| 2017-2041 | `handle_result()` | 25 | → `nika-cli/src/dispatch.rs` |
| 2043-2107 | `download_remote_workflow()` | 65 | → `nika-cli/src/remote.rs` NEW |
| 2109-2171 | `resolve_workflow_path()` | 63 | → `nika-cli/src/discover.rs` NEW |
| 2173-2200 | `count_nika_workflows()` | 28 | → `nika-cli/src/discover.rs` |
| 2202-2335 | `run_demo()` | 134 | → `nika-cli/src/demo.rs` NEW |
| 2337-2388 | `print_agent_presets()` | 52 | → `nika-cli/src/verbs.rs` (extend existing) |
| 2390-2642 | `run_bench()` | 253 | → `nika-cli/src/bench.rs` NEW |
| 2644-2711 | `evaluate_quality()` | 68 | → `nika-cli/src/bench.rs` |
| 2713-2840 | `aggregate_bench_stats()` | 128 | → `nika-cli/src/bench.rs` |
| 2842-2849 | `percentile()` | 8 | → `nika-cli/src/bench.rs` |
| **2851-3253** | **`run_workflow()` THE BIGGEST** | **403** | → **`nika-cli/src/run.rs` NEW** |
| 3255-3273 | `normalize_golden()` | 19 | → `nika-cli/src/test.rs` NEW |
| 3275-3334 | `compare_golden()` | 60 | → `nika-cli/src/test.rs` |
| 3336-3479 | `test_workflow()` | 144 | → `nika-cli/src/test.rs` |
| 3481-3606 | `eval_workflow()` | 126 | → `nika-cli/src/eval.rs` (extend existing) |
| 3608-3725 | `explain_workflow()` | 118 | → `nika-cli/src/explain.rs` NEW |
| 3727-4184 | `validate_workflow()` | 458 | → `nika-cli/src/check.rs` NEW |
| 4186-4217 | `validate_schema_file()` | 32 | → `nika-cli/src/check.rs` |
| 4219-4778 | `validate_workflow_strict()` | 560 | → `nika-cli/src/check.rs` |
| 4780-4834 | `filter_tasks_for_target()` | 55 | → `nika-cli/src/task_filter.rs` NEW |
| 4836-4887 | `filter_tasks_from()` | 52 | → `nika-cli/src/task_filter.rs` |
| 4889-4904 | `resolve_or_discover_workflow()` | 16 | → `nika-cli/src/discover.rs` |
| 4906-4928 | `discover_workflows()` | 23 | → `nika-cli/src/discover.rs` |
| 4930-4956 | `pick_workflow()` | 27 | → `nika-cli/src/discover.rs` |
| 4958-4976 | `simple_input_resolve()` | 19 | → `nika-cli/src/inputs.rs` NEW |
| 4978-5178 | `dry_run_workflow()` | 201 | → `nika-cli/src/run.rs` |
| 5180-5205 | `parse_input_value()` | 26 | → `nika-cli/src/inputs.rs` |
| 5207-5229 | `parse_cli_inputs()` | 23 | → `nika-cli/src/inputs.rs` |
| 5231-5530 | `load_input_file()` + remaining helpers | 300 | → `nika-cli/src/inputs.rs` |

**Summary of extractions:**
- **NEW modules:** `env.rs`, `features.rs`, `dispatch.rs`, `remote.rs`, `discover.rs`, `demo.rs`, `bench.rs`, `run.rs`, `test.rs`, `explain.rs`, `check.rs`, `task_filter.rs`, `inputs.rs`
- **EXTEND existing:** `verbs.rs`, `eval.rs`

### 15.2 Commit plan (TDD-driven, 10 commits)

Each commit follows the same RED-GREEN-REFACTOR-VERIFY loop:

```
RED:     write test that fails (proves old code is gone / new code not yet there)
GREEN:   move the code; test passes
REFACTOR: tighten visibility, remove dead imports
VERIFY:  cargo test --workspace --lib + cargo clippy → must match baseline
COMMIT:  1 logical change per commit with co-author
```

#### Commit 15.1 — Extract `run.rs` (run_workflow + dry_run_workflow)

**Why first:** Biggest single win (600 LOC), cleanest boundary (no TUI, no clap derives).

**Pre-work:**
```bash
# Check call sites of run_workflow + dry_run_workflow in the workspace
Grep run_workflow tools --type rust
# Expected: only main.rs (plus test files)
```

**TDD test (RED):**
1. Create `tools/nika-cli/tests/run_integration.rs`:
   ```rust
   #[test]
   fn run_workflow_exported_from_nika_cli() {
       // Compile-time proof: the function is pub in nika-cli
       let _f: fn(&str, _, _, &[String], _, _, _, _, _, _, _, _, _, &str, _) -> _ =
           nika_cli::run::run_workflow;
   }
   ```
2. `cargo test -p nika-cli --lib run_workflow_exported_from_nika_cli` → **FAILS** (module doesn't exist)

**GREEN steps:**
1. Create `tools/nika-cli/src/run.rs` with `pub async fn run_workflow(...)` signature matching main.rs
2. Move the body wholesale from `tools/nika/src/main.rs` lines 2851-3253
3. Add missing imports — follow compile errors one at a time
4. Add `pub mod run;` to `tools/nika-cli/src/lib.rs`
5. In `main.rs`, replace the body with `nika_cli::run::run_workflow(...).await`
6. Move `dry_run_workflow` similarly (lines 4978-5178)
7. Re-run test: **PASS**

**REFACTOR:**
- Imports in main.rs: remove now-unused ones (cargo will warn)
- If any helpers in the moved code are still referenced from main.rs, move them too (don't leave duplicates)

**VERIFY:**
```bash
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10791 (10790 baseline + 1 new integration test)
cargo clippy --workspace --lib -- -D warnings
# Expected: clean
git diff --stat tools/nika/src/main.rs tools/nika-cli/src/run.rs tools/nika-cli/src/lib.rs
# Expected: main.rs -~600, run.rs +~600
```

**COMMIT MESSAGE:**
```
refactor(cli): extract run_workflow + dry_run_workflow to nika-cli::run

Phase 15 commit 1/10 — main.rs decomposition.

Moves 600 LOC from tools/nika/src/main.rs to tools/nika-cli/src/run.rs.
No behavior change; main.rs Run/RunDry arms now delegate via
`nika_cli::run::run_workflow(...)`.

Part of Constellation Phase 15: main.rs 5530 → <500 LOC.

Verification:
- cargo test --workspace --lib: 10791 passed (0 failed)
- cargo clippy --workspace --lib -- -D warnings: clean
- main.rs LOC: 5530 → ~4930

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

#### Commit 15.2 — Extract `check.rs` (validate_workflow + validate_schema_file + validate_workflow_strict)

**Why second:** Second biggest win (~1050 LOC). Strict mode connects to MCP — ensure those imports follow the move.

**TDD test:** same pattern — compile-time export check.

**GREEN steps:**
1. Create `tools/nika-cli/src/check.rs`
2. Move `validate_workflow` (3727-4184), `validate_schema_file` (4186-4217), `validate_workflow_strict` (4219-4778)
3. Watch for dependencies on `crate::filter_tasks_for_target` — move those TOGETHER or extract first to dispatch.rs
4. Update `main.rs::Check` arm to delegate

**VERIFY:**
```bash
# main.rs should be ~3880 LOC now
wc -l tools/nika/src/main.rs
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10792
```

**RISK:** `validate_workflow_strict` calls into `nika::mcp::validation::McpValidator`. The call has to work from nika-cli crate — verify nika-cli has `nika = { workspace = true }` in Cargo.toml (likely yes).

---

#### Commit 15.3 — Extract `bench.rs` (run_bench + evaluate_quality + aggregate_bench_stats + percentile)

**Why:** 460 LOC, self-contained.

**TDD test:** Add unit test for `percentile()` — it's pure and testable:
```rust
#[test]
fn percentile_empty_is_zero() { … }
#[test]
fn percentile_p50_of_sorted_1_to_10() { … }
#[test]
fn percentile_p99_of_sorted_1_to_100() { … }
```

**GREEN:** Move the 4 functions.

**REFACTOR:** `percentile()` is now unit-testable where it wasn't before. Good.

---

#### Commit 15.4 — Extract `test.rs` (test_workflow + normalize_golden + compare_golden)

**Why:** 223 LOC, contained.

**TDD test:** Unit tests for `normalize_golden()`:
```rust
#[test]
fn normalize_golden_strips_timestamps() { … }
#[test]
fn normalize_golden_preserves_structure() { … }
```

---

#### Commit 15.5 — Extend `eval.rs` with `eval_workflow` body

**Why:** `eval.rs` already exists in nika-cli but has a thin handler — the body lives in main.rs. Move it inline.

**Size:** 126 LOC moved.

---

#### Commit 15.6 — Extract `explain.rs` (explain_workflow)

**Why:** 118 LOC, pure display logic.

**TDD test:** 
```rust
#[test]
fn explain_produces_output_for_minimal_workflow() {
    // Build a minimal workflow in memory, call explain, check output contains task count
}
```

---

#### Commit 15.7 — Extract `discover.rs` + `task_filter.rs` + `remote.rs`

**Why:** Group small helpers into logical modules.

**Functions moved:**
- `discover.rs`: `resolve_or_discover_workflow`, `discover_workflows`, `pick_workflow`, `resolve_workflow_path`, `count_nika_workflows`
- `task_filter.rs`: `filter_tasks_for_target`, `filter_tasks_from`
- `remote.rs`: `download_remote_workflow`

**Size:** ~210 LOC total.

**TDD tests:** 
- `filter_tasks_for_target` — provide a synthetic task list, assert correct subset
- `is_nika_workflow` → path discriminator (pure function)

---

#### Commit 15.8 — Extract `inputs.rs` (parse_input_value + parse_cli_inputs + load_input_file + simple_input_resolve)

**Why:** Input parsing is pure and unit-testable.

**TDD tests (actual value):**
```rust
#[test]
fn parse_input_value_infers_number() {
    assert_eq!(parse_input_value("42"), serde_json::json!(42));
}
#[test]
fn parse_input_value_infers_bool() { … }
#[test]
fn parse_input_value_keeps_string_default() { … }
#[test]
fn parse_cli_inputs_parses_multi_value() { … }
#[test]
fn parse_cli_inputs_rejects_missing_equals() { … }
```

**Size:** ~370 LOC.

---

#### Commit 15.9 — Extract `demo.rs` (run_demo + print_agent_presets) and `env.rs` + `features.rs`

**Why:** Final cleanup of print/env helpers.

**Functions:**
- `demo.rs`: `run_demo`, `print_agent_presets`
- `env.rs`: `print_env_info`
- `features.rs`: `print_features`, `print_feature`, `count_features`

**Size:** ~450 LOC.

---

#### Commit 15.10 — Extract `dispatch.rs` + final main.rs polish

**Why:** Final shrink. Move the remaining helpers.

**Functions:**
- `dispatch.rs`: `is_tui_mode`, `should_skip_auto_setup`, `maybe_run_auto_setup`, `is_nika_workflow`, `handle_result`

**Size:** ~100 LOC.

**Then:** audit main.rs. Target `<500 LOC`. The `Cli` struct + `Commands` enum will take ~800 LOC by themselves — this is acceptable but consider splitting `Commands` enum to `nika-cli/src/commands.rs` if it pushes over 500.

**Actual target after 10 commits:** `main.rs <= 900 LOC` is acceptable (Cli derive dominates). If you can get it under 500, great.

**Final verification:**
```bash
wc -l tools/nika/src/main.rs                    # target: <900, stretch: <500
wc -l tools/nika-cli/src/*.rs | sort -rn        # ~14k total
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10800+ (10790 baseline + ~15 new unit tests)
cargo clippy --workspace --lib -- -D warnings
```

### 15.3 Per-commit TDD cycle template

```
┌──────────────────────────────────────────────────────────┐
│  1. Announce skill: "test-driven-development"            │
│  2. Write failing test (compile OR assertion failure)    │
│  3. Run test — prove it fails                            │
│  4. Move the code (smallest possible diff)               │
│  5. Follow compile errors one at a time                  │
│  6. Run test — prove it passes                           │
│  7. Announce skill: "verification-before-completion"     │
│  8. Run full workspace tests + clippy                    │
│  9. Show output to confirm                               │
│ 10. Spawn code-reviewer agent (see 0.4)                  │
│ 11. Address real issues; ignore style nits                │
│ 12. git add <specific files> (NEVER git add -A)          │
│ 13. git commit with Nika 🦋 co-author                    │
│ 14. Mark TodoList item complete                          │
└──────────────────────────────────────────────────────────┘
```

### 15.4 Risk register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Moved function references private items in `main.rs` | HIGH | Compile error | Make them pub(crate) or move them too |
| Cyclic import: `nika-cli` → `nika` (binary) | LOW | Compile fail | Binary crates cannot be imported — this is impossible |
| `nika-cli` doesn't already depend on everything `main.rs` uses | MEDIUM | Compile fail | Add missing workspace deps as encountered |
| Clippy regression from new module boundaries | LOW | -D warnings fails | Address immediately; never `#[allow]` |
| Test regression from subtle behavior drift | MEDIUM | Tests fail | TDD test catches it pre-commit |
| `cli_styles()` depends on private consts in main.rs | LOW | Compile error | Move consts too, or make pub(crate) |

### 15.5 Decision tree: "what if this happens"

```
Compile error after moving a function?
├── Missing import?          → Add it to the new module
├── Private item referenced? → Make it pub(crate) or move it too
├── Trait not in scope?      → Import it in the new module
└── Cyclic dependency?       → STOP; ask rust-architect agent

Test regression?
├── Same test failing before? → Baseline issue, not your commit
├── New test failing?         → Systematic-debugging skill; do NOT guess
└── Flaky test?               → Root cause; condition-based-waiting skill

Clippy warning?
├── In moved code?            → Fix it (we may as well improve on the way)
├── In new test?              → Fix it
└── In unrelated code?        → Baseline issue, pre-existing, leave it
```

---

## PHASE 12 — nika-builtin extraction — FULL PLAN (SESSIONS 6-9)

### 12.0 Goal (everything in scope — no skips)

Extract all **63 builtin tools** + sealed `BuiltinTool` trait into a dedicated L2 crate `nika-builtin`, including media tools, introspection, and RunTool. No "defer to Phase 12.5".

```
Before (S4 baseline):
  nika-engine/src/runtime/builtin/     ~22,383 LOC (with tests)
  
After (S9 target):
  nika-builtin/                        ~20,000 LOC  (extracted from engine)
  nika-engine/src/runtime/builtin/     ~2,000 LOC   (only glue: BuiltinToolRouter wiring for engine-specific contexts)
```

### 12.1 Full tool inventory (63 tools)

| Tier | Count | Tools |
|------|-------|-------|
| **Core** | 7 | sleep, log, emit, assert, prompt, run, complete |
| **Data** | 13 | jq, tree_data, inject, map, filter, group_by, enrich, json_merge, json_diff, set_diff, zip, chunk, token_count |
| **Data Sprint 2** | 6 | json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten |
| **File** | 5 | read, write, edit, glob, grep |
| **Introspection** | 6 | cost, records, dag_info, task_status, threads, orchestrate |
| **Media Tier 1 (always-on)** | 5 | import, decode, dimensions, thumbhash, dominant_color |
| **Media Tier 2 (core)** | 6 | thumbnail, convert, strip, metadata, optimize, svg_render |
| **Media Tier 3 (opt-in)** | 13 | phash, compare, pdf_extract, chart, provenance, verify, qr_validate, quality, html_to_md, css_select, extract_metadata, extract_links, readability, pipeline |
| **Adapters** | 2 | rig_adapter (NikaBuiltinToolAdapter), file_adapter, fetch_tool |
| **TOTAL** | **63** | — |

### 12.2 Blocker analysis with resolution paths (NO skips)

| ID | Blocker | Resolution |
|----|---------|------------|
| **B1** | Every builtin uses `crate::error::NikaError` | **Define `BuiltinError` in `nika-kernel/src/builtin.rs`** with variants: `InvalidArgs`, `Io`, `Parse`, `Timeout`, `Schema`, `Denied`, `Other`. Add `impl From<BuiltinError> for NikaError` in nika-engine. Every tool returns `Result<String, BuiltinError>`. |
| **B2** | `run.rs` depends on `crate::runtime::Runner` | **Define `RunExecutor` trait in `nika-kernel/src/scope.rs`:** `async fn run_workflow(path: &str, inputs: Value, depth: u32) -> Result<Value, BuiltinError>`. `RunTool` takes `Arc<dyn RunExecutor>`. nika-engine provides the concrete impl via a newtype wrapping `Runner`. Cycle broken. |
| **B3** | `records.rs`, `router.rs` take `Arc<RunContext>` | **Use splinter traits already in `nika-kernel/src/scope.rs`** (currently unused): `TaskResults`, `RecordStore`, `BindingScope`, `MediaStaging`, `VaultLookup`, `InvocationContext`. `records.rs` takes `Arc<dyn RecordStore>`. `cost.rs` takes `Arc<dyn EventEmitter>`. Each tool consumes only the splinters it needs. |
| **B4** | `rig_adapter.rs`, introspection, cost take owned `EventLog` | **Use `Arc<dyn EventEmitter>`** — blanket impl already shipped in Phase 5.1 (commit `4c2af7fe3`). Replace field types. |
| **B5** | Media tools take `Arc<MediaToolContext>` | **Define `MediaContext` trait in `nika-kernel/src/scope.rs`:** `fn cas(&self) -> &dyn BlobStore`, `fn compute_pool(&self) -> &ComputePool`, `fn budget(&self) -> &Budget`. Media tools take `Arc<dyn MediaContext>`. Concrete impl stays in nika-engine (wraps `MediaToolContext`). |
| **B6** | File tools wrap `crate::tools::{ReadTool, WriteTool, ...}` + `ToolContext` | **Rewrite the 5 file tools** to take `Arc<dyn Filesystem>` from `nika-fs`. `ToolContext` moves to `nika-builtin/src/context.rs` as a security boundary (permission mode + working dir). The underlying `tokio::fs` calls go through `TokioFs` (already shipped in S3). |
| **B7** | `prompt.rs` depends on `PolicyEnforcer`, `HitlHandler`, `PolicyDecision` | **Define `HitlPrompt` trait in `nika-kernel/src/scope.rs`:** `async fn ask(&self, message: &str, default: Option<&str>) -> Result<String, BuiltinError>`. nika-engine provides impl via `HitlHandler`. `PolicyEnforcer` stays engine-side — `prompt.rs` doesn't directly need it. |
| **B8** | `data/transform.rs` uses `crate::binding::TransformExpr` | No blocker — this type is already in `nika-core::binding::transform`. Fix the import path. |
| **B9** | `rig_adapter.rs:127` hardcodes `crate::runtime::orchestrate` constant | Define `pub const ORCHESTRATE_TOOL_NAME: &str = "orchestrate"` in nika-core::catalogs::builtins (or nika-kernel). Import there. |
| **B10** | `run.rs` uses `parse_analyzed` from nika-core | No blocker — already in nika-core. |
| **B11** | Media tools depend on `nika-media` crate directly | `nika-builtin` declares `nika-media` as dep. Fine — both are L2, parallel. |
| **B12** | `jq.rs` depends on `jaq-core`, `jaq-std`, `jaq-json` | `nika-builtin` Cargo.toml declares them. Already at workspace level. |

**Every blocker has a resolution path. Nothing skipped.**

### 12.3 Commit plan — 13 commits across 4 sessions

#### SESSION 6 — Foundation (4 commits)

**Commit 12.1 — `nika-kernel` additions**
- Create `nika-kernel/src/builtin.rs` with `BuiltinError` enum
- Extend `nika-kernel/src/scope.rs` with `RunExecutor`, `HitlPrompt`, `MediaContext` traits
- Add `impl From<BuiltinError> for NikaError` in nika-engine/src/error.rs
- **TDD tests:** 
  - `BuiltinError::Display` messages
  - `From<BuiltinError> for NikaError` round-trip
  - `RunExecutor` trait object-safety check (compile-time)
- **Verification:** all 10,790 + new tests pass
- **Agent:** spawn `spn-rust:rust-architect` BEFORE writing the trait (see 0.4)

**Commit 12.2 — Create `nika-builtin` crate skeleton**
- `tools/nika-builtin/Cargo.toml` with deps: nika-core, nika-kernel, nika-event, nika-fs, nika-media (eventually), serde_json, async-trait, thiserror, tracing, jaq-core, jaq-std, jaq-json
- `src/lib.rs` with module stubs
- Add to workspace members
- Move `trait.rs` → `nika-builtin/src/builtin_trait.rs`, **seal it**:
  ```rust
  mod sealed { pub trait Sealed {} }
  pub trait BuiltinTool: sealed::Sealed + Send + Sync { … }
  ```
- **TDD test:** compile-time assertion `assert_send_sync::<Arc<dyn BuiltinTool>>()`
- **Agent:** spawn `spn-rust:rust-core` to review the sealed trait design

**Commit 12.3 — Move 5 pure core tools (sleep, log, emit, assert, complete)**
- These have zero coupling beyond `NikaError` (now `BuiltinError`)
- Move each with its tests; each tool becomes a pub mod
- Re-export from `nika-builtin::lib`
- `nika-engine/src/runtime/builtin/{sleep,log,emit,assert,complete}.rs` → re-export from nika-builtin
- **TDD:** run each tool's existing tests; they must still pass
- **Verification:** 10,790 → still 10,790 (no net change)

**Commit 12.4 — Move 13 data tools (entire `data/` directory)**
- Move `data/{aggregate,io,jq,json_diff,merge,text,transform}.rs` + `data/mod.rs`
- Fix imports: `crate::binding::TransformExpr` → `nika_core::binding::transform::TransformExpr`
- Re-export `JqTool`, `JsonDiffTool`, `JsonMergeTool`, `SetDiffTool`, `ZipTool`, `MapTool`, `FilterTool`, `GroupByTool`, `ChunkTool`, `TokenCountTool`, `EnrichTool`, `InjectTool`, `TreeDataTool`
- **TDD:** data tool tests move with them
- **Verification:** test count unchanged

---

#### SESSION 7 — Sprint 2 + Introspection + File (4 commits)

**Commit 12.5 — Move 6 Sprint 2 data tools**
- `aggregate.rs`, `json_verify.rs`, `yaml_validate.rs`, `locale_lookup.rs`, `json_transform.rs` (JsonFlatten + JsonUnflatten)

**Commit 12.6 — Move 6 introspection tools**
- `cost.rs`, `records.rs`, `introspect_{dag,task,threads,orchestrate}.rs`
- These use `Arc<dyn EventEmitter>` and `Arc<dyn RecordStore>` splinters
- **Agent:** spawn `spn-rust:rust-async-expert` to review the wiring

**Commit 12.7 — Move 5 file tools + `ToolContext`**
- Rewrite `read.rs`, `write.rs`, `edit.rs`, `glob.rs`, `grep.rs` to use `Arc<dyn Filesystem>` from nika-fs
- Move `ToolContext` struct to nika-builtin
- Move `file_adapter.rs`
- Move `check_path_readable` → use `Filesystem::canonicalize` + `Filesystem::exists`
- **Critical:** the Shield integration (check_path_readable blocks untrusted agents from reading nika.toml etc.) MUST survive the move. Dedicated test.

**Commit 12.8 — Define `RunExecutor` impl in nika-engine, move `run.rs`**
- `run.rs` now takes `Arc<dyn RunExecutor>` 
- nika-engine provides `EngineRunExecutor` wrapping `Runner`
- Cycle broken: nika-builtin has zero knowledge of Runner
- **Agent:** spawn `spn-rust:rust-async-expert` for the async trait + Runner wiring

---

#### SESSION 8 — Media (3 commits)

**Commit 12.9 — Define `MediaContext` trait, wire Media Tier 1 (5 always-on tools)**
- `import`, `decode`, `dimensions`, `thumbhash`, `dominant_color`
- These are called in the hot path; must stay fast

**Commit 12.10 — Move Media Tier 2 (6 core tools)**
- `thumbnail`, `convert`, `strip`, `metadata`, `optimize`, `svg_render`
- Feature-gated in nika-media already; carry the gates through

**Commit 12.11 — Move Media Tier 3 (13 opt-in tools) + `rig_adapter.rs` + `fetch_tool.rs`**
- Large commit — group them since they all depend on `MediaContext`
- `rig_adapter.rs` (NikaBuiltinToolAdapter) moves too
- `fetch_tool.rs` (used by agent dispatch)

---

#### SESSION 9 — Router migration + cleanup (2 commits)

**Commit 12.12 — Migrate `BuiltinToolRouter` to nika-builtin**
- Rewrite constructor signature:
  ```rust
  pub fn new(
      events: Arc<dyn EventEmitter>,
      filesystem: Arc<dyn Filesystem>,
      media_context: Arc<dyn MediaContext>,
      run_executor: Arc<dyn RunExecutor>,
      hitl: Arc<dyn HitlPrompt>,
      task_results: Arc<dyn TaskResults>,
      record_store: Arc<dyn RecordStore>,
  ) -> Self
  ```
- nika-engine/TaskExecutor builds this from its concrete `EventLog`, `RunContext`, etc.
- **Agent:** spawn `feature-dev:code-architect` BEFORE touching the constructor (see 0.4)

**Commit 12.13 — Cleanup + re-exports**
- Remove old `nika-engine/src/runtime/builtin/*.rs` files that got moved
- Keep only: `BuiltinToolRouter` builder glue, `EngineRunExecutor` impl
- `pub use nika_builtin::*` from nika-engine for backward compat imports (temporary)
- **Final verification:**
  - `nika-engine/src/runtime/builtin/` < 2000 LOC
  - `nika-builtin/src/` ~20000 LOC
  - All 10,790+ tests pass
  - Clippy clean

### 12.4 Phase 12 TDD pattern

For EACH tool migration:

1. **RED:** grep for the tool's test file in nika-engine. Copy test verbatim to nika-builtin. Run — fails (module doesn't exist).
2. **GREEN:** move the tool module.
3. **VERIFY:** tests pass in new location.
4. **REFACTOR:** if old path still has the file, delete it. If re-export is needed for backward compat imports, add it.

### 12.5 Phase 12 verification checklist (per commit)

```
[ ] cargo test --workspace --lib → pass count ≥ baseline + new tests
[ ] cargo clippy --workspace --lib -- -D warnings → clean
[ ] cargo build --workspace → clean
[ ] Check nika-builtin LOC growth
[ ] Check nika-engine LOC shrinkage
[ ] No orphaned modules (dead code in nika-engine after move)
[ ] Spawn code-reviewer agent before commit
[ ] Git commit uses Nika 🦋 co-author
```

---

## 4. CROSS-CUTTING RULES (apply to EVERY commit in BOTH phases)

### 4.1 Test-driven development — NON-NEGOTIABLE

Every commit starts with a test that fails. This is the test-driven-development skill's RED phase.

**Forbidden patterns:**
- "I'll add tests at the end" — NO
- "This is just a move, no tests needed" — NO, write a compile-time test
- "The existing tests cover it" — acceptable ONLY if you run them first and prove they catch regressions by temporarily breaking the code

**Required patterns:**
- Write test → run test → see it fail → implement → run test → see it pass
- If you can't write a failing test, you don't understand the change well enough

### 4.2 Verification before completion — NON-NEGOTIABLE

Before claiming "done", committing, or moving on:
1. Run the full workspace test suite
2. Run clippy with `-D warnings`
3. Show the command output in your response
4. Compare pass counts against the baseline
5. If anything regressed, STOP and fix it — do not "try again later"

**Forbidden phrases:**
- "Should work" — run it
- "I think it's ready" — prove it
- "Let me commit this and we'll check later" — NO
- "The tests probably pass" — NO

### 4.3 Systematic debugging — when things break

Use the four-phase framework:
1. **Root cause investigation** — what is the EXACT error? Copy the full message.
2. **Pattern analysis** — have we seen this before? Grep the codebase for similar patterns.
3. **Hypothesis testing** — form a specific hypothesis, test it minimally
4. **Implementation** — fix only what's broken, not what's nearby

**Forbidden patterns:**
- Guessing and retrying
- "Let me try this" without a hypothesis
- Bypassing errors with `#[allow]`, `.unwrap()`, or `#[ignore]`
- Swapping approaches at the first friction

### 4.4 Architecture decisions — when to delegate

Delegate to `spn-rust:rust-architect` when:
- Defining a new trait (sealed, object safety, ergonomics)
- Deciding where a type lives (nika-kernel vs nika-engine vs nika-core)
- Breaking a dependency cycle
- Changing a constructor signature that touches 10+ call sites

Delegate to `spn-rust:rust-core` when:
- Reviewing error type design
- Reviewing ownership patterns (Arc vs Box vs &T)
- Reviewing trait bounds (Send, Sync, 'static, ?Sized)

Delegate to `spn-rust:rust-async-expert` when:
- Wiring `Arc<dyn EventEmitter>` through async functions
- Spawning tasks
- Cancellation token propagation
- `.await`-crossing state

Delegate to `feature-dev:code-reviewer` when:
- After implementing a commit but BEFORE committing it
- Prompt with the specific diff, not "review my work"

### 4.5 Git discipline

- **1 logical change = 1 commit.** Never batch unrelated fixes.
- Commit message format: `type(scope): description` + empty line + body + empty line + co-author
- Co-author line: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`  — NEVER Claude, NEVER Anthropic
- `git add <specific files>` — **never** `git add -A` or `git add .`
- **Do NOT push** unless explicitly asked
- Pre-commit hooks run rustfmt + clippy — if they fail, fix the underlying issue, NEVER skip with `--no-verify`

### 4.6 What you may NOT touch

- The 5 verbs (`infer`, `exec`, `fetch`, `invoke`, `agent`) — NEVER change
- Schema version `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Shield files (merged in Sprint 2, stable) — only if directly required by the phase
- `.cursor/mcp.json` deletion in git status — leave it alone, unrelated

### 4.7 What you MUST touch (the `perfection > timing` rule)

- **If you find dead scaffolding, wire it. Do NOT delete it.**
- **If you find a bug, fix it in the same commit if it's in the area you're touching.**
- **If you find a better name, rename it.**
- **If you find unused imports after a move, remove them.**
- **If a clippy warning is in code you're touching, fix it.**
- **No `// TODO: fix later` comments. Fix it now or track it in the handoff.**

---

## 5. NUMBERS — STATE OF THE WORLD

| Metric | S0 baseline | After S1+2 | After S3 | **After S4 (now)** | Target S5 | Target S9 |
|--------|-------------|------------|----------|--------------------|-----------|-----------|
| Workspace crates | 17 | 19 | 24 | **24** | 24 | **25** (+nika-builtin) |
| Tests | 10,666 | 10,693 | 10,768 | **10,790** | ~10,805 | ~10,850 |
| God files (>1500 LOC source) | 5 | 3 | 2 | **2** | **1** (main.rs done) | 1 |
| Traits defined | 0 | 10 | 10 | **10** | 10 | **14** (+4 builtin traits) |
| Production trait impls | 0 | 0 | 5 | **6** | 6 | **7** (+RunExecutor) |
| Mock implementations | 0 | 5 | 5 | **5** | 5 | 5 |
| Clippy warnings | 0 | 0 | 0 | **0** | 0 | 0 |
| `nika/src/main.rs` LOC | 5,530 | 5,530 | 5,530 | **5,530** | **<900** | <900 |
| `nika-engine/` LOC | 160,000 | 160,000 | 160,000 | **160,000** | 160,000 | **~140,000** |
| `nika-cli/` LOC | 8,000 | 8,000 | 8,000 | **8,000** | **~14,000** | ~14,000 |
| `nika-builtin/` LOC | — | — | — | — | — | **~20,000** |

---

## 6. ULTIMATE TL;DR FOR THE NEXT AGENT

> **Start with baseline verification (section 0.2).** If it doesn't pass, investigate first.
>
> **Read** this entire handoff, then `nika/CLAUDE.md`, `tools/nika/CLAUDE.md`, `tools/nika-engine/ARCHITECTURE.md`.
>
> **Execute Phase 15 in SESSION 5** — 10 commits, mostly mechanical moves from `tools/nika/src/main.rs` (5530 LOC) to `tools/nika-cli/src/*.rs` (new modules). Target: main.rs < 900 LOC.
>
> **For every commit:**
> 1. Announce `test-driven-development` skill
> 2. Write a failing test FIRST
> 3. Move the code
> 4. Prove the test passes
> 5. Announce `verification-before-completion` skill
> 6. Run `cargo test --workspace --lib` + `cargo clippy --workspace --lib -- -D warnings`
> 7. Spawn `feature-dev:code-reviewer` agent with the diff
> 8. Address real issues
> 9. Commit with Nika 🦋 co-author, specific file staging
> 10. Mark TaskList item complete
>
> **For architecture decisions** in Phase 12 (SESSIONS 6-9), delegate to `spn-rust:rust-architect`, `spn-rust:rust-core`, `spn-rust:rust-async-expert` before writing traits or changing constructors.
>
> **Perfection > timing.** No "skip", no "defer", no "post-launch". Every blocker in Phase 12 has a resolution path above — execute them all. Launch date follows the work.
>
> **Do NOT push** unless explicitly asked. Do NOT commit unless explicitly asked.
>
> **Baseline to match:** 10,790 tests passing, zero clippy warnings, 24 crates. If you regress, STOP and fix it before moving on.
>
> GOOO 🚀
