# Nika Shield — Sprint 2 Handoff

> **Date:** 2026-04-07
> **Status:** READY TO EXECUTE
> **Predecessor:** `docs/plans/2026-04-07-nika-shield-implementation-plan.md`
> **Prior work:** 17 commits shipped, 10,565 tests passing, zero warnings
> **This sprint:** 7 deferred items to complete the Shield

---

## Context Recap

### What Was Shipped in Sprint 1 (17 commits)

All foundation APIs are in place. The Shield is **architecturally complete** but
the integration points in the runner hot path are stubs. Sprint 1 deliverables:

```
Phase 1 (6 commits) — Trust System + Taint Analysis
  d3ff8efe8  TrustLevel enum + InvocationSource + builtin categorization
  ad894e97b  TaintAnalyzer with 6 warning types (TAINT-001..006)
  4d630136f  TaskResult.trust_level field + RunContext::get_trust()
  0c9035532  SecurityPolicyConfig in PolicyConfig
  872cf1980  nika check --security — compile-time warnings display
  6cd95ad32  Skill integrity verification via blake3 (NIKA-271)

Phase 7 (4 commits) — Hardening Quick Wins
  bbc4b1dfd  html_escape, md_escape, sanitize transforms (67 total)
  ba9540ece  LLM judge guardrail output fenced with NIKA-JUDGE-FENCE
  4b6212f4f  Test fixture updates for SecurityPolicyConfig
  (is_in_json_context was already robust — no change needed)

Phase 2 (3 commits) — Automatic Spotlighting (infrastructure)
  dc904b100  SpotlightFence with randomized UUID delimiter
  41f1f36dc  trust: elevated field in raw + analyzed AST
  9a82da226  Spotlight infrastructure wired into infer executor (stub)

Phase 3 (1 commit) — Capability Enforcement (types)
  dfaefa987  TaskCapabilities + restrict_agent_tools() helper

Phase 4 (1 commit) — Canary tokens
  da65b665e  CanarySystem with 3 UUID-derived tokens + 3 detection modes

Phase 6 (1 commit) — Telemetry events
  d63c97ef9  14 new EventKind variants + task_id extraction + render stubs

Phase 8 (1 commit) — Documentation
  c5584f6d2  SECURITY.md — threat model, defenses, error codes, OWASP mapping

Test fixture fixes
  e05aee575  Cleanup trust_elevated field in test fixtures
```

### What Sprint 1 Did NOT Deliver (this sprint's scope)

1. **Per-binding spotlight wrapping** in infer executor — infrastructure is there,
   but the actual wrapping at resolve time is not wired up.
2. **Canary integration** — CanarySystem exists but isn't injected into prompts
   or checked against outputs in the runner.
3. **Agent tool restriction** — `restrict_agent_tools()` exists but isn't called
   from the agent loop.
4. **`nika:run` trust ceiling** — plan documented, no code yet.
5. **ML detection** — optional, behind `shield-ml` feature flag.
6. **L-SEC-001..008 lint rules** — the infrastructure (taint analyzer) is ready,
   need to wire into `nika-cli/src/lint.rs`.
7. **Security summary in run output** — helper function + end-of-run rendering.

---

## Prerequisites — Read These First

Before touching any code, read these files in this order. They establish the
vocabulary and architectural patterns. **Do NOT skip this step** — you will
make wrong assumptions if you do.

1. **`SECURITY.md`** (project root) — threat model and what we claim to protect against
2. **`docs/plans/2026-04-07-nika-shield-mega-plan.md`** — the original plan
3. **`docs/plans/2026-04-07-nika-shield-implementation-plan.md`** — sprint 1 detailed plan
4. **`tools/nika-core/src/trust.rs`** (260 lines) — TrustLevel + InvocationSource API
5. **`tools/nika-core/src/ast/analyzer/taint.rs`** (755 lines) — TaintAnalyzer source
6. **`tools/nika-core/src/capabilities.rs`** (153 lines) — TaskCapabilities API
7. **`tools/nika-engine/src/runtime/spotlight.rs`** (125 lines) — SpotlightFence API
8. **`tools/nika-engine/src/runtime/canary.rs`** (219 lines) — CanarySystem API
9. **`tools/nika-engine/src/runtime/executor/infer.rs`** — lines 72-160 for the integration
   point (search for "Nika Shield: Spotlight")

Also read the CLAUDE.md files in the project:
- `nika/CLAUDE.md` — user-facing docs
- `tools/nika/CLAUDE.md` — internal architecture + testing rules

## Critical Rules (NON-NEGOTIABLE)

1. **No new verbs.** 5 verbs are sacred. All new behavior goes through existing patterns.
2. **`cargo test --workspace --lib`** after EVERY commit. Always `--lib` (no keychain popups).
3. **`cargo clippy --workspace -- -D warnings`** — zero warnings policy.
4. **1 fix = 1 commit.** Format: `feat(security):` or `fix(security):`.
5. **Co-author ALL commits:** `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`
6. **NEVER use Claude as co-author** — only Nika.
7. **No breaking changes.** All new features additive, off by default.
8. **Errors use NikaError with NIKA-XXX codes.** Shield range is 380-384 and 271.
9. **Read the existing code before modifying it.** Use Explore agents for unfamiliar areas.
10. **Test fixtures update is YOUR responsibility.** When you add a struct field,
    update ALL test fixtures in the same commit. Use `cargo check --workspace`
    aggressively to find broken literals.

---

## Sprint 2 Order of Execution

Items are listed in dependency order. Items 1-4 unblock each other. Items 5-7
are independent and can be done in any order.

```
Item 1  Per-binding spotlight wrapping (~250 LOC, ~6 tests)
        ↓ (unblocks deep trust-aware integration)
Item 2  Canary integration into runner hot path (~180 LOC, ~5 tests)
        ↓
Item 3  Agent tool restriction call site (~120 LOC, ~4 tests)
        ↓
Item 4  nika:run trust ceiling enforcement (~150 LOC, ~4 tests)

Item 5  Security summary in run output (~140 LOC, ~3 tests) [INDEPENDENT]
Item 6  L-SEC lint rules (~200 LOC, ~8 tests) [INDEPENDENT]
Item 7  ML detection (optional, behind feature flag) (~1000 LOC, ~8 tests) [INDEPENDENT]
```

Total estimate: **~2040 LOC code + ~38 tests** (excluding optional ML).

Target: commit per item, verify after each, one PR per item or small groups.

---

## Item 1 — Per-Binding Spotlight Wrapping

**Current state:** `SpotlightFence` exists. Infer executor has a TODO comment.
The hard part is knowing which parts of the resolved prompt came from which binding.

**Goal:** After template resolution of `infer.prompt` and `infer.system`, if any
`with:` binding has `TrustLevel::Untrusted` or `ModelTainted` (looked up via
`RunContext::get_trust()`), wrap ONLY that binding's value with a spotlight
fence, not the whole prompt.

### Why This Is Hard

`template_resolve()` returns a `String` with bindings already substituted.
We can't identify which characters came from which binding without changing
the resolution function itself.

### Recommended Approach — Pre-resolution substitution

Instead of modifying `template_resolve()`, do a pre-pass:

1. Compute trust per binding alias (use task's `with_spec` from `AnalyzedTask`).
2. For each untrusted binding, look up the actual resolved value from `ResolvedBindings`.
3. Create a modified `ResolvedBindings` where untrusted values are pre-wrapped.
4. Pass the modified bindings to `template_resolve()`.

This way, when the template engine substitutes `{{with.article}}` it substitutes
the already-wrapped value. Zero changes to `template.rs`.

### Files to Modify

- `tools/nika-engine/src/runtime/executor/infer.rs` — line ~110 (where the TODO is)

### Required Type Changes

You need access to `task.with_spec` from inside `run_infer()`. Currently the
signature is:

```rust
pub(super) async fn run_infer(
    &self,
    task_id: &Arc<str>,
    infer: &InferParams,              // <-- only infer params, no task
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    output_policy: Option<&OutputPolicy>,
) -> Result<String, NikaError>
```

Options:
- **A (preferred):** Add `with_spec: &WithSpec` parameter to `run_infer()`.
  Update caller in `executor/mod.rs:586` and test calls in `executor/tests.rs`.
- **B:** Look up the task via `task_id` from an execution context. Requires
  threading the analyzed workflow reference, which is a bigger change.

Go with **A**. The `WithSpec` type is `FxHashMap<String, WithEntry>` from
`nika-core::binding`. Each `WithEntry` has a `source: BindingPath` with
`source: BindingSource` — check if it's `BindingSource::Task(task_id)` to
look up trust.

### Implementation Sketch

```rust
// In tools/nika-engine/src/runtime/executor/infer.rs, replace the TODO block:

use nika_core::binding::{BindingSource, WithSpec};
use crate::runtime::spotlight::SpotlightFence;

// At the top of run_infer(), after validation:
let fence = self.shield_fence.clone(); // Arc<SpotlightFence> on TaskExecutor
let spotlight_enabled = self.policy_enforcer.read().config().security.spotlight;

// Compute trust per alias
let mut untrusted_aliases: Vec<(String, TrustLevel, String)> = Vec::new();
if spotlight_enabled {
    for (alias, entry) in with_spec.iter() {
        let trust = match &entry.source.source {
            BindingSource::Task(task_id) => {
                datastore.get_trust(task_id.as_ref()).unwrap_or(TrustLevel::Trusted)
            }
            BindingSource::Input(_) => datastore.invocation_source().input_trust(),
            _ => TrustLevel::Trusted,
        };
        if trust.is_untrusted() {
            let source_label = match &entry.source.source {
                BindingSource::Task(t) => t.to_string(),
                BindingSource::Input(i) => format!("input.{}", i),
                _ => alias.clone(),
            };
            untrusted_aliases.push((alias.clone(), trust, source_label));
        }
    }
}

// Build wrapped bindings for template resolution
let wrapped_bindings: Cow<'_, ResolvedBindings> = if untrusted_aliases.is_empty() {
    Cow::Borrowed(bindings)
} else {
    let mut modified = bindings.clone();
    for (alias, trust, source) in &untrusted_aliases {
        if let Some(value) = modified.get(alias) {
            let raw_str = value_to_string_for_prompt(value); // helper
            let wrapped = fence.wrap(&raw_str, source, *trust);
            modified.set(alias.clone(), Value::String(wrapped));
        }
    }

    // Emit SpotlightApplied event per wrapped binding
    for (alias, trust, _) in &untrusted_aliases {
        self.event_log.emit(EventKind::SpotlightApplied {
            task_id: Arc::clone(task_id),
            binding_alias: alias.clone(),
            trust_level: trust.to_string(),
        });
    }

    Cow::Owned(modified)
};

// Use wrapped_bindings in the existing template_resolve() calls
let mut prompt = match template_resolve(&infer.prompt, wrapped_bindings.as_ref(), datastore) {
    // ... existing error handling
};
```

### Gotchas

- `datastore.invocation_source()` doesn't exist yet — add it as a getter on
  `RunContext`. Default to `InvocationSource::Cli` for backward compat.
- `ResolvedBindings::get()` and `set()` — verify these exist with matching
  signatures. If not, add minimal methods or use the raw `HashMap` interface.
- `value_to_string_for_prompt()` — convert `Value::String` directly, other
  types via `serde_json::to_string()`. Use an inline helper.
- `shield_fence` needs to be added to `TaskExecutor` struct and initialized
  in the constructor. Use `Arc<SpotlightFence>` so it's cheap to clone.
  Only ONE fence per workflow run — pass it to the executor at construction time
  via the runner.

### Tests to Write

Put them in `tools/nika-engine/src/runtime/executor/tests.rs`:

1. `test_spotlight_wraps_untrusted_fetch_result` — fetch task → infer task,
   verify the infer prompt contains `NIKA-FENCE` markers.
2. `test_spotlight_skips_trusted_data` — literal prompt, no wrapping.
3. `test_spotlight_respects_elevated` — task with `trust: elevated`, no wrapping.
4. `test_spotlight_respects_policy_disabled` — `policy.security.spotlight = false`,
   no wrapping.
5. `test_spotlight_emits_event` — check `EventLog` has `SpotlightApplied` event.
6. `test_spotlight_multiple_bindings` — 2 untrusted + 1 trusted binding, each
   untrusted one wrapped individually.

### Verification

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib -- spotlight
cargo clippy --workspace -- -D warnings
```

### Commit Message

```
feat(security): per-binding spotlight wrapping in infer executor

Pre-pass that checks each with: binding's trust level and wraps untrusted
values with SpotlightFence markers before template resolution. Zero changes
to template.rs — wrapping happens at the binding level.

Respects trust: elevated and policy.security.spotlight = false.
Emits SpotlightApplied events for each wrapped binding.

6 tests.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 2 — Canary Integration into Runner Hot Path

**Current state:** `CanarySystem` exists with tests. Not injected or checked anywhere.

**Goal:** 
1. Create one `CanarySystem` per workflow run (in the Runner).
2. Inject canary tokens into EVERY infer/agent system prompt.
3. Check EVERY LLM output against the canary.
4. On detection: emit `CanaryDetected` event. In strict mode, return NIKA-382 error.

### Files to Modify

- `tools/nika-engine/src/runtime/runner/mod.rs` — create CanarySystem at start,
  pass to executor
- `tools/nika-engine/src/runtime/executor/mod.rs` — store `Arc<CanarySystem>` on
  `TaskExecutor`
- `tools/nika-engine/src/runtime/executor/infer.rs` — inject before send,
  check after response
- `tools/nika-engine/src/error.rs` — add NIKA-382 variant

### Implementation Approach

**Step 1:** Add `canary: Arc<CanarySystem>` field to `TaskExecutor`. Initialize
in constructor with `Arc::new(CanarySystem::new())`.

**Step 2:** In `run_infer()`, before sending to provider:
```rust
let system_with_canary = self.canary.inject_into_system_prompt(
    resolved_system.as_deref().unwrap_or("")
);
let resolved_system = Some(system_with_canary);
self.event_log.emit(EventKind::CanaryInjected { task_id: Arc::clone(task_id) });
```

**Step 3:** After response is received:
```rust
if let Some(detection) = self.canary.check_output(&response) {
    let match_type_str = match detection.match_type {
        CanaryMatchType::Exact => "exact",
        CanaryMatchType::Substring => "substring",
        CanaryMatchType::CharSpaced => "char_spaced",
    };
    self.event_log.emit(EventKind::CanaryDetected {
        task_id: Arc::clone(task_id),
        match_type: match_type_str.to_string(),
    });

    // In strict mode, fail the task
    if self.policy_enforcer.read().config().security.taint_mode == TaintMode::Strict {
        return Err(NikaError::CanaryLeaked {
            task_id: task_id.to_string(),
        });
    }
}
```

**Step 4:** Same treatment in `rig_agent_loop/mod.rs` for agent tasks.
Multi-turn agents check after EACH assistant message.

### Gotchas

- Canary injection makes system prompts ~100 chars longer. Token budget may
  need to account for this. Check if `ProviderCalled` events report prompt
  length — if so, the injection should happen before length reporting.
- For providers that enforce strict system prompt format (e.g., structured
  role metadata), test that canary injection doesn't break serialization.
- The `mock` provider should NOT trigger false positives. Tests use random
  canary values, so this should work naturally.
- Streaming responses: check the FULL accumulated response, not each chunk.
- Extended thinking / reasoning traces: check those too if exposed.

### Tests to Write

1. `test_canary_injected_into_system_prompt` — mock provider, verify canary
   is in the system prompt it sees.
2. `test_canary_detection_fires_event` — craft a mock response containing
   the exact canary, verify `CanaryDetected` in EventLog.
3. `test_canary_strict_mode_errors` — `taint_mode = "strict"`, canary leak
   produces NIKA-382.
4. `test_canary_warn_mode_continues` — default warn mode, canary leak
   logs event but task succeeds.
5. `test_canary_no_false_positive_normal_response` — normal LLM response,
   no detection.

### Commit Message

```
feat(security): integrate CanarySystem into runner hot path

Inject 3 random canary tokens into every infer/agent system prompt at
workflow start. Check every LLM response against the canary (exact,
substring, char-spaced). On detection emit CanaryDetected; in strict
mode return NIKA-382 error.

Mock provider ignores canaries naturally. Agent loop checks after
each turn. Response length accounting updated.

5 tests + NIKA-382 error variant.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 3 — Agent Tool Restriction Call Site

**Current state:** `restrict_agent_tools()` helper exists in `nika-core/src/capabilities.rs`
with 3 unit tests. Not called from anywhere.

**Goal:** Before an agent's rig loop starts, compute transitive input trust and
filter dangerous tools if untrusted.

### Files to Modify

- `tools/nika-engine/src/runtime/rig_agent_loop/mod.rs` — find where tools
  are built (likely near line 230 or wherever `tools: Vec<ToolDef>` is constructed)
- `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs` — same
- `tools/nika-engine/src/runtime/executor/agent.rs` — may also need changes

### Implementation

The `restrict_agent_tools()` function is pure. The hard part is knowing:
1. The agent's `with_spec` (to compute input trust)
2. Whether the task has `trust: elevated` 
3. The `dangerous_tools` list from policy config

All three are available if you thread the `AnalyzedTask` reference to the
agent loop, OR if you compute input trust at the executor level and pass
a precomputed `has_untrusted_inputs: bool` + `trust_elevated: bool`.

**Recommended:** Precompute at executor level, pass as function arguments.
This keeps the agent loop pure.

```rust
// In executor/agent.rs (or wherever agents are dispatched):

let has_untrusted = with_spec.values().any(|entry| {
    match &entry.source.source {
        BindingSource::Task(t) => datastore
            .get_trust(t.as_ref())
            .is_some_and(|tr| tr.is_untrusted()),
        BindingSource::Input(_) => datastore.invocation_source().input_trust().is_untrusted(),
        _ => false,
    }
});

let trust_elevated = task.trust_elevated;

let dangerous_tools = self.policy_enforcer
    .read()
    .config()
    .security
    .dangerous_tools
    .clone();

let (filtered_tools, removed) = nika_core::capabilities::restrict_agent_tools(
    agent.tools.clone(),
    has_untrusted,
    trust_elevated,
    &dangerous_tools,
);

// Emit event for each removed tool
for tool in &removed {
    self.event_log.emit(EventKind::AgentToolRestricted {
        task_id: Arc::clone(task_id),
        removed_tool: tool.clone(),
        reason: format!("Untrusted input + not elevated"),
    });
}

// Pass filtered_tools to the rig agent loop instead of agent.tools
```

### Additional: Block Recon via nika:read

For tainted agents, also block `nika:read` calls to sensitive files:
- `.nika.yaml` (any workflow file)
- `nika.toml`
- `.mcp.json`
- `*.env`, `.env.*`

This goes in the `nika:read` builtin tool implementation, not the agent loop.
Check if the caller is an agent with untrusted inputs (propagate a flag via
tool context) and reject paths matching these patterns.

File: `tools/nika-engine/src/runtime/builtin/file_tools.rs` or similar.
Search for `nika:read` to find the handler.

### Tests to Write

1. `test_agent_dangerous_tools_removed_on_untrusted` — agent with
   `nika:write` + untrusted fetch input → tool removed + event fired.
2. `test_agent_tools_kept_with_elevated` — same setup with `trust: elevated`
   → all tools kept.
3. `test_agent_tools_kept_when_trusted` — no untrusted sources → no filtering.
4. `test_agent_recon_blocked_for_tainted_nika_read_nika_toml` — tainted
   agent calling `nika:read` on `nika.toml` → error.

### Commit Message

```
feat(security): restrict agent tools based on trust chain

Before an agent's rig loop starts, compute transitive input trust via
the task's with_spec bindings. If any input is untrusted and the task
does not have trust: elevated, remove tools listed in
policy.security.dangerous_tools.

Also block nika:read on .nika.yaml, nika.toml, .mcp.json for agents
with untrusted inputs (prevents recon → exfil chains).

Emits AgentToolRestricted event per removed tool.

4 tests.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 4 — nika:run Trust Ceiling Enforcement

**Current state:** `nika:run` is a builtin tool that executes a nested workflow.
It's already in `policy.security.dangerous_tools` by default. But the trust
ceiling (parent task's trust limits the nested workflow) is not enforced.

**Goal:**
1. When a task calls `nika:run`, propagate the caller's trust level to the nested
   workflow's `InvocationSource`.
2. If the calling task is untrusted AND `nika:run` is in `dangerous_tools`, reject
   with NIKA-380 unless `trust: elevated`.
3. Nested workflow's own `inputs:` trust = `min(parent_trust, own_trust)`.

### Files to Modify

- `tools/nika-engine/src/runtime/builtin/run_tool.rs` (or wherever `nika:run` lives —
  search for `"nika:run"` in the builtin router)
- Possibly `tools/nika-engine/src/runtime/runner/mod.rs` if the nested runner
  construction needs changes

### Finding nika:run

```bash
rg 'nika:run' tools/nika-engine/src/runtime/builtin/
```

It's probably in a file like `run_tool.rs` or registered in `router.rs`.

### Implementation Approach

The `nika:run` tool invokes a fresh `Runner` instance for the nested workflow.
Currently this runner gets default `InvocationSource::Cli`. You need to:

**Step 1:** Get the calling task's trust level. The caller is the task that
invoked `nika:run`. Use the `task_id` from the tool context → lookup via
`datastore.get_trust(task_id)`.

**Step 2:** If the caller is untrusted:
```rust
let caller_trust = datastore.get_trust(caller_task_id).unwrap_or(TrustLevel::Trusted);

if caller_trust.is_untrusted() {
    let policy = self.policy_enforcer.read().config().security.clone();
    if policy.dangerous_tools.contains(&"nika:run".to_string())
        && !caller_task.trust_elevated
    {
        return Err(NikaError::CapabilityDenied {
            task_id: caller_task_id.to_string(),
            action: "nika:run".to_string(),
            reason: "Parent task has untrusted inputs, nika:run is dangerous, \
                     and trust: elevated is not set".to_string(),
        });
    }
}
```

**Step 3:** When constructing the nested runner, pass `caller_trust` as the
trust ceiling. The nested runner's `InvocationSource` should resolve inputs at
`min(caller_trust, own_trust)`:
```rust
let nested_source = if caller_trust.is_untrusted() {
    InvocationSource::Serve  // Treats inputs as untrusted
} else {
    InvocationSource::Cli
};
```

**Step 4:** If the nested workflow's outputs flow back to the calling task, the
result should also be marked with `caller_trust.merge(ModelTainted)` (because
the nested workflow likely processed the untrusted input).

### Gotchas

- `caller_task_id` might not be directly available in the builtin tool handler.
  Check what's in `ToolContext`. If absent, add it.
- Nested workflows may have their own MCP server startups. Those are isolated
  per-run, so no state leak concern.
- Recursion: A tainted agent calling `nika:run` which calls `nika:run` again.
  The trust ceiling should propagate — write a test for this.

### Tests to Write

1. `test_nika_run_inherits_parent_trust` — parent task has untrusted input,
   nested workflow inputs are treated as untrusted.
2. `test_nika_run_blocked_when_tainted_without_elevated` — NIKA-380 error.
3. `test_nika_run_allowed_with_elevated` — `trust: elevated` bypasses the block.
4. `test_nika_run_nested_recursion_propagates` — 2 levels of `nika:run`,
   innermost still sees untrusted inputs.

### Commit Message

```
feat(security): enforce trust ceiling for nika:run nested workflows

When a task calls nika:run, the nested workflow inherits the parent's
trust level as ceiling. Nested inputs = min(parent_trust, own_trust).

If the calling task has untrusted inputs AND nika:run is in
policy.security.dangerous_tools (default), the call is blocked with
NIKA-380 unless trust: elevated is set on the parent task.

4 tests covering inheritance, blocking, elevation, and recursion.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 5 — Security Summary in Run Output

**Current state:** 14 security events are captured in traces but not rendered
in the run summary.

**Goal:** After a `nika run` completes, display a compact security summary
showing trust level distribution, spotlight counts, canary status, etc.

### Files to Modify

- `tools/nika-display/src/summary.rs` (905 lines) — add `format_security_summary()`
- `tools/nika-display/src/renderer.rs` — stats accumulation (count events)
- `tools/nika-display/src/live.rs` — stats accumulation
- `tools/nika-tui/src/state/stats.rs` — if the TUI tracks stats separately

### Implementation

**Step 1:** Add security fields to `RunStats` (or equivalent):
```rust
// In tools/nika-display/src/renderer.rs or stats.rs
pub struct RunStats {
    // ... existing fields ...

    // Nika Shield
    pub trust_trusted: u32,
    pub trust_model_generated: u32,
    pub trust_model_tainted: u32,
    pub trust_untrusted: u32,
    pub spotlight_applied: u32,
    pub spotlight_skipped: u32,
    pub canary_injected: u32,
    pub canary_detected: u32,
    pub scan_findings: u32,
    pub skill_integrity_failed: u32,
    pub agent_tools_restricted: u32,
    pub capability_denied: u32,
}
```

**Step 2:** In `apply_event()`, increment counters on each security event.

**Step 3:** Add `format_security_summary()` function:
```rust
pub fn format_security_summary(stats: &RunStats, policy: &SecurityPolicyConfig) -> String {
    // Only show if any security activity happened
    let has_activity = stats.spotlight_applied > 0
        || stats.canary_detected > 0
        || stats.scan_findings > 0
        || stats.agent_tools_restricted > 0
        || stats.trust_untrusted > 0;

    if !has_activity && policy.taint_mode == TaintMode::Off {
        return String::new();
    }

    let canary_status = if stats.canary_detected > 0 {
        format!("{} DETECTED ⚠", stats.canary_detected).red().to_string()
    } else {
        format!("{} injected, 0 detections", stats.canary_injected).green().to_string()
    };

    format!(
        "-- Security Summary ------------------------------------\n\
         Trust levels:  {t} Trusted | {g} Generated | {m} Tainted | {u} Untrusted\n\
         Spotlighting:  Applied to {sa} binding(s), skipped {ss}\n\
         Canary:        {canary}\n\
         Scan findings: {sf}\n\
         Tool restrict: {tr} tool(s) removed from tainted agents\n\
         Policy:        taint_mode={tm:?}, spotlight={sp}\n\
         --------------------------------------------------------",
        t = stats.trust_trusted,
        g = stats.trust_model_generated,
        m = stats.trust_model_tainted,
        u = stats.trust_untrusted,
        sa = stats.spotlight_applied,
        ss = stats.spotlight_skipped,
        canary = canary_status,
        sf = stats.scan_findings,
        tr = stats.agent_tools_restricted,
        tm = policy.taint_mode,
        sp = policy.spotlight,
    )
}
```

**Step 4:** Call it from `format_run_summary()` after the regular summary.

### Tests to Write

1. `test_security_summary_clean_workflow` — zero counters, summary either
   empty or shows defaults.
2. `test_security_summary_with_spotlight` — formatting includes "Applied to N".
3. `test_security_summary_canary_detected` — warning-colored output.

### Commit Message

```
feat(security): security summary in nika run output

Track 12 security event counters in RunStats and display them in a
compact summary block after the regular run summary. Only shown when
there's security activity to report (spotlight, canary, findings, etc.)
or when taint_mode is not off.

3 tests for clean/active/detected scenarios.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 6 — L-SEC Lint Rules

**Current state:** `TaintAnalyzer` exists and produces `TaintWarning` variants.
`nika lint` exists in `tools/nika-cli/src/lint.rs` with 918 lines of rules.
Not wired together.

**Goal:** Add 8 new lint rules that run the taint analyzer and convert its
warnings into lint findings.

### Files to Modify

- `tools/nika-cli/src/lint.rs` — add `lint_security()` function

### Implementation

Find the existing lint function structure (probably a `Vec<LintFinding>` or
similar). Add a new section:

```rust
// In tools/nika-cli/src/lint.rs, near the other lint functions

fn lint_security(
    workflow: &nika_core::ast::analyzed::AnalyzedWorkflow,
) -> Vec<LintFinding> {
    use nika_core::ast::analyzer::taint::{TaintAnalyzer, TaintWarning};
    use nika_core::trust::InvocationSource;

    let report = TaintAnalyzer::analyze(workflow, InvocationSource::Cli);
    let mut findings = Vec::new();

    for warning in &report.warnings {
        let (code, severity) = match warning {
            TaintWarning::UntrustedToExec { .. } => ("L-SEC-001", Severity::Warning),
            TaintWarning::UntrustedToAgentTools { .. } => ("L-SEC-002", Severity::Warning),
            TaintWarning::UntrustedToInferNoSchema { .. } => ("L-SEC-003", Severity::Info),
            TaintWarning::UntrustedForEachAmplification { .. } => ("L-SEC-004", Severity::Warning),
            TaintWarning::UntrustedToFetchUrl { .. } => ("L-SEC-005", Severity::Warning),
            TaintWarning::UntrustedWhenCondition { .. } => ("L-SEC-006", Severity::Info),
        };
        findings.push(LintFinding {
            code: code.to_string(),
            severity,
            message: warning.message(),
            recommendation: Some(warning.recommendation().to_string()),
            // ... other fields specific to LintFinding
        });
    }

    // L-SEC-007: Skill file not in skills.integrity
    // Iterate workflow.skills_map, check against policy's skills.integrity map.

    // L-SEC-008: Agent max_turns > 20 with untrusted inputs
    for task in &workflow.tasks {
        if let AnalyzedTaskAction::Agent(agent) = &task.action {
            if let Some(Templatable::Value(max)) = &agent.max_turns {
                if *max > 20 {
                    let input_trust = report.trust_map.get(&task.name).copied();
                    if input_trust.is_some_and(|t| t.is_untrusted()) {
                        findings.push(LintFinding {
                            code: "L-SEC-008".to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "Agent '{}' has max_turns={} with untrusted inputs",
                                task.name, max
                            ),
                            recommendation: Some(
                                "Reduce max_turns to 20 or add trust: elevated".to_string()
                            ),
                            // ...
                        });
                    }
                }
            }
        }
    }

    findings
}
```

Wire `lint_security()` into the main lint dispatcher (usually a function
that collects all findings from all lint_* functions).

### Lint Codes Table

| Code | Description | Severity |
|------|-------------|----------|
| L-SEC-001 | Untrusted → exec without structured intermediate | Warning |
| L-SEC-002 | Agent with dangerous tools processes untrusted data | Warning |
| L-SEC-003 | Infer processes untrusted data without structured schema | Info |
| L-SEC-004 | for_each over untrusted data with concurrency > 5 | Warning |
| L-SEC-005 | Fetch response flows to another fetch URL | Warning |
| L-SEC-006 | `when:` condition depends on untrusted data | Info |
| L-SEC-007 | Skill file not in skills.integrity | Info |
| L-SEC-008 | Agent max_turns > 20 with untrusted inputs | Warning |

### Tests to Write

Probably golden file tests if that's the pattern in `lint.rs`. Otherwise unit tests:
1. `test_lint_sec_001_untrusted_to_exec`
2. `test_lint_sec_002_agent_dangerous_tools`
3. `test_lint_sec_003_infer_no_schema`
4. `test_lint_sec_008_high_max_turns_tainted`
5. `test_lint_clean_workflow_no_sec_findings`

### Commit Message

```
feat(security): add L-SEC-001..008 lint rules

Wire TaintAnalyzer into nika lint. Converts TAINT-001..006 warnings into
L-SEC-001..006 lint findings, plus 2 new rules:
- L-SEC-007: skill file not in [skills.integrity]
- L-SEC-008: agent max_turns > 20 with untrusted inputs

5 tests + golden file comparisons.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Item 7 — ML Detection (OPTIONAL)

**Current state:** Not started. Behind `shield-ml` feature flag per plan.
**Decision:** Can be skipped for initial release. Low priority.

**Goal (if pursued):** Heuristic scanner using Aho-Corasick for known injection
patterns, plus optional ONNX-based DeBERTa classifier for sophisticated attacks.

### Scope

- ~300 LOC heuristic scanner (no new deps — aho-corasick is transitive)
- ~500 LOC ML detector behind `shield-ml` feature
- ~200 LOC `nika shield` CLI subcommand
- ~8 tests

### Files to Create

- `tools/nika-engine/src/runtime/heuristic_scanner.rs`
- `tools/nika-engine/src/runtime/ml_detector.rs` (feature-gated)
- `tools/nika-cli/src/shield.rs` (new CLI subcommand)

### Implementation Details

See the original plan at `docs/plans/2026-04-07-nika-shield-mega-plan.md` section 9.
All architectural details are there. Key points:

- Heuristic scanner: 100+ patterns in categories (instruction override,
  role-playing, encoding bypass, context manipulation, exfiltration)
- ML detector: load `protectai/deberta-v3-base-prompt-injection-v2` ONNX model
- Head+tail chunking for long texts (first 256 + last 256 tokens)
- Threshold: 0.85 default, configurable via `policy.security.ml_threshold`
- Actions: warn | block | log (from `policy.security.ml_action`)

### Feature Flag Setup

In `tools/nika-engine/Cargo.toml`:
```toml
[features]
shield-ml = ["dep:ort", "dep:tokenizers"]

[dependencies]
ort = { version = "2.0", optional = true }
tokenizers = { version = "0.20", optional = true }
```

Forward the feature through `tools/nika/Cargo.toml`:
```toml
[features]
shield-ml = ["nika-engine/shield-ml"]
```

### Commit Message (if pursued)

```
feat(security): optional ML-based prompt injection detection

Aho-Corasick heuristic scanner with 100+ patterns (always available)
plus optional ONNX DeBERTa classifier behind shield-ml feature flag.

Heuristic: runs on fetch responses + MCP tool outputs before they
enter the DAG. Categories: instruction override, role-playing,
encoding bypass, context manipulation, data exfiltration.

ML: loads protectai/deberta-v3-base-prompt-injection-v2 from
~/.nika/models/. Head+tail chunking for long texts. Configurable
threshold (default 0.85) and action (warn|block|log).

New nika shield CLI subcommand: status, download-model, scan.

8 tests (heuristic only — ML tests gated on feature flag).

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Final Verification Checklist

After ALL items are complete:

```
[ ] cargo test --workspace --lib passes (target: ~10650+ tests)
[ ] cargo clippy --workspace -- -D warnings is clean
[ ] nika check workflow.nika.yaml --security works and reports findings
[ ] Trust levels appear in NDJSON traces (nika run + inspect .nika/traces/)
[ ] Security summary shows in nika run output
[ ] nika lint includes L-SEC-001..008 rules
[ ] Spotlighting actually wraps untrusted content in prompts
    (test: create fetch→infer workflow, inspect the prompt sent to mock provider)
[ ] Canary tokens injected into system prompts and detected on leak
[ ] Agent with untrusted input has dangerous tools removed
[ ] nika:run blocked from tainted caller without trust: elevated
[ ] All 14 security EventKind variants have formatters (renderer, live, TUI)
[ ] SECURITY.md is accurate (update if implementation differs from claims)
[ ] No new required dependencies (ML deps behind feature flag)
[ ] All security features off by default (backward compat preserved)
[ ] No new verbs (still 5 sacred verbs)
```

---

## Error Codes Already Reserved

These were allocated in Sprint 1. Use them in Sprint 2 as needed:

| Code | Name | Status |
|------|------|--------|
| NIKA-271 | SkillIntegrityFailed | Implemented in sprint 1 |
| NIKA-380 | CapabilityDenied | Reserved — use in Item 3 + Item 4 |
| NIKA-381 | TrustViolation | Reserved — use in strict mode enforcement |
| NIKA-382 | CanaryLeaked | Reserved — use in Item 2 |
| NIKA-383 | InjectionDetected | Reserved — use in Item 7 |
| NIKA-384 | SpotlightRequired | Reserved — use if spotlight is mandatory in strict mode |

---

## Known Codebase Landmarks

Paths you will touch often:

**nika-core (L0 — zero I/O):**
- `src/trust.rs` — TrustLevel, InvocationSource, builtin trust categorization
- `src/capabilities.rs` — TaskCapabilities, restrict_agent_tools
- `src/ast/analyzer/taint.rs` — TaintAnalyzer (read only, don't modify)
- `src/ast/analyzed/task.rs` — AnalyzedTask with trust_elevated field
- `src/ast/raw/task.rs` — RawTask with trust field

**nika-engine (L2 — runtime):**
- `src/runtime/boot.rs` — PolicyConfig + SecurityPolicyConfig
- `src/runtime/spotlight.rs` — SpotlightFence
- `src/runtime/canary.rs` — CanarySystem
- `src/runtime/policy.rs` — PolicyEnforcer.config() accessor
- `src/runtime/executor/infer.rs` — THE integration point for Items 1 + 2
- `src/runtime/executor/mod.rs` — TaskExecutor struct (line 58)
- `src/runtime/executor/agent.rs` — Agent dispatch
- `src/runtime/rig_agent_loop/mod.rs` — Agent loop (Item 3 target)
- `src/runtime/rig_agent_loop/thinking.rs` — Judge guardrail (already hardened)
- `src/runtime/runner/mod.rs` — Main Runner struct
- `src/runtime/skill_injector.rs` — Skill loading with integrity check
- `src/store/run_context.rs` — TaskResult.trust_level + get_trust()
- `src/runtime/builtin/` — Find nika:run for Item 4

**nika-event:**
- `src/log.rs` — EventKind enum with 14 new security variants

**nika-display:**
- `src/summary.rs` — RunStats + format_run_summary (Item 5)
- `src/renderer.rs` — CLI renderer match
- `src/live.rs` — Live renderer match
- `src/format_event.rs` — Event formatters (may need new ones)

**nika-cli:**
- `src/lint.rs` — Lint rules (Item 6 target)
- `src/check.rs` — nika check command (already wired)

**nika (binary):**
- `src/main.rs` — CLI command definitions, Check command with --security

---

## Testing Philosophy Reminder (from CLAUDE.md)

**Tests must be INTELLIGENT, not superficial:**
- Validate output PROGRAMMATICALLY (type, enum, range) — never `assert!(!is_empty())`
- Structured output prompts must be NATURAL — never mention JSON format
- Same test on ALL providers — failure = engine bug, not provider limitation
- Check EventLog for correct events, not just output strings
- E2E: parse → analyze → execute → validate → verify events

**For security tests specifically:**
- Craft realistic injection payloads, not just "hello world"
- Test WITH the defense enabled AND disabled — both behaviors must work
- Test strict mode AND warn mode
- Test the negative case: clean workflow should have zero findings
- For canary tests: use the actual detection modes (exact, substring, char-spaced)

---

## Recovery from Known Pitfalls

**"cargo test fails after adding a struct field"**
The sed command from sprint 1 was risky. If you add a field to `AnalyzedTask`
or `PolicyConfig`, grep for `when: None,` or `security: Default::default()`
patterns in these files:
- `tools/nika-engine/src/dag/flow.rs` (14+ test fixtures)
- `tools/nika-engine/src/dag/indexed.rs`
- `tools/nika-engine/src/dag/validate.rs`
- `tools/nika-engine/src/runtime/orchestrate.rs`
- `tools/nika-engine/src/runtime/runner/tests.rs`
- `tools/nika-core/src/ast/analyzed/mod.rs` (module test)
- `tools/nika-core/src/ast/analyzed/workflow.rs` (inline tests)
- `tools/nika-cli/src/lint.rs` (3 test fixtures)
- `tools/nika-engine/src/runtime/executor/tests_wiremock.rs`

Use `cargo check --workspace` aggressively to find broken literals.

**"My changes broke the display renderers"**
New EventKind variants require matching arms in:
- `tools/nika-display/src/live.rs` (LiveRenderer::render)
- `tools/nika-display/src/renderer.rs` (CliRenderer::render)
- `tools/nika-tui/src/state/event_handler/mod.rs` (handle_event)

Sprint 1 added catch-all arms for the 14 new events. You can reuse those
arms or add specific formatting as needed.

**"My commit hook failed on cargo fmt"**
The pre-commit hook runs `cargo fmt --check`. Fix with `cargo fmt` and re-stage.

**"I introduced a clippy warning"**
The hook blocks commits with warnings. Read the warning carefully — clippy
is usually right. Common issues:
- `never_loop` — use `if let` instead of `while let` + `break`
- `needless_borrow` — remove `&`
- `redundant_clone` — use `.to_string()` or `.into()`
- `single_match` — use `if let`

---

## What Success Looks Like

Sprint 2 complete = Nika Shield is **operationally effective**, not just
architecturally designed. Running `nika run` on a workflow with a fetch→infer
chain should:

1. Emit `TrustLevelAssigned` for each task in the trace
2. Show `SpotlightApplied` events with actual fence wrapping in the prompt
3. Show `CanaryInjected` before provider call and no `CanaryDetected` for
   well-behaved outputs
4. Display a security summary at the end
5. Flag the workflow in `nika lint` as L-SEC-003 if there's no `structured:`

Running `nika check workflow.nika.yaml --security` should show the same
warnings (compile-time view of the same truth).

Running a malicious test workflow should show the defenses catching attacks:
- Injection payload in fetched HTML → spotlight fences the payload
- Prompt asking for system_prompt repetition → canary detection fires
- Agent task with `nika:write` processing fetched data → tool removed

**Good luck, Crumpet should be proud.** 🦋

---

## Quick Start Commands

```bash
# Environment setup
cd /Users/thibaut/dev/supernovae/nika

# Read the prior sprint's work
git log --oneline 834320464..HEAD | head -20

# Verify you're starting from a clean state
cd tools
cargo test --workspace --lib 2>&1 | grep 'test result' | awk '{sum += $4} END {print "Tests passing:", sum}'
# Expected: 10565

cargo clippy --workspace -- -D warnings
# Expected: clean finish

# Start Item 1
# ... (follow the implementation plan above)
```
