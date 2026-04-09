# Constellation Execution Handoff — SESSION 6 (Enriched)

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
5. docs/plans/2026-04-08-constellation-v2-mega-plan.md     — THE PLAN — sections 3, 5, 6, 7, 8, 9, 12, 17
6. docs/sprints/HANDOFF-CONSTELLATION-SESSION5-2026-04-08.md  — Phase 12 full commit plan (section 3)
```

### 0.2 Baseline verification (FIRST commands to run)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
git status                                  # clean tree expected
git log --oneline -3                        # should show ef17f7e9a at top
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: 10833
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: clean Finished
```

**If baseline is broken, STOP and investigate before touching anything.**

### 0.3 Skills you MUST use (trigger conditions)

| Skill | When to use | Why |
|-------|-------------|-----|
| `spn-powers:test-driven-development` | **Before writing ANY implementation code.** Every commit starts with a failing test. | RED-GREEN-REFACTOR ensures tests verify behavior by requiring failure first. |
| `spn-powers:verification-before-completion` | **Before claiming "done", committing, or moving to next commit.** | Evidence before assertions. Run tests, show output, compare to baseline. |
| `spn-powers:systematic-debugging` | **When encountering any compile error, test failure, or unexpected behavior.** | 4-phase framework: root cause → pattern analysis → hypothesis → fix. No guessing. |
| `spn-powers:root-cause-tracing` | **When errors happen deep in execution.** | Trace backward through call stack to find the source. |
| `spn-powers:defense-in-depth` | **When data validation happens at multiple layers** (relevant for Phase 12 file tool security). | Validate at every layer to make bugs structurally impossible. |
| `spn-rust:rust-core` | **When designing any trait, error type, or ownership pattern.** | Senior-level Rust: sealed traits, object safety, Send+Sync bounds. |
| `spn-rust:rust-async-expert` | **When wiring `Arc<dyn EventEmitter>` or spawning tasks.** | No locks across .await, no blocking I/O, Send+Sync correct. |
| `spn-rust:rust` | **When writing any Rust code.** Master skill with discovery routing. | Routes to specialized sub-skills automatically. |

**Protocol:**
1. Announce it: *"I'm using the test-driven-development skill to …"*
2. Follow the skill exactly
3. Do not rationalize away the discipline

### 0.4 Agents you MUST delegate to (with prompt templates)

| Agent | When to spawn | Exact prompt pattern |
|-------|---------------|----------------------|
| `spn-rust:rust-architect` | **Before Phase 12 commit 1** (trait design) | *"I'm extracting `nika-builtin` from `nika-engine`. The `BuiltinTool` trait currently lives in `nika-engine/src/runtime/builtin/trait.rs`. I need your architectural verdict on: (1) Should `BuiltinError` go in nika-kernel (L0.5) and get a `From` impl for NikaError, or in a new crate? (2) Should `BuiltinTool` be sealed via private-mod pattern, or via #[builtin_tool] proc-macro? (3) RunTool depends on Runner — how do I break the cycle? The `RunExecutor` trait in nika-kernel is proposed. Give a verdict with reasoning, not options."* |
| `spn-rust:rust-core` | **Before committing any new trait** | *"Review this trait definition for soundness, object safety, Send+Sync bounds, and ergonomics. Here's the file: <paste>. Report issues, don't ask questions."* |
| `spn-rust:rust-async-expert` | **Before wiring `Arc<dyn EventEmitter>` through builtin router** | *"I'm migrating `BuiltinToolRouter` to take `Arc<dyn EventEmitter>` instead of owned `EventLog`. Here's the before and after: <paste>. Check: (1) no locks held across .await, (2) no blocking I/O, (3) Send+Sync correct. Report violations."* |
| `feature-dev:code-reviewer` | **After each commit (before git commit itself)** | *"Review this diff for Phase 12 commit N (<description>). Context: <what we're doing>. Check: (1) no behavior drift, (2) imports correct, (3) no copy-paste bugs. Report real issues only."* |
| `feature-dev:code-architect` | **Before commit 12.12 (router migration)** | *"Design the new BuiltinToolRouter API. Current: takes `Arc<RunContext>`. Target: takes individual trait objects. Give constructor signature, field types, builder methods."* |

**Protocol:** Every agent starts with ZERO context. Prompts must be self-contained with file paths and snippets.

---

## 1. SITUATION

- **Version:** v0.79.0 (no bump yet)
- **Branch:** main
- **Last commit:** `ef17f7e9a` fix(cli): address P1 findings from code review
- **Tests:** **10,833 passed, 0 failed**
- **Clippy:** Zero warnings (`--all-targets --all-features`)
- **Crates:** **25** workspace members
- **Launch target:** May 5, 2026 (target, not constraint — scope wins)
- **Codename:** Constellation v2.1
- **Working directory:** `tools/nika/` (within `/Users/thibaut/dev/supernovae/nika/`)

---

## 2. WHAT CAME BEFORE — CUMULATIVE CONTEXT

### 2.1 Phase progression (all sessions)

| Phase | Title | Session | Commits | Status |
|-------|-------|---------|---------|--------|
| S1 bugs | ARM64 linker, dead MPSC, param redaction, Mutex-before-await | S1 | 4 | ✅ |
| Quick wins | `#[must_use]`, FxHashSet, OnceLock, Arc hoist | S1 | 4 | ✅ |
| Pre-0 | ARCHITECTURE.md | S1 | 2 | ✅ |
| 1 | `nika-kernel` crate (10 traits, L0.5) | S2 | 1 | ✅ |
| 2 | `nika-kernel-mock` (5 mocks, 23 tests) | S2 | 1 | ✅ |
| **3** | **`nika-macros` crate (3 derives + 1 attr macro)** | **S5** | **7** | **✅** |
| 4 partial | rstest pilot on transform.rs | S3 | 1 | ✅ |
| 5.1 | `EventEmitter` blanket impl for `Arc<T>` | S3 | 1 | ✅ |
| 8a | transform.rs split (5570→5 files) | S3 | 1 | ✅ |
| 8b | template.rs split (4938→2 files) | S3 | 1 | ✅ |
| 9+10 | 5 L1 effect crates (75 tests) | S3 | 2 | ✅ |
| 11 | Provider trait bridge + `get_dyn_provider` | S4 | 1 | ✅ |
| **15** | **main.rs → nika-cli (5530→2043 LOC)** | **S5** | **8** | **✅** |
| 16 partial | analyze.rs split (5531→6 files) | S3 | 1 | ✅ |

### 2.2 Deferred phases (with reasons — DO NOT start these)

| Phase | Why deferred | When to do |
|-------|-------------|------------|
| **5.2** EventSink flip (5 hot sites) | 30+ structs hold `EventLog` by value; perf win negligible since EventLog is cheap to clone internally | Phase 14 (each verb crate takes `Arc<dyn EventEmitter>`) |
| **6** error_domains big-bang | 180+ call sites, miette `#[diagnostic]` doesn't delegate through `#[error(transparent)]`, 70 sites need semantic analysis | Dedicated session. Start with DagError (0 sites), then ProviderError (15), BindingError (8), ExecutionError (70) |
| **7** LSP handler unification | Needs handler migration pattern first | Post-Phase 12 |
| **8c** runner/mod.rs split | 2,344 LOC but already decomposed into sub-methods; tight coupling needs Phase 14 VerbExecutor restructure | Phase 14 |

### 2.3 Current layering (25 crates)

```
L0    nika-core (23k)         AST, types, catalogs, trust, capabilities, policy — zero I/O
L0.5  nika-kernel (717)       10 trait defs — zero impls
      nika-kernel-mock (744)  5 hand-written mocks — dev-dep
      nika-macros (554)       3 derives + 1 attr macro
L1    nika-clock, nika-fs, nika-blob, nika-http, nika-exec-runner  — 5 L1 effect crates
      nika-event (4.5k)       EventLog + EventEmitter blanket impl
      nika-lsp-core (12k)     LSP intelligence (pure functions)
L2    nika-engine (160k)      MONOLITH — providers, builtins, runtime, http, exec
      + kernel_bridge.rs      impl Provider for RigProvider (S4)
      + get_dyn_provider()    Arc<dyn Provider> keystone (S4)
      nika-display (13k), nika-media (14k), nika-mcp (9k)
      nika-daemon (7k), nika-storage (1k), nika-vault (1.2k)
L3    nika-cli (12.5k)        CLI handlers — expanded in S5 (+9 modules)
      nika-tui (88k), nika-serve (4k), nika-lsp (2.5k), nika-sdk (3k), nika-init (21k)
L5    nika (2k)               Binary entry point — 2,043 LOC
```

### 2.4 Phase 15 recap — what just happened

**main.rs: 5,530 → 2,043 LOC** (63% reduction). 9 new nika-cli modules:

| Module | LOC | Functions |
|--------|-----|-----------|
| check.rs | 1,072 | validate_workflow, validate_schema_file, validate_workflow_strict |
| eval.rs | +130 | eval_workflow (appended) |
| run.rs | 621 | run_workflow, dry_run_workflow |
| bench.rs | 520 | run_bench, evaluate_quality, aggregate_bench_stats, percentile |
| inputs.rs | 350 | parse_input_value, parse_cli_inputs, load_input_file, simple_input_resolve |
| test_cmd.rs | 334 | test_workflow, normalize_golden, compare_golden |
| discover.rs | 298 | resolve_workflow_path, download_remote_workflow, etc. |
| demo.rs | 194 | run_demo, print_agent_presets |
| explain.rs | 133 | explain_workflow |
| task_filter.rs | 114 | filter_tasks_for_target, filter_tasks_from |

**Key pattern:** `nika::*` → `nika_engine::*` for all code in nika-cli. TUI arms stay in main.rs (`nika::tui::*` only exists via binary re-export, and nika-tui is 88k LOC — too heavy as dep for nika-cli).

### 2.5 Phase 3 recap — nika-macros

`nika-macros` (L0.5, 554 LOC) — 3 derive macros + 1 attribute macro:

| Macro | Type | What it does | Already wired |
|-------|------|-------------|---------------|
| `#[derive(NikaErrorCode)]` | derive | Generates `code()` from `#[nika_code("NIKA-XXX")]` | ✅ wired in error.rs |
| `#[derive(EventTaskId)]` | derive | Generates `task_id()` method | ✅ wired in event |
| `#[builtin_tool]` | attribute | Generates `BuiltinTool` impl from async fn | ❌ NOT YET WIRED — wire in Phase 12 |
| `dispatch_helpers!` | helper | Transform dispatch macros | ✅ wired in transform.rs |

**`#[builtin_tool]` actual API** (from `nika-macros/src/builtin_tool.rs`):
```rust
#[builtin_tool(name = "echo", description = "Echo back")]
async fn echo_tool(params: EchoParams) -> Result<EchoResponse, NikaError> { ... }
// Generates: pub struct EchoToolStub; + impl BuiltinTool for EchoToolStub
```

Key attrs: `name` (required), `description` (optional), `error` (default: `crate::NikaError`), `trait_path` (default: `crate::runtime::builtin::BuiltinTool`).

**When moving tools to nika-builtin**, override `trait_path` and `error` to point at the new crate's types.

### 2.6 Known god files remaining

| File | LOC | Target phase |
|------|-----|--------------|
| `nika-engine/src/error.rs` | 2,874 | Phase 6 (dedicated session) |
| `nika-engine/src/runtime/runner/mod.rs` | 2,344 | Phase 14 |
| `nika-engine/src/binding/template/mod.rs` | 2,053 | Phase 14+ |
| `nika-engine/src/binding/resolve.rs` | 3,948 | Post-launch |

### 2.7 Known bugs found in Session 5 code review (P2, not yet fixed)

These are **pre-existing** bugs discovered during Phase 15 extraction — not regressions.

| ID | File | Description | Priority |
|----|------|-------------|----------|
| **CR-F3** | check.rs:192,638 | Regex compiled per `validate_workflow` / `validate_workflow_strict` call — perf issue in lint loops | P2 |
| **CR-F5** | discover.rs:50 | `download_remote_workflow` uses `curl` subprocess without SSRF blocklist (private IP ranges not blocked) | P2-sec |
| **CR-F7** | eval.rs:739 | `eval_workflow` parallelism is sequential — semaphore loop awaits serially, `--parallel N` is a no-op above 1 (needs `tokio::spawn`) | P2-bug |

**Already fixed in Session 5:**
- P1 F1: `check.rs` strict mode shell-guard check now uses regex (was string contains)
- P1 F2: `eval.rs` semaphore `.unwrap()` replaced with `map_err` + `?`
- Style: duplicate `#[cfg(unix)]` in cli/mod.rs removed

### 2.8 V2.2 Tech Debt + V2.3 Aggressive Targets (reference)

Full details in:
- `docs/sprints/CONSTELLATION-V2.2-TECH-DEBT-ADDENDUM.md` — 55+ bugs, 28 crate adoptions, 15 P0 security in L1 effect crates
- `docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md` — engine ≤100k LOC, zero-unwrap policy, blake3 cache

Key targets for Phase 12:
- **Engine ≤100k LOC** — Phase 12 extracts ~28k from engine's 160k → ~132k (contributes ~47% of the target)
- **Zero unwrap** — New code must use `?` operator. Existing `.unwrap()` in moved code → replace with `map_err`

---

## 3. THE PHASE — Phase 12: nika-builtin extraction

### 3.0 Goal

```
Before:  nika-engine/src/runtime/builtin/     ~31,000 LOC (24 files + 8 data/ + 10 media/)
After:   nika-builtin/                        ~28,000 LOC
         nika-engine/src/runtime/builtin/     ~3,000 LOC (router glue + engine bridges)
```

### 3.1 Full tool inventory (63 tools)

| Tier | Count | Tools |
|------|-------|-------|
| **Core** | 7 | sleep, log, emit, assert, prompt, run, complete |
| **Data** | 13 | jq, tree_data, inject, map, filter, group_by, enrich, json_merge, json_diff, set_diff, zip, chunk, token_count |
| **Data Sprint 2** | 6 | json_verify, yaml_validate, locale_lookup, aggregate, json_flatten, json_unflatten |
| **File** | 5 | read, write, edit, glob, grep |
| **Introspection** | 6 | cost, records, dag_info, task_status, threads, orchestrate |
| **Media T1** (always-on) | 5 | import, decode, dimensions, thumbhash, dominant_color |
| **Media T2** (core) | 6 | thumbnail, convert, strip, metadata, optimize, svg_render |
| **Media T3** (opt-in) | 13 | phash, compare, pdf_extract, chart, provenance, verify, qr_validate, quality, html_to_md, css_select, extract_metadata, extract_links, readability, pipeline |
| **Adapters** | 2 | rig_adapter, file_adapter |

### 3.2 Blocker analysis with resolution paths (NO skips)

| ID | Blocker | Resolution |
|----|---------|------------|
| **B1** | Every builtin uses `crate::error::NikaError` | **Define `BuiltinError` in nika-kernel/src/builtin.rs** with variants: InvalidArgs, Io, Parse, Timeout, Schema, Denied, Other. `impl From<BuiltinError> for NikaError` in nika-engine. Every tool returns `Result<String, BuiltinError>`. |
| **B2** | `run.rs` depends on `crate::runtime::Runner` | **Define `RunExecutor` trait in nika-kernel/src/scope.rs**: `async fn run_workflow(path, inputs, depth) -> Result<Value, BuiltinError>`. nika-engine provides `EngineRunExecutor` newtype wrapping `Runner`. Cycle broken. |
| **B3** | `records.rs`, `router.rs` take `Arc<RunContext>` | **Use splinter traits already in nika-kernel/src/scope.rs** (currently defined but unused): `TaskResults`, `RecordStore`, `BindingScope`, `MediaStaging`, `VaultLookup`, `InvocationContext`. Each tool takes only the traits it needs. |
| **B4** | `rig_adapter.rs`, cost take owned `EventLog` | **Use `Arc<dyn EventEmitter>`** — blanket impl shipped in Phase 5.1. Replace field types. |
| **B5** | Media tools take `Arc<MediaToolContext>` | **Define `MediaContext` trait in nika-kernel/src/scope.rs**: `fn cas() -> &dyn BlobStore`, `fn compute_pool() -> &ComputePool`, `fn budget() -> &Budget`. Concrete impl stays in nika-engine. |
| **B6** | File tools wrap `ReadTool`/`WriteTool` + `ToolContext` | **Rewrite** to use `Arc<dyn Filesystem>` from nika-fs. Move `ToolContext` (permission mode + working dir) to nika-builtin. Shield's `check_path_readable` MUST survive the move (blocks untrusted agents from nika.toml etc.). |
| **B7** | `prompt.rs` depends on `PolicyEnforcer`, `HitlHandler` | **Define `HitlPrompt` trait in nika-kernel**: `async fn ask(message, default) -> Result<String, BuiltinError>`. nika-engine provides impl via `HitlHandler`. |
| **B8** | `data/transform.rs` uses `TransformExpr` | Already in `nika_core::binding::transform`. Fix import path. |
| **B9** | `rig_adapter.rs:127` hardcodes orchestrate constant | Define `pub const ORCHESTRATE_TOOL_NAME: &str = "orchestrate"` in nika-core::catalogs::builtins. |
| **B10** | `run.rs` uses `parse_analyzed` | Already in nika-core. |
| **B11** | Media tools depend on `nika-media` | Both L2, parallel deps. `nika-builtin` declares `nika-media` as dep. Fine. |
| **B12** | `jq.rs` depends on `jaq-core`, `jaq-std`, `jaq-json` | Already at workspace level. Add to `nika-builtin/Cargo.toml`. |

### 3.3 Commit plan — 13 commits across 4 sessions

#### SESSION 6 — Foundation (4 commits, THIS SESSION)

**Commit 12.1 — nika-kernel additions (BuiltinError + splinter traits)**
- Create `nika-kernel/src/builtin.rs` with `BuiltinError` enum (7 variants + Display + Error)
- Extend `nika-kernel/src/scope.rs` with `RunExecutor`, `HitlPrompt`, `MediaContext` traits
- Add `impl From<BuiltinError> for NikaError` in nika-engine/src/error.rs
- **TDD tests:** `BuiltinError::Display` messages, `From` roundtrip, all 3 new traits are object-safe (compile-time check)
- **Agent:** spawn `spn-rust:rust-architect` BEFORE writing the traits (see 0.4)

**Commit 12.2 — Create nika-builtin crate skeleton**
- `tools/nika-builtin/Cargo.toml` with deps: nika-core, nika-kernel, nika-event, nika-fs, serde_json, async-trait, thiserror, tracing
- `src/lib.rs` with module stubs
- Add to workspace `members` in `tools/Cargo.toml`
- Move `nika-engine/src/runtime/builtin/trait.rs` → `nika-builtin/src/builtin_trait.rs`
- **Seal the trait** via private-mod pattern:
  ```rust
  mod sealed { pub trait Sealed {} }
  pub trait BuiltinTool: sealed::Sealed + Send + Sync { … }
  ```
- **TDD test:** `assert_send_sync::<Arc<dyn BuiltinTool>>()`

**Commit 12.3 — Move 5 pure core tools (sleep, log, emit, assert, complete)**
- Zero coupling beyond `BuiltinError` (the cleanest first move)
- Each tool → pub mod in nika-builtin
- Re-export from nika-builtin::lib
- In nika-engine: re-export from nika-builtin (backward compat, temporary)
- **Verify:** existing test count unchanged (tests move WITH the tools)

**Commit 12.4 — Move 13 data tools (entire data/ directory)**
- Move `data/{aggregate,io,jq,json_diff,merge,text,transform}.rs` + `data/mod.rs`
- Fix import: `crate::binding::TransformExpr` → `nika_core::binding::transform::TransformExpr`
- jaq-core, jaq-std, jaq-json deps added to nika-builtin
- **Verify:** test count unchanged

#### SESSION 7 — Sprint 2 + Introspection + File (4 commits)

**Commit 12.5** — 6 Sprint 2 data tools (aggregate, json_verify, yaml_validate, locale_lookup, json_flatten, json_unflatten)
**Commit 12.6** — 6 introspection tools (cost, records, dag_info, task_status, threads, orchestrate) — use `Arc<dyn EventEmitter>` + `Arc<dyn RecordStore>`
**Commit 12.7** — 5 file tools + `ToolContext` — rewrite to `Arc<dyn Filesystem>`, preserve `check_path_readable` Shield integration
**Commit 12.8** — RunTool: define `EngineRunExecutor` in nika-engine wrapping `Runner`, `run.rs` takes `Arc<dyn RunExecutor>`

#### SESSION 8 — Media (3 commits)

**Commit 12.9** — MediaContext trait + 5 Tier 1 tools (import, decode, dimensions, thumbhash, dominant_color)
**Commit 12.10** — 6 Tier 2 media tools (thumbnail, convert, strip, metadata, optimize, svg_render)
**Commit 12.11** — 13 Tier 3 media + rig_adapter + file_adapter

#### SESSION 9 — Router + cleanup (2 commits)

**Commit 12.12** — Migrate `BuiltinToolRouter` constructor: `Arc<dyn EventEmitter>` + `Arc<dyn Filesystem>` + `Arc<dyn MediaContext>` + `Arc<dyn RunExecutor>` + `Arc<dyn HitlPrompt>` + `Arc<dyn TaskResults>` + `Arc<dyn RecordStore>`
**Commit 12.13** — Cleanup: delete moved files from nika-engine, verify engine shrunk ~28k LOC

### 3.4 Per-commit TDD cycle (follow EXACTLY)

```
┌──────────────────────────────────────────────────────────┐
│  1. Announce skill: "test-driven-development"            │
│  2. Write failing test (compile error OR assertion)      │
│  3. Run test — PROVE it fails (show output)              │
│  4. Implement (smallest possible diff)                   │
│  5. Follow compile errors one at a time                  │
│  6. Run test — PROVE it passes (show output)             │
│  7. Announce skill: "verification-before-completion"     │
│  8. Run: cargo test --workspace --lib                    │
│  9. Run: cargo clippy --all-targets --all-features       │
│ 10. Show BOTH outputs. Compare test count to baseline.   │
│ 11. Spawn feature-dev:code-reviewer agent with diff      │
│ 12. Address real issues; ignore style nits                │
│ 13. git add <specific files> (NEVER git add -A)          │
│ 14. git commit with Nika 🦋 co-author                    │
└──────────────────────────────────────────────────────────┘
```

### 3.5 Risk register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| BuiltinTool trait not object-safe | MEDIUM | Blocks everything | Spawn rust-architect BEFORE writing, add compile-time test |
| Cyclic dep nika-builtin ↔ nika-engine | HIGH | Compile fail | RunExecutor trait in nika-kernel breaks cycle; NEVER import nika-engine from nika-builtin |
| Shield regression (check_path_readable) | MEDIUM | Security hole | Dedicated test: untrusted agent blocked from reading nika.toml after move |
| jaq-core version mismatch | LOW | Compile fail | Use workspace = true for all jaq deps |
| Media feature gates don't carry through | MEDIUM | Missing tools | Carry `#[cfg(feature = "...")]` gates from nika-engine to nika-builtin |
| Test count regresses after move | LOW | Red baseline | Count tests before and after EVERY commit |

### 3.6 Decision tree

```
Compile error after moving a tool?
├── Missing import?          → Add it to nika-builtin's module
├── Private item referenced? → Make it pub(crate) in source, or move it too
├── Trait not in scope?      → Import from nika-kernel or nika-core
├── Feature gate missing?    → Add to nika-builtin Cargo.toml [features]
└── Cyclic dependency?       → STOP; spawn rust-architect agent

Test regression?
├── Same test failing before? → Pre-existing, not your commit
├── New test failing?         → Use systematic-debugging skill
└── Flaky?                    → Use root-cause-tracing skill

Clippy warning?
├── In moved code?            → Fix it (improve on the way)
├── In new code?              → Fix it (zero warnings policy)
└── In unrelated code?        → Leave it (pre-existing baseline)
```

---

## 4. CROSS-CUTTING RULES (apply to EVERY commit)

### 4.1 Test-driven development — NON-NEGOTIABLE
Every commit starts with a test that fails.

### 4.2 Verification before completion — NON-NEGOTIABLE
`cargo test --workspace --lib` + `cargo clippy --all-targets --all-features` before every commit. Show output. Compare pass count.

### 4.3 Git discipline
- 1 logical change = 1 commit
- `type(scope): description` + `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude, NEVER Anthropic
- `git add <specific files>` — NEVER `git add -A` or `git add .`
- Do NOT push unless explicitly asked
- Pre-commit hooks: rustfmt + clippy — if they fail, fix the issue, NEVER `--no-verify`

### 4.4 What you may NOT touch
- The 5 verbs (`infer`, `exec`, `fetch`, `invoke`, `agent`) — NEVER change
- Schema `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Shield files (merged in Sprint 2, stable) — only if directly required
- nika-cli Phase 15 modules — stable, don't refactor

### 4.5 What you MUST touch (perfection > timing)
- Dead scaffolding → wire it, don't delete
- Bugs in touched code → fix in same commit
- Better names → rename
- Unused imports after a move → remove
- Clippy warnings in touched code → fix
- `.unwrap()` in production code → replace with `map_err` + `?`
- NO `// TODO: fix later` — fix now or track in handoff

---

## 5. NUMBERS

| Metric | S0 baseline | S4 end | **S5 end (now)** | Target S9 |
|--------|-------------|--------|------------------|-----------|
| Workspace crates | 17 | 24 | **25** | **26** (+nika-builtin) |
| Tests | 10,666 | 10,790 | **10,833** | ~10,900 |
| God files >1500 LOC | 5 | 2 | **2** | 1 |
| Traits defined | 0 | 10 | **10** | **14** (+4 builtin) |
| Production trait impls | 0 | 6 | **6** | **7** (+RunExecutor) |
| Clippy warnings | 0 | 0 | **0** | 0 |
| `nika/src/main.rs` LOC | 5,530 | 5,530 | **2,043** | 2,043 |
| `nika-engine/` LOC | 160k | 160k | **160k** | **~132k** |
| `nika-builtin/` LOC | — | — | — | **~28k** |
| `nika-cli/` LOC | 8k | 8k | **12.5k** | 12.5k |
| `nika-macros/` LOC | — | — | **554** | ~600 |

---

## 6. TL;DR FOR THE NEXT AGENT

> **Baseline:** `ef17f7e9a`, 10,833 tests, 25 crates, clippy clean.
>
> **Read:** this handoff → `nika/CLAUDE.md` → `tools/nika/CLAUDE.md` → `tools/nika-engine/ARCHITECTURE.md`.
>
> **Execute Phase 12 SESSION 6** — 4 commits:
> 1. `BuiltinError` + splinter traits in nika-kernel (spawn `rust-architect` FIRST)
> 2. `nika-builtin` crate skeleton with sealed trait
> 3. Move 5 pure core tools (sleep, log, emit, assert, complete)
> 4. Move 13 data tools (jq, map, filter, merge, etc.)
>
> **Wire `#[builtin_tool]` macro** when moving tools — `trait_path` and `error` attrs need overriding for nika-builtin paths.
>
> **Every commit:** TDD (section 3.4), verify (10,833+ tests, 0 clippy), code review agent, Nika 🦋 co-author.
>
> **Perfection > timing.** Every blocker has a resolution path (section 3.2). No shortcuts.
>
> GOOO 🚀
