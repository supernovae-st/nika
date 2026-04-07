# Nika Architecture Roadmap — Post-Launch Refactoring

> **Version**: v0.75.0 | **Date**: 2026-04-07 | **Tests**: 10,365 GREEN
> **Source**: 6 parallel Rust architect agents + 3 code review agents
> **Scope**: Post-launch quality improvements. Nothing here blocks May 5.

---

## Executive Summary

Nika is **feature-complete for launch**. The codebase is healthy (10,365 tests, clippy clean, 0 blockers). But 6 architecture debts exist that will slow development velocity if not addressed:

| Debt | Impact | Effort | Phase |
|------|--------|--------|-------|
| God Crate: nika-engine 158K LOC | Compile bottleneck, all crates wait | 2 weeks | Phase 1 |
| NikaError 103 flat variants | 5 files to add 1 error | 4-6 weeks | Phase 2 |
| Runner monolith 2,262 LOC | Hard to navigate, test, review | 1 week | Phase 3 |
| Provider type zoo (4 enums) | 20 touch points for new provider | 1 week | Phase 4 |
| Serve/daemon dual schedulers | 2 DBs, duplicate job logic | 2 weeks | Phase 5 |
| Parser/analyzer file size | 4,196 + 5,462 LOC single files | 1 week | Phase 6 |

**Total**: ~10 weeks of focused work, parallelizable to ~6 weeks with 2 engineers.

---

## Phase 1: Break the God Crate (nika-engine → 5 crates)

### Why First
nika-engine at 158K LOC is 39% of the codebase. A single-line change forces 35s recompilation. Every downstream crate (cli, tui, serve, lsp) waits. This is the highest-ROI refactor.

### Target Architecture

```
BEFORE:                          AFTER:
nika-engine (158K LOC)           nika-engine (100K LOC, slim)
  ├── ast/ (22K)          →      nika-ast (33K) — parsing, binding, lowering
  ├── binding/ (11K)      →        (merged into nika-ast)
  ├── provider/ (9K)      →      nika-provider (10K) — LLM abstraction
  ├── secrets/ (1K)       →        (merged into nika-provider)
  ├── dag/ (5K)           →      nika-dag (7K) — graph structure, validation
  ├── store/ (2K)         →        (merged into nika-dag)
  └── runtime/ (72K)             (stays in nika-engine-slim)
```

### Execution Plan

**Step 1.1: Extract nika-provider (~3 days)**
```
Files to move:
  nika-engine/src/provider/       → nika-provider/src/
  nika-engine/src/secrets/        → nika-provider/src/secrets/

New Cargo.toml deps:
  nika-core, nika-vault, rig-core, reqwest

Key challenge:
  Invert engine→daemon dependency via trait:
    nika-core: define `trait SecretProvider { fn get_secret(&self, name: &str) -> Option<String>; }`
    nika-daemon: impl SecretProvider for VaultSecretProvider
    nika-provider: depends on nika-core (trait), NOT nika-daemon
    nika binary: constructs provider with daemon's impl

Verification:
  cargo tree -p nika-provider --no-dedupe | grep -c "nika-engine"  # MUST BE 0
  cargo tree -p nika-engine --no-dedupe | grep -c "rig-core"       # MUST BE 0
  cargo test --workspace --lib                                      # ALL PASS
```

**Step 1.2: Extract nika-dag (~2 days)**
```
Files to move:
  nika-engine/src/dag/            → nika-dag/src/
  nika-engine/src/store/          → nika-dag/src/store/

New Cargo.toml deps:
  nika-core, nika-event, petgraph

No external dep changes — petgraph moves from engine to dag.
```

**Step 1.3: Extract nika-ast (~3 days)**
```
Files to move:
  nika-engine/src/ast/            → nika-ast/src/
  nika-engine/src/binding/        → nika-ast/src/binding/

New Cargo.toml deps:
  nika-core

Key challenge:
  nika-ast/lower.rs imports types from runtime (Task, Workflow).
  These need to stay in nika-engine OR be defined in nika-ast with
  the engine re-exporting them. Cleanest: define in nika-ast, engine re-exports.
```

**Step 1.4: Verify compilation parallelism**
```
cargo build --timings 2>&1 | grep "nika-"
# Expect: nika-ast, nika-provider, nika-dag compile in PARALLEL
# Expect: nika-engine-slim starts AFTER all three finish
# Target: critical path 65s → 46s
```

### Session Prompt for Phase 1

```
> Copy-paste into a Claude Code session.
> Mode: TDD, 1 crate extraction = 1 PR.

WHO: Rust architect on Nika (v0.75.0, 17 crates, /Users/thibaut/dev/supernovae/nika)

TASK: Extract nika-provider from nika-engine.

RULES:
- cargo test --workspace --lib BEFORE and AFTER every change
- cargo tree -p nika-provider --no-dedupe | grep "nika-engine" MUST BE 0
- Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
- 1 logical change = 1 commit

STEPS:
1. Create tools/nika-provider/Cargo.toml (workspace member)
2. Move tools/nika-engine/src/provider/ → tools/nika-provider/src/
3. Move tools/nika-engine/src/secrets/ → tools/nika-provider/src/secrets/
4. Define SecretProvider trait in nika-core
5. Update all imports in nika-engine to use nika-provider
6. Verify: cargo test --workspace --lib
7. Verify: cargo tree shows no circular deps

READ FIRST:
  tools/nika-engine/src/provider/mod.rs
  tools/nika-engine/src/provider/rig/mod.rs (1,954 LOC — the dispatch_rig! macro)
  tools/nika-engine/src/secrets/ (1,060 LOC)
  tools/nika-engine/Cargo.toml (find rig-core, reqwest deps)
```

---

## Phase 2: NikaError Domain Hierarchy

### Why
103 flat variants, quintuple dispatch. Adding 1 error = editing 5 files.

### Target

```
NikaError (top-level, ~20 variants)
  ├── WorkflowError (NIKA-001..009) — 6 variants
  ├── SchemaError (NIKA-010..019) — 2 variants
  ├── DagError (NIKA-020..029) — 6 variants
  ├── ProviderError (NIKA-030..039) — 8 variants
  ├── BindingError (NIKA-040..049) — 7 variants
  ├── PathError (NIKA-050..059) — 5 variants
  ├── OutputError (NIKA-060..069) — 3 variants
  ├── WithBlockError (NIKA-070..083) — 7 variants
  ├── IoError (NIKA-090..098) — 7 variants
  ├── McpError (NIKA-100..110) — 11 variants (unify with nika-mcp)
  ├── AgentError (NIKA-112..116) — 5 variants
  ├── ToolError (NIKA-200..213) — 4 variants
  ├── ArtifactError (NIKA-280..285) — 4 variants
  ├── StructuredOutputError (NIKA-300..303) — 4 variants
  └── ~15 singleton variants (Io, Json, Timeout, etc.)
```

### Execution Plan

**Step 2.0: Delete dead error_domains.rs (15 min)**
`tools/nika-engine/src/error_domains.rs` has 4 sub-enums that NO production code uses. Delete it.

**Step 2.1: Extract ProviderError (1 day)**
```
Create: tools/nika-engine/src/error/provider.rs
Move: 8 variants (NIKA-030..037) from error.rs
Add: #[error(transparent)] Provider(#[from] ProviderError) to NikaError
Update: code(), is_recoverable() to delegate
Test: cargo test -p nika-engine --lib -- error
```

**Step 2.2: Unify McpError (1 day)**
```
nika-mcp::McpError already exists with 14 variants.
Make NikaError::Mcp(#[from] nika_mcp::McpError) — eliminates 57-line From impl.
```

**Step 2.3-2.N: One domain per PR**
Continue with DagError, BindingError, WithBlockError, etc. Each is independent.

### Session Prompt for Phase 2

```
> Copy-paste into a Claude Code session.
> Mode: TDD, 1 domain = 1 commit.

WHO: Rust engineer on Nika (v0.75.0)

TASK: Extract ProviderError sub-enum from NikaError.

READ FIRST:
  tools/nika-engine/src/error.rs (2,659 LOC — find NIKA-030..037 variants)
  tools/nika-engine/src/error_domains.rs (dead code — delete first)

STEPS:
1. Delete error_domains.rs (dead code). Commit: "chore: delete abandoned error_domains.rs"
2. Create src/error/mod.rs + src/error/provider.rs
3. Move NIKA-030..037 variants to ProviderError enum with #[error], #[diagnostic], code(), is_recoverable()
4. Add NikaError::Provider(#[from] ProviderError)
5. Update NikaError::code() to delegate: Self::Provider(e) => e.code()
6. cargo test --workspace --lib
7. Commit: "refactor(error): extract ProviderError sub-enum (NIKA-030..037)"
```

---

## Phase 3: Runner Module Decomposition

### Why
runner/mod.rs at 2,262 LOC has 7+ responsibilities tangled together. The `run()` method alone is 1,043 LOC.

### Target

```
runner/
  mod.rs           ~250 LOC  — Runner struct, constructors, pub run()
  builder.rs       ~180 LOC  — with_*, quiet, pause/resume, accessors
  scheduler.rs     ~120 LOC  — get_ready_tasks, all_done, deadlock detection
  init.rs          ~260 LOC  — init_run (context, inputs, skills, DAG layers)
  dag_loop.rs      ~350 LOC  — select! loop, cancellation, timeout
  spawn.rs         ~350 LOC  — for_each spawning, regular task spawning
  aggregation.rs   ~180 LOC  — for_each result merge, partial success
  finalize.rs      ~200 LOC  — media integrity, artifacts, trace, summary
  lockfile.rs      ~100 LOC  — LockfileGuard (RAII, platform-specific)
  helpers.rs       ~70  LOC  — value_to_array, detect_first_configured_provider
  tests.rs         (existing — already split)
```

### Execution: 8 moves, ordered by risk

| # | Module | LOC | Risk | Notes |
|---|--------|-----|------|-------|
| 1 | lockfile.rs | 90 | None | Pure RAII, zero deps on Runner |
| 2 | helpers.rs | 65 | None | Free functions |
| 3 | builder.rs | 170 | Low | Simple delegators |
| 4 | scheduler.rs | 110 | Low | Query methods |
| 5 | finalize.rs | 180 | Low | Called once, clear boundary |
| 6 | init.rs | 250 | Medium | Heavy self-mutation |
| 7 | aggregation.rs | 180 | Medium | Post-collection processing |
| 8 | spawn.rs + dag_loop.rs | 700 | High | Core async, many captures |

### Session Prompt for Phase 3

```
> Copy-paste into a Claude Code session.

WHO: Rust engineer on Nika (v0.75.0)

TASK: Extract lockfile.rs, helpers.rs, builder.rs from runner/mod.rs (moves 1-3).

READ FIRST:
  tools/nika-engine/src/runtime/runner/mod.rs (2,262 LOC)
  Lines 44-134: LockfileGuard (→ lockfile.rs)
  Lines 136-200: Helper functions (→ helpers.rs)
  Lines 405-577: Builder methods (→ builder.rs)

RULES:
- Each extraction = 1 commit
- cargo test --workspace --lib between each move
- Use pub(super) for module-internal visibility
- Keep impl Runner blocks — Rust allows split across files

EXPECTED RESULT: runner/mod.rs drops from 2,262 → ~1,937 LOC (-325).
```

---

## Phase 4: Provider declare_rig_providers! Macro

### Why
4 parallel enums. 20 touch points to add a provider. The dispatch_rig! macro is correct but should be part of a larger codegen strategy.

### Target

```rust
// tools/nika-engine/src/provider/rig/mod.rs

declare_rig_providers! {
    claude   => anthropic::Client, "anthropic", ProviderKind::Claude;
    openai   => openai::Client,    "openai",    ProviderKind::OpenAI;
    mistral  => mistral::Client,   "mistral",   ProviderKind::Mistral;
    groq     => groq::Client,      "groq",      ProviderKind::Groq;
    deepseek => deepseek::Client,  "deepseek",  ProviderKind::DeepSeek;
    gemini   => gemini::Client,    "gemini",    ProviderKind::Gemini;
    xai      => xai::Client,       "xai",       ProviderKind::XAi;
}
// Adding provider #10 = 1 line here
```

### What the macro generates
- RigProvider enum variants
- dispatch_rig! macro arms
- from_name() match arms
- name() match arms
- cost_provider_kind() match arms
- Debug impl
- Agent loop dispatch

### Additional cleanups
1. Merge `from_name()` and `from_name_with_key()` → single constructor with `Option<&str>`
2. Extract capabilities to KNOWN_PROVIDERS catalog (supports_vision, supports_thinking, etc.)
3. Merge ProviderKind into catalog (eliminate 4th enum)
4. Delegate `auto()` and `is_configured()` to catalog iteration

---

## Phase 5: Serve/Daemon Job Ownership

### Why
Two processes run independent job schedulers on separate SQLite databases. Schedule reconciler lives in serve but cron firing lives in daemon.

### Target Architecture

```
daemon (global, ~/.nika/)          serve (project-scoped)
  ├── Secrets (NikaVault)            ├── HTTP routing (Axum)
  ├── Cache (DashMap)                ├── Auth (Bearer/MultiKey)
  ├── Jobs (OWNS lifecycle)          ├── Rate limiting
  ├── Cron scheduler                 ├── SSE event proxy
  ├── Schedule reconciler ←NEW       ├── Artifact serving
  └── Watch (file events)            └── OpenAPI docs
                                     
  serve delegates job ops to daemon via IPC
  (or embeds daemon in-process when no socket found)
```

### Migration Steps
1. Extract serve's job execution into a `JobExecutor` trait (shared between serve and daemon)
2. Add DaemonClient to nika-serve — serve submits jobs via IPC when daemon is reachable
3. Move `reconcile_yaml_schedules` from serve to daemon (add `ReconcileSchedules` IPC message)
4. Serve's storage becomes read-only (queries through daemon)
5. Gate nika-engine dep in daemon behind `feature = ["embedded"]`

---

## Phase 6: Parser/Analyzer File Split

### Why
parser.rs (4,196 LOC) and analyze.rs (5,462 LOC) are single-file monoliths. Navigation is painful.

### Parser Split

```
raw/parser/
  mod.rs      ~300 LOC  — pub fn parse(), re-exports
  fields.rs   ~200 LOC  — get_string_field, get_u32_field, etc.
  verbs.rs    ~400 LOC  — parse_infer_action, parse_exec_action, etc.
  task.rs     ~400 LOC  — parse_task, parse_with_refs, parse_for_each
  workflow.rs ~400 LOC  — parse_mcp_config, parse_include, parse_inputs
  validate.rs ~200 LOC  — validate_verb_keys, validate_task_keys
  tests/      ~2,096 LOC — split by subject
```

### Analyzer: Pluggable Validation Passes (future)

```rust
trait AnalysisPass {
    fn name(&self) -> &'static str;
    fn run(&self, raw: &RawWorkflow, ctx: &mut AnalyzerContext);
}

fn default_passes() -> Vec<Box<dyn AnalysisPass>> {
    vec![
        Box::new(SchemaValidation),
        Box::new(FeatureGateValidation),
        Box::new(ModelNameValidation),
        // adding a check = adding 1 line here
    ]
}
```

This would also unify `nika check` validation with `nika lint` rules.

---

## Priority Matrix

```
                    IMPACT
                    HIGH ┃ Phase 1 (God Crate)    Phase 2 (Errors)
                         ┃
                    MED  ┃ Phase 4 (Providers)    Phase 5 (Serve/Daemon)
                         ┃
                    LOW  ┃ Phase 3 (Runner)       Phase 6 (Parser)
                         ┃
                         ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
                           LOW                MED               HIGH
                                          EFFORT
```

### Recommended Execution Order
1. **Phase 3** (1 week) — Runner decomposition, zero-risk warm-up
2. **Phase 1** (2 weeks) — God Crate extraction, highest compile-time ROI
3. **Phase 2** (4-6 weeks, background) — Error domains, 1-2 per week
4. **Phase 4** (1 week) — Provider macro, after Phase 1 (provider crate exists)
5. **Phase 6** (1 week) — Parser/analyzer split, whenever convenient
6. **Phase 5** (2 weeks) — Serve/daemon, most complex, last

---

## Verification Protocol

Every phase must maintain:

```bash
# Rust
cd tools && cargo test --workspace --lib              # 10,365+ tests, 0 failures
cd tools && cargo clippy --workspace -- -D warnings    # 0 warnings
cd tools && cargo fmt --all --check                    # clean

# TypeScript
cd editors/vscode && npm run compile                   # builds

# Crate boundary guards (after Phase 1)
cargo tree -p nika-provider --no-dedupe | grep -c "nika-engine"  # MUST BE 0
cargo tree -p nika-ast --no-dedupe | grep -c "nika-engine"       # MUST BE 0
cargo tree -p nika-dag --no-dedupe | grep -c "nika-engine"       # MUST BE 0

# Compile time benchmark
cargo build --timings 2>&1 | grep "nika-"
```

---

## Quick Wins (do in any session, anytime)

These don't need a dedicated phase — just fix when touching the file:

| Fix | File | Effort |
|-----|------|--------|
| Centralize VERB_KEYS constant | parser.rs + validate | 15 min |
| Merge Runner constructors (with_event_log + with_policy) | runner/mod.rs | 30 min |
| Delete error_domains.rs (dead code) | error_domains.rs | 5 min |
| Add comment explaining `schedule: _` drop in lower.rs | lower.rs:61 | 2 min |
| Eliminate unlower() by making expand_includes work on AnalyzedWorkflow | lower.rs + include.rs | 2h |

---

*Generated: 2026-04-07 | Based on 6 Rust architect agents + 3 code review agents*
*All proposals verified against actual source code with exact LOC counts*
