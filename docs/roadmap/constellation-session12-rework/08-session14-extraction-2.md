# Session 14 — Infer + Agent Extraction, TaskExecutor Dissolution

> ⚠️ **SUPERSEDED 2026-04-11** — Session 14 HAS SHIPPED. This doc is the original ambitious S14 plan written 2026-04-10 before S13 completed. The actual S14 landed with a drastically reduced scope (5 commits Wave A–B + S14.5 hotfix, NOT the 20-commit Infer+Agent extraction this doc describes).
>
> **For the real S14 record, read:**
> - `20b-session14-scope-correction.md` — scope correction + Phase 0/1 findings + postmortem
> - `16-session-journal.md` (Session 14 entry, lines 287+) — full commit log with S14.5 hotfix
> - `23-session15-mega-prompt.md` — canonical S15 doc (S15 picks up the infer.rs bridge + MCP Pool + agent extraction that this doc originally scheduled for S14)
>
> **Historical value only** — preserved for the original design thinking on nika-shield crate, ProviderRegistry trait, SecurityContext extraction, etc.
>
> ---

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract `nika-verb-infer` (2157 LOC) + `nika-verb-agent` (602 LOC) into their own crates, delete `TaskExecutor` entirely, dissolve `nika-engine` toward a thin shim.

**Architecture:** Enrich `Provider` trait to cover vision/tool-use/structured-output, add `ProviderRegistry` trait in `nika-runtime`, create `nika-shield` L1 crate for SecurityContext/SpotlightFence/CanarySystem, move the entire `rig_agent_loop/` directory into `nika-verb-agent/src/agent_loop/` verbatim with import-path surgery, then wire both through the existing `dispatch()` function and delete `TaskExecutor`.

**Tech Stack:** Rust 1.75+ (AFIT), `async_trait` where `dyn` is needed, `tokio`, `rig-core`, `nika-kernel`, `nika-runtime`.

**Preconditions:** Sessions 12-13 complete. `nika-runtime` exists with `VerbCapabilities` + `dispatch()` wired for Exec, Fetch, Invoke. `nika-policy`, `nika-extract` live. `TaskExecutor` retains only `run_infer` + `run_agent`. Engine at ~144k LOC.

**Estimated:** 20 commits, ~14-18h over 2 working days.

---

## 0. Actual Code State (Verified by Research Agent)

Facts from reading the codebase, not assumptions:

**TaskExecutor fields (22 total) — `infer.rs` accesses 13 of them:** `shield`, `event_log`, `skills_map`, `skills_base_dir`, `skill_injector`, `default_provider`, `default_model`, `custom_endpoints`, `get_rig_provider()`, `policy_enforcer`, `cancel_token`, `cas`, `workflow_base_dir`.

**StructuredOutputEngine:** `nika-engine/src/runtime/structured_output.rs`. Standalone struct, NOT dependent on `TaskExecutor` fields. Takes `Arc<EventLog>` in constructor. **Does not need to be moved before infer extraction** — nika-verb-infer re-uses it from nika-engine's public API.

**RigProvider's non-trait methods used by infer.rs (8):** `infer_vision()`, `infer_with_tools()`, `infer_with_options()`, `infer_stream_with_options()`, `supports_native_structured_output()`, `is_anthropic()`, `supports_vision()`, `supports_thinking()`. All `impl RigProvider` blocks, NOT on the `Provider` kernel trait.

**rig_agent_loop:** `mod.rs` + `chat.rs` + `providers.rs` + `streaming.rs` + `thinking.rs` + `types.rs` + `tests.rs` + `tests_shield_mcp_wrap.rs` = ~2500 LOC total. `RigAgentLoop` struct has 14 fields, holds `AgentMediaStaging`, `rig::message::Message` history, `DynamicSubmitTool`, `SkillInjector`, `NikaMcpTool`, `NikaMcpToolDef`. Zero kernel-trait dependency today.

**decompose.rs:** 3 strategies (Semantic via MCP, Static via binding resolution, Nested BFS via MCP). 352 LOC. Uses `self.mcp_pool` and `self.event_log`. Logically a runner concern, not a verb.

---

## 1. The Central Architectural Decision

**Problem in one sentence:** `infer.rs` uses `RigProvider` directly for 8 non-trait methods; `rig_agent_loop` has 14 fields anchored to concrete engine types; both cannot be cleanly extracted until the `Provider` trait is enriched.

**Decision: Two-wave extraction within Session 14.**

**Wave A — Prerequisite trait work (first half):** Enrich `Provider` trait + add `ProviderRegistry` + create `nika-shield`. This is the "make infer.rs trait-based" wave. Four commits.

**Wave B — Extraction (second half):** Once `infer.rs` calls only `Arc<dyn Provider>` (enriched) + `Arc<dyn ProviderRegistry>` + `&ShieldContext`, extract into `nika-verb-infer`. Extract `nika-verb-agent` with the agent_loop/ directory verbatim move.

**Wave C — Dissolution (final third):** Delete `TaskExecutor`, dissolve `nika-engine` orchestration code into `nika-runtime`.

---

## 2. Decisions Already Taken (Do Not Re-Debate)

### Decision 2.1 — RigAgentLoop placement: verbatim into `nika-verb-agent`

**Verdict:** Move the entire `rig_agent_loop/` directory into `nika-verb-agent/src/agent_loop/`. The crate is large (~2500 LOC) but self-contained. Precedent: `nika-verb-exec` wraps `nika-exec-runner`. Here we wrap the agent loop inside the verb crate. **No new intermediate crate** (`nika-agent-loop`).

Rejected: new `nika-agent-loop` L1 crate (one more Cargo.toml for no consumer); keeping in `nika-engine` (defeats the refactor).

### Decision 2.2 — InferCaps vs AgentCaps: independent, no verb-to-verb deps

**Initial thought:** `AgentCaps` embeds `InferCaps` and nika-verb-agent depends on nika-verb-infer.

**Revised after deeper reading:** `RigAgentLoop.run()` calls `rig_agent.chat(prompt)` which internally handles all multi-turn LLM calls via rig-core. The "inner infer" is rig-native, not a call back into `TaskExecutor::run_infer`. **Therefore InferCaps and AgentCaps remain independent. nika-verb-agent does NOT depend on nika-verb-infer.** No crate-level coupling between verbs.

### Decision 2.3 — decompose.rs placement: nika-runtime

`decompose.rs` uses `self.mcp_pool` + `self.event_log`. Logically part of DAG runner (expands `for_each: decompose:` specs). **Moves to nika-runtime as `nika_runtime::decompose::expand()` free function.** Not into a verb crate. Runner calls it when building iteration lists.

### Decision 2.4 — Shield placement: new `nika-shield` L1 crate

**Verdict:** Create `nika-shield` as a thin L1 crate in Wave A. ~500 LOC total. `SecurityPolicyConfig` stays in `nika-core`. `SecurityContext` + `SpotlightFence` + `CanarySystem` move to `nika-shield`. Both `nika-verb-infer` and `nika-verb-agent` depend on it without going through `nika-runtime`.

**DECIDE BEFORE STARTING:** Grep `use crate::event` in `canary.rs` and `spotlight.rs`. If they import `EventKind` via nika-engine's re-export, change to `nika_event::EventKind` directly before moving.

### Decision 2.5 — Error type aggregation: `RunError` in nika-runtime

After dissolution, `nika-runtime::RunError` aggregates all verb errors via `impl From<ExecError | FetchError | InvokeError | InferError | AgentError>`. `NikaError` in nika-engine (if the shim survives) does `impl From<RunError> for NikaError`. Long-term `NikaError` disappears and `RunError` IS the public error.

### Decision 2.6 — nika-engine post-extraction state: thin shim, not deletion

After Session 14, nika-engine retains:
- `provider/rig/` (concrete RigProvider + 6 submodules — still required by nika-verb-infer via trait bridge)
- `runtime/boot.rs` (BootSequence, used by nika-cli + nika-tui)
- `runtime/structured_output.rs` (StructuredOutputEngine, re-used by nika-verb-infer)
- `runtime/chat_workflow.rs` (used by nika-cli chat mode)
- Residual orchestration code not yet extracted

**Target: under 30k LOC.** Full dissolution is Phase 15+.

---

## 3. Wave A — Prerequisites (4 commits, ~3-4h)

### Commit W14-A1 — `feat(kernel): enrich Provider trait — vision, tool-use, structured-output`

**Files:**
- Modify: `tools/nika-kernel/src/provider.rs`

**Changes to `Provider` trait:**

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    // existing
    fn name(&self) -> &str;
    fn capabilities(&self) -> Option<ModelCapabilities>;
    async fn infer(&self, req: InferRequest) -> Result<InferResponse, ProviderError>;
    async fn infer_stream(&self, req: InferRequest) -> InferStream;

    // NEW — vision inference
    async fn infer_vision(&self, req: InferRequest) -> Result<InferResponse, ProviderError> {
        Err(ProviderError::Unsupported("infer_vision".into()))
    }

    // NEW — tool-use inference
    async fn infer_with_tools(
        &self,
        req: InferRequest,
        tools: Vec<ToolDef>,
        choice: ToolChoice,
    ) -> Result<InferResponse, ProviderError> {
        Err(ProviderError::Unsupported("infer_with_tools".into()))
    }

    // NEW — options override (model, max_tokens, temperature)
    async fn infer_with_options(
        &self,
        prompt: &str,
        opts: &InferOptions,
    ) -> Result<String, ProviderError> {
        Err(ProviderError::Unsupported("infer_with_options".into()))
    }

    // NEW — capability probes
    fn supports_vision(&self) -> bool { false }
    fn supports_native_structured_output(&self) -> bool { false }
    fn supports_thinking(&self) -> bool { false }
    fn is_anthropic_compatible(&self) -> bool { false }
}
```

`InferOptions`, `ToolDef`, `ToolChoice` move from `nika-engine/src/provider/rig/inference.rs` into `nika-kernel::provider`. Pure data, no I/O.

**TDD tests in `provider.rs` tests module:**
- `mock_provider_returns_unsupported_for_infer_vision` — default impl contract
- `mock_provider_capability_probes_default_false` — defaults
- 3-4 unit tests total

**Verification:**
```bash
cd tools && cargo test -p nika-kernel --lib provider::
cargo clippy -p nika-kernel --lib -- -D warnings
```

**Rollback:** `git reset --hard HEAD~1`

### Commit W14-A2 — `refactor(engine): impl Provider for RigProvider — fill enriched trait`

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/kernel_bridge.rs`

Fill all new trait methods on `impl Provider for RigProvider`, delegating to concrete methods:

```rust
#[async_trait]
impl Provider for RigProvider {
    // ... existing ...

    async fn infer_vision(&self, req: InferRequest) -> Result<InferResponse, ProviderError> {
        // translate InferRequest -> concrete vision call
        self.infer_vision_inner(req).await.map_err(Into::into)
    }

    async fn infer_with_tools(
        &self, req: InferRequest, tools: Vec<ToolDef>, choice: ToolChoice,
    ) -> Result<InferResponse, ProviderError> {
        self.infer_with_tools_inner(req, tools, choice).await.map_err(Into::into)
    }

    async fn infer_with_options(
        &self, prompt: &str, opts: &InferOptions,
    ) -> Result<String, ProviderError> {
        self.infer_with_options_inner(prompt, opts).await.map_err(Into::into)
    }

    fn supports_vision(&self) -> bool { self.supports_vision_inner() }
    fn supports_native_structured_output(&self) -> bool { self.supports_native_structured_output_inner() }
    fn supports_thinking(&self) -> bool { self.supports_thinking_inner() }
    fn is_anthropic_compatible(&self) -> bool { self.is_anthropic() }
}
```

Rename the concrete methods to `*_inner` so the trait methods shadow them. Existing inherent-method call sites in `infer.rs` keep working during migration.

**Verification:** full workspace `cargo test --workspace --lib` green. Golden tests green.

### Commit W14-A3 — `feat(runtime): ProviderRegistry trait + ProviderRegistryImpl`

**Files:**
- Create: `tools/nika-runtime/src/provider_registry.rs`
- Modify: `tools/nika-runtime/src/caps.rs` (add `provider_registry` field to `VerbCapabilities`)
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs` (TaskExecutor delegates `get_rig_provider()` to ProviderRegistryImpl)

```rust
// nika-runtime/src/provider_registry.rs

use nika_kernel::provider::Provider;
use std::sync::Arc;

pub trait ProviderRegistry: Send + Sync {
    fn get(&self, name: &str) -> Result<Arc<dyn Provider>, RegistryError>;
    fn default_provider(&self) -> &str;
    fn default_model(&self) -> Option<&str>;
    fn custom_endpoints(&self) -> &CustomEndpointMap;
}

pub struct ProviderRegistryImpl {
    cache: Arc<DashMap<String, Arc<dyn Provider>>>,
    default_provider: Arc<str>,
    default_model: Option<Arc<str>>,
    custom_endpoints: Arc<CustomEndpointMap>,
}

impl ProviderRegistry for ProviderRegistryImpl {
    fn get(&self, name: &str) -> Result<Arc<dyn Provider>, RegistryError> {
        if let Some(cached) = self.cache.get(name) {
            return Ok(Arc::clone(&cached));
        }
        let rig = RigProvider::from_name(name, &self.custom_endpoints)?;
        let arc: Arc<dyn Provider> = Arc::new(rig);
        self.cache.insert(name.to_string(), Arc::clone(&arc));
        Ok(arc)
    }
    // ...
}
```

TaskExecutor's `get_rig_provider()` becomes a thin wrapper on `registry.get()`. No behavior change.

**Verification:** existing provider-cache tests in `executor/tests.rs` still green.

### Commit W14-A4 — `feat(shield): create nika-shield L1 crate`

**Files:**
- Create: `tools/nika-shield/Cargo.toml`
- Create: `tools/nika-shield/src/lib.rs`
- Create: `tools/nika-shield/src/context.rs` (moved from `nika-engine/src/runtime/shield.rs`)
- Create: `tools/nika-shield/src/spotlight.rs` (moved)
- Create: `tools/nika-shield/src/canary.rs` (moved)
- Modify: `Cargo.toml` workspace members
- Modify: `tools/nika-engine/src/runtime/shield.rs` → re-export shim: `pub use nika_shield::*;`
- Modify: `tools/nika-engine/src/runtime/spotlight.rs` → shim
- Modify: `tools/nika-engine/src/runtime/canary.rs` → shim

**Cargo.toml for nika-shield:**
```toml
[package]
name = "nika-shield"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true       # SecurityPolicyConfig, TrustLevel
nika-event.workspace = true      # EventLog + EventKind
thiserror.workspace = true
serde.workspace = true
rustc-hash.workspace = true
```

**DECIDE BEFORE STARTING:**
1. `grep -rn "use crate::event" nika-engine/src/runtime/canary.rs nika-engine/src/runtime/spotlight.rs` — change any `crate::event::EventKind` to `nika_event::EventKind`.
2. `grep -rn "crate::error" nika-engine/src/runtime/shield.rs nika-engine/src/runtime/canary.rs nika-engine/src/runtime/spotlight.rs` — if they touch `NikaError`, extract local error types (`ShieldError`) to break the dep.

**Verification:**
```bash
cargo test -p nika-shield --lib
cargo test --workspace --lib
cargo tree -p nika-shield --edges normal | grep -v "nika-core\|nika-event" # should be empty of other nika-* deps
```

---

## 4. Wave B1 — nika-verb-infer Extraction (5 commits, ~4-5h)

### Commit W14-B1 — `feat(verb-infer): create nika-verb-infer crate skeleton`

**Files:**
- Create: `tools/nika-verb-infer/Cargo.toml`
- Create: `tools/nika-verb-infer/src/lib.rs`
- Create: `tools/nika-verb-infer/src/error.rs`
- Create: `tools/nika-verb-infer/src/caps.rs` (re-export InferCaps from nika-runtime)
- Create stub modules: `prompt.rs`, `vision.rs`, `guardrails.rs`, `callbacks.rs`, `structured.rs`, `run.rs`

**Cargo.toml:**
```toml
[package]
name = "nika-verb-infer"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
nika-core.workspace = true
nika-kernel.workspace = true
nika-event.workspace = true
nika-shield.workspace = true
nika-runtime.workspace = true    # InferCaps
nika-engine.workspace = true     # TEMP — see doc comment
rig-core.workspace = true        # TEMP — for vision content blocks until Provider trait subsumes
tokio.workspace = true
tracing.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
```

**Justify the `nika-engine` dep explicitly in the file's doc comment:** "TEMP: re-uses `StructuredOutputEngine` and `provider::rig` types until Phase 15 extracts them. Not a clean dep graph — it is the honest cost of not bundling Phase 15 work into Session 14."

**InferCaps in `nika-runtime/src/caps.rs`:**

```rust
pub struct InferCaps {
    pub provider_registry: Arc<dyn ProviderRegistry>,
    pub shield: nika_shield::ShieldContext,
    pub skill_injector: Arc<SkillInjector>,
    pub skills_map: Arc<HashMap<String, String>>,
    pub skills_base_dir: PathBuf,
    pub blob_store: Arc<dyn BlobStore>,
    pub event_log: EventLog,
    pub cancel_token: CancellationToken,
    pub workflow_base_dir: PathBuf,
    pub policy_enforcer: Arc<RwLock<PolicyEnforcer>>, // from nika-policy
}
```

**Verification:** `cargo check -p nika-verb-infer` green (empty stubs compile).

### Commit W14-B2 — `feat(verb-infer): implement prompt.rs, vision.rs, guardrails.rs, callbacks.rs`

**prompt.rs** — move from `infer.rs` lines 105-377:
- Spotlight wrapping loop (105-173)
- `template_resolve` calls (179-200)
- Skills injection (208-252)
- Canary injection (253-260)
- Schema loading (280-355)
- Context assembly event emit (377)

**vision.rs** — move `run_infer_vision()` (lines 1647-1910):
- Takes `Arc<dyn BlobStore>` (kernel trait) + `Arc<dyn Provider>` (enriched with `infer_vision()`).
- `detect_image_media_type` helper moves here from `executor/verbs.rs`.
- Content block resolution: resolve CAS hashes to base64 via BlobStore.

**guardrails.rs** — move `check_infer_guardrails()` (lines ~1960-2000):
- 4 guardrail types: length, schema, regex, llm.
- Pure evaluation, no I/O except the `llm` type which takes `&dyn Provider`.
- Takes `InferParams` + output `&str` + `&dyn Provider`, returns `Result<(), InferError>`.

**callbacks.rs** — move `make_infer_callback()` (lines 48-72):
- Now takes `Arc<dyn Provider>` not `&RigProvider`.
- Body: closure that does `provider.infer_with_options(prompt, opts).await`.

**Verification:** `cargo test -p nika-verb-infer --lib` green, all submodules compile.

### Commit W14-B3 — `feat(verb-infer): implement structured.rs and main run() function`

**structured.rs** — thin wrapper around `StructuredOutputEngine`:
- Constructs engine from nika-engine's public API.
- Attaches `make_infer_callback()` result.
- Attaches `workflow_base_dir`.
- Does NOT move the StructuredOutputEngine struct itself (too much scope).

**run.rs** — main `pub async fn run()`:

```rust
pub async fn run(
    caps: &InferCaps,
    task_id: &Arc<str>,
    infer: &InferParams,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    output_policy: Option<&OutputPolicy>,
) -> Result<String, InferError> {
    // 1. Resolve provider via caps.provider_registry.get(provider_name)?
    // 2. Apply shield (spotlight + canary) via prompt::apply_shield(&caps.shield, ...)
    // 3. Template-resolve the prompt via prompt::resolve(bindings, ctx)
    // 4. Inject skills via prompt::inject_skills(&caps.skill_injector, ...)
    // 5. Build system prompt via prompt::assemble_system(...)
    // 6. Dispatch to streaming vs non-streaming vs vision vs mock fast-path
    // 7. Apply structured output via structured::apply(engine, ...)
    // 8. Apply guardrails via guardrails::check(...)
    // 9. Emit ProviderResponded event + return output
}
```

Mirrors `TaskExecutor::run_infer()` but calls free functions. ~300 LOC total (vs 2157 monolith).

**Verification:** all existing infer tests still green via temporary bridge in engine.

### Commit W14-B4 — `feat(runtime): wire dispatch() TaskAction::Infer arm`

**Files:**
- Modify: `tools/nika-runtime/src/dispatch.rs`
- Modify: `tools/nika-runtime/src/caps.rs` (add `infer_caps()` builder on `VerbCapabilities`)

```rust
// nika-runtime/src/dispatch.rs
match &task.action {
    TaskAction::Exec(p) => nika_verb_exec::run(p, bindings, rc, vc.exec_caps()).await.map_err(Into::into),
    TaskAction::Fetch(p) => nika_verb_fetch::run(p, bindings, rc, vc.fetch_caps()).await.map_err(Into::into),
    TaskAction::Infer(p) => nika_verb_infer::run(&vc.infer_caps(), task_id, p, bindings, rc, output_policy)
        .await
        .map(VerbOutput::Infer)
        .map_err(Into::into),
    TaskAction::Invoke(p) => nika_verb_invoke::run(p, bindings, rc, vc.invoke_caps()).await.map_err(Into::into),
    TaskAction::Agent(_) => todo!("wired in W14-B6"),
}
```

**Verification:** `cargo test --workspace --lib` + golden tests for infer still pass via Runner path.

### Commit W14-B5 — `refactor(engine): TaskExecutor::run_infer delegates + delete infer.rs (-2157 LOC)`

**Two-step commit (bridge first, delete second):**

Step 1 (bridge): `TaskExecutor::run_infer()` body becomes:
```rust
pub(super) async fn run_infer(&self, ...) -> Result<String, NikaError> {
    let caps = self.build_infer_caps();
    nika_verb_infer::run(&caps, task_id, infer, bindings, ctx, output_policy)
        .await
        .map_err(Into::into)
}
```

Run full test suite + golden. Must be green.

Step 2 (delete): `chore(engine): delete nika-engine/src/runtime/executor/infer.rs (-2157 LOC)` — mod declaration removed from `executor/mod.rs`, file deleted.

**Verification:**
```bash
find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED: ~142,000 total (down from ~144,000)
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
```

---

## 5. Wave B2 — nika-verb-agent Extraction (4 commits, ~4-5h)

### Pre-work W14-C0 — Import-path mapping table (NO COMMIT, ~30min)

Before writing any code, grep every file in `rig_agent_loop/` for `use crate::` and build a mapping table:

| Old path | New path |
|---|---|
| `use crate::provider::rig::RigProvider` | `use nika_engine::provider::rig::RigProvider` |
| `use crate::runtime::SkillInjector` | `use nika_engine::runtime::SkillInjector` |
| `use crate::mcp::McpClient` | `use nika_engine::mcp::McpClient` |
| `use crate::event::{EventKind, EventLog}` | `use nika_event::{EventKind, EventLog}` |
| `use crate::runtime::shield::SecurityContext` | `use nika_shield::ShieldContext` |
| `use crate::runtime::limit_tracker::LimitTracker` | `use nika_engine::runtime::limit_tracker::LimitTracker` |
| `use crate::runtime::submit_tool::DynamicSubmitTool` | `use nika_engine::runtime::submit_tool::DynamicSubmitTool` |

Save the table as a scratch file in `docs/plans/constellation-session12-rework/scratch-s14-import-map.md`. Delete after W14-C2 commits.

### Commit W14-C1 — `feat(verb-agent): create crate + verbatim move of agent_loop/`

**Files:**
- Create: `tools/nika-verb-agent/Cargo.toml`
- Create: `tools/nika-verb-agent/src/lib.rs`
- Create: `tools/nika-verb-agent/src/error.rs`
- Create: `tools/nika-verb-agent/src/caps.rs` (re-export AgentCaps from nika-runtime)
- Create: `tools/nika-verb-agent/src/run.rs` (stub for W14-C2)
- Create: `tools/nika-verb-agent/src/decompose.rs` (moved from engine)
- Create: `tools/nika-verb-agent/src/agent_loop/mod.rs` (verbatim from engine, import paths rewritten)
- Create: `tools/nika-verb-agent/src/agent_loop/chat.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/providers.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/streaming.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/thinking.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/types.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/tests.rs`
- Create: `tools/nika-verb-agent/src/agent_loop/tests_shield_mcp_wrap.rs`

**Cargo.toml for nika-verb-agent:**
```toml
[dependencies]
nika-core.workspace = true
nika-kernel.workspace = true
nika-event.workspace = true
nika-shield.workspace = true
nika-runtime.workspace = true    # AgentCaps
nika-engine.workspace = true     # RigProvider, SkillInjector, McpClient (TEMP until Phase 15)
nika-builtin.workspace = true
rig-core.workspace = true
rustc-hash.workspace = true
serial_test = { workspace = true, optional = true }
tokio.workspace = true
tracing.workspace = true
parking_lot.workspace = true
```

**AgentCaps in nika-runtime/src/caps.rs:**
```rust
pub struct AgentCaps {
    pub provider_registry: Arc<dyn ProviderRegistry>,
    pub shield: nika_shield::ShieldContext,
    pub skill_injector: Arc<SkillInjector>,
    pub skills_map: Arc<HashMap<String, String>>,
    pub mcp_pool: McpClientPool,
    pub builtin_router: Arc<BuiltinToolRouter>,
    pub policy_enforcer: Arc<RwLock<PolicyEnforcer>>,
    pub event_log: EventLog,
    pub cancel_token: CancellationToken,
    pub workflow_base_dir: PathBuf,
    pub resolved_agents: Arc<ResolvedAgents>,
    pub blob_store: Arc<dyn BlobStore>,
}
```

**Do not run tests after this commit** — `run.rs` is still a stub, compile is all we want.

**Verification:**
```bash
cargo check -p nika-verb-agent
# If errors, expect ~30-50 unresolved imports. Fix via the mapping table.
```

**This is the highest-risk commit of Session 14.** Budget 2-3 hours for import surgery. If `cargo check` error count exceeds expectations (>100), STOP and audit whether rig_agent_loop has deeper coupling than the mapping table captures.

### Commit W14-C2 — `feat(verb-agent): implement run.rs`

**run.rs** — mirrors `TaskExecutor::run_agent()` (~602 LOC):

```rust
pub async fn run(
    caps: &AgentCaps,
    task_id: &Arc<str>,
    agent: &AgentParams,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
) -> Result<String, AgentError> {
    // 1. Resolve agent preset from caps.resolved_agents (if from: set)
    // 2. Template-resolve prompt + system + tools via bindings
    // 3. Apply shield (spotlight + canary) via nika_shield::apply(caps.shield, ...)
    // 4. Apply policy via caps.policy_enforcer.check_tool_call(...) for each tool
    // 5. Build RigAgentLoop via agent_loop::RigAgentLoop::new_with_shield(...)
    // 6. Run the loop: loop.run().await
    // 7. Check guardrails (length, schema, regex, llm)
    // 8. Return final output
}
```

The `RigAgentLoop` now lives in `crate::agent_loop`, so the call is `crate::agent_loop::RigAgentLoop::new_with_shield(...)`.

**Verification:**
```bash
cargo test -p nika-verb-agent --lib
# The 70 rig_agent_loop tests that used to live in nika-engine now run in nika-verb-agent.
```

### Commit W14-C3 — `feat(runtime): wire dispatch() TaskAction::Agent arm`

Similar to W14-B4. Fill the `Agent` arm in `dispatch.rs`. Add `agent_caps()` builder on `VerbCapabilities`.

### Commit W14-C4 — `refactor(engine) + chore: delete agent.rs + decompose.rs + rig_agent_loop/`

**Bridge first, delete second:**

Step 1: `TaskExecutor::run_agent()` delegates to `nika_verb_agent::run()`. Golden tests pass.

Step 2: Delete `executor/agent.rs` (-602 LOC), `executor/decompose.rs` (-352 LOC), entire `rig_agent_loop/` directory (~2500 LOC). Total: **-3454 LOC** from nika-engine.

**Verification:**
```bash
find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED: ~138,500 total (down from ~142,000)
cargo test --workspace --lib
```

---

## 6. Wave C — TaskExecutor Dissolution (5 commits, ~2-3h)

### Commit W14-D1 — `refactor(runtime): Runner builds VerbCapabilities directly, no TaskExecutor`

**Files:**
- Modify: `tools/nika-runtime/src/runner.rs` (or wherever Runner lives after S13)

Runner currently holds `executor: TaskExecutor`. After all 5 verbs delegate through dispatch(), Runner constructs `VerbCapabilities` once at startup and passes to `dispatch()` directly. TaskExecutor becomes a thin wrapper around VerbCapabilities (zero logic).

### Commit W14-D2 — `chore(engine): delete TaskExecutor struct + constructor logic`

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs` → delete TaskExecutor struct and all 8 `with_*` builder methods (300+ LOC of constructor logic)
- Modify: `tools/nika-engine/src/runtime/executor/verbs.rs` → move shared helpers (`estimate_tokens`, `strip_think_tags`, `redact_for_event`, `value_as_prompt_str`, `coerce_json_types`, `json_value_size_estimate`) to `nika-runtime/src/util.rs`

### Commit W14-D3 — `chore(engine): delete runtime/executor/ directory`

After W14-D2, the `executor/` directory should only contain `mod.rs` (empty) and `tests/` files. Migrate test infrastructure to each verb crate's test module OR `nika-runtime/src/tests/`. Delete the directory.

**Engine LOC delta:** -1000 to -1500 LOC (constructor + verbs.rs + residual test shells).

### Commit W14-D4 — `chore(engine): remove shield re-export shims from runtime/`

Delete `nika-engine/src/runtime/shield.rs`, `spotlight.rs`, `canary.rs` (the shim re-exports from W14-A4). Any code still importing them directly needs to switch to `nika_shield::*`. Grep-and-replace.

### Commit W14-D5 — `chore(workspace): nika-engine marked as thin shim in Cargo.toml + doc`

Update `tools/nika-engine/Cargo.toml` with a top-of-file comment:
```toml
# nika-engine (dissolution phase 14 — targets Phase 15 for full deletion)
# Residual content:
#   - provider/rig/         concrete RigProvider (used by nika-verb-infer via trait bridge)
#   - runtime/boot.rs       BootSequence, BootPhase (used by nika-cli, nika-tui)
#   - runtime/structured_output.rs  (used by nika-verb-infer)
#   - runtime/chat_workflow.rs     (used by nika-cli chat mode)
# Target LOC after S14: <30k (from 148k pre-Constellation)
```

Update `tools/nika-engine/src/lib.rs` doc comment similarly.

---

## 7. Wave D — Session Close (2 commits, ~1h)

### Commit W14-E1 — `docs(constellation): ARCHITECTURE.md — S14 complete, TaskExecutor dissolved, verb crates 5/5`

**Files:**
- Modify: `tools/nika-engine/ARCHITECTURE.md`
- Modify: `docs/ARCHITECTURE.md` (top-level if exists)

Update:
- Crate count: 28 → **36** (after S12: +2, after S13: +4, after S14: +3)
  - S12 new: `nika-policy`, `nika-extract`
  - S13 new: `nika-runtime`, `nika-verb-exec`, `nika-verb-invoke`, `nika-verb-fetch`
  - S14 new: `nika-shield`, `nika-verb-infer`, `nika-verb-agent`
- Engine LOC: 148,792 → **~138,500** (−10,292 net, target <=100k achieved by further work in Phase 15)
- 5 verb crates exist and are wired through `dispatch()`
- TaskExecutor deleted
- nika-shield added to the diamond diagram
- Mention nika-engine is in thin-shim dissolution state

### Commit W14-E2 — `chore: session14 memory + binary size record`

**Files:**
- Create: `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_constellation_session14.md`
- Modify: `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md`

Run `cargo build --release -p nika` and record binary size. Compare to Session 12 baseline (**118 MB**).

**Prediction:** binary size increases 2-5 MB (to 120-123 MB) due to per-crate metadata overhead + duplicated generic instantiations. If it grows >5 MB, audit for duplicate serde derivations.

---

## 8. Shared Invariants (Reapply Every Commit)

1. AGPL-3.0-or-later header on every new file
2. Co-author line: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic
3. `cargo test --workspace --lib` green before commit
4. `cargo clippy --workspace --lib -- -D warnings` clean before commit
5. Zero `.unwrap()` / `.expect()` in new production code
6. Diamond layering: `cargo tree -p nika-verb-* | grep nika-engine` documented when non-empty with TEMP justification
7. No `trait Verb` — use `enum TaskAction` + `match` in `dispatch()`
8. No monolithic `VerbCtx` — per-verb typed contexts (`ExecCaps`, `FetchCaps`, `InferCaps`, `InvokeCaps`, `AgentCaps`)
9. 1 fix = 1 commit; atomic refactors that break compile if split = exception
10. Push only after explicit user authorization

---

## 9. Golden Test Infrastructure — Critical

Golden tests for infer MUST go through `Runner::run()` → `dispatch()` → `nika_verb_infer::run()`. They must NOT call TaskExecutor directly. If Sessions 12-13 added executor-based golden tests, those tests break when TaskExecutor is deleted. **They must be rewritten as Runner-based.**

Fixture pattern:
```rust
let runner = Runner::from_bootstrap(config).await?;
let result = runner.run(workflow_yaml).await?;
assert_eq!(result.task_output("extract"), expected_json);
```

Audit before Session 14:
- `executor/tests.rs` — likely has direct TaskExecutor construction
- `executor/tests_shield_*.rs` — must move to nika-shield or nika-runtime
- `executor/tests_wiremock.rs` — HTTP tests, may be Runner-based already

---

## 10. Risk Register (Session 14 Specific)

| # | Risk | Likelihood | Mitigation |
|---|---|---|---|
| R1 | Import surgery in W14-C1 explodes (>100 errors) | High | Pre-work mapping table, time-box 3h, stop and audit if over |
| R2 | `RigAgentLoop` has hidden deps on non-bridged engine types | Medium | Run `cargo check` iteratively during mapping |
| R3 | `StructuredOutputEngine` imports `provider::rig` types → nika-verb-infer dep on engine is more than "TEMP" | Medium | Audit imports in W14-B1 pre-work; if bad, bump StructuredOutputEngine extraction into Session 14 scope |
| R4 | Golden tests for infer don't go through Runner (test-only regression) | High | Make Runner-based test fixture infra a W14-B4 prerequisite |
| R5 | Binary size grows >5 MB | Low | Measure; if bad, investigate duplicate monomorphizations |
| R6 | `nika-shield` moves break concurrent test suites (spotlight/canary tests) | Medium | Move tests with their subjects; tests in nika-shield/tests/ |
| R7 | Agent's `MockProvider` from nika-kernel-mock no longer accessible post-crate-move | Low | Verify in W14-C1 before writing run.rs |
| R8 | `EventKind` variants used by rig_agent_loop are not yet in nika-event (still in nika-engine's event module) | Medium | Grep before W14-C1; migrate variants to nika-event if needed |

---

## 11. Test Count Prediction

**Current:** ~10,769 tests (pre-S12 baseline). After S12 foundation: +20-30 (new trait tests). After S13: +10-15 (runtime dispatch tests + golden). After S14: +15-25 (verb crate tests + golden additions).

**Predicted final:** **~10,820-10,840 tests**. Most are migrating, not new.

---

## 12. DECIDE BEFORE STARTING

Block on these answers before Wave A begins:

1. **CanarySystem imports** — grep `use crate::event` in `nika-engine/src/runtime/canary.rs`. Migrate to `nika_event::` if needed.
2. **StructuredOutputEngine dep direction** — confirm `structured_output.rs` does NOT import `provider::rig` types. If it does, Session 14 scope expands.
3. **SpawnAgentTool** — `spawn.rs` may create child RigAgentLoops. After W14-C4 deletion, audit nika-tui's SpawnAgentTool imports.
4. **Session 13 completion state** — verify `nika-runtime` exists with `VerbCapabilities` + `dispatch()` compiles before starting.
5. **RigAgentLoop test helpers** — verify `MockProvider` from `nika-kernel-mock` accessible from `nika-verb-agent` test scope.

---

## 13. Effort Breakdown

| Wave | Commits | Hours |
|---|---|---|
| Wave A — Prerequisites | 4 | 3-4 |
| Wave B1 — verb-infer | 5 | 4-5 |
| Wave B2 — verb-agent | 4 | 4-5 (import surgery) |
| Wave C — Dissolution | 5 | 2-3 |
| Wave D — Close | 2 | 1 |
| **Total** | **20** | **14-18** |

The highest-risk step is W14-C1 import surgery. If `cargo check -p nika-verb-agent` after mapping surfaces >100 errors, pause and audit.

---

## 14. Done Criteria

- [ ] All 20 commits landed
- [ ] `cargo test --workspace --lib` green (~10,820-10,840 tests)
- [ ] `cargo clippy --workspace --lib -- -D warnings` clean
- [ ] `cargo build --release -p nika` green, binary size recorded
- [ ] `find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1` ~138,500 (target ~10k below S13)
- [ ] Crate count: 33 → **36** (3 new: nika-shield, nika-verb-infer, nika-verb-agent)
- [ ] `tools/nika-engine/src/runtime/executor/` directory deleted
- [ ] All 5 verb crates exist and are wired through `nika-runtime::dispatch()`
- [ ] ARCHITECTURE.md + `project_constellation_session14.md` updated
- [ ] User authorized push
- [ ] `git push origin main` completed

---

**Last resort rollback:** `git reset --hard <session14-start-commit>`. Requires explicit user approval.
