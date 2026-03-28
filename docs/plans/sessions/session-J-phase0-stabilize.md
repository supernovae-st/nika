# Session J: Phase 0 Stabilize (~4-5h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-28-phase0-stabilize.md` -- READ IT FIRST.
Master plan: `docs/plans/2026-03-28-v1-master-plan.md` for Phase 0 scope.
Dev reference: `tools/nika/CLAUDE.md` for conventions.

## Mission: Wire `agents:` block to tasks via `preset:`, fix blockers, update docs

Phase 0 is the bridge from quality overhaul to the intelligence phases. Zero new features
except the `preset:` field wiring (which connects the EXISTING `agents:` AST to task execution).
Fix blockers (CLAUDE.md error table, VS Code version). Bootstrap registry URL. Update docs.

### Methodology
For EVERY change: read code -> write test -> implement -> verify -> commit.
`cargo test --workspace --lib` (always --lib). 1 fix = 1 commit.

---

## VERIFIED STATE (confirmed 2026-03-28)

### agents: block already exists in AST
- **Definition**: `tools/nika-core/src/ast/agent_def.rs` (408 LOC) -- `AgentDef` enum
- **Workflow field**: `tools/nika-engine/src/ast/workflow.rs:67` -- `pub agents: Option<FxHashMap<String, AgentDef>>`
- **Resolution**: `tools/nika-engine/src/runtime/resolver.rs` (764 LOC) -- `ResolvedAgent` (system, provider, model, max_turns, temperature)
- **Runner wiring**: `tools/nika-engine/src/runtime/runner.rs:1191-1198` -- `resolve_assets_analyzed()`

### BUT: No task references resolved agents as presets
Tasks specify `provider:` and `model:` directly or fall back to workflow defaults.
`ResolvedAgent` objects are resolved but never consumed by `infer:`, `exec:`, or `fetch:`.

### Naming collision: `agent:` is already a verb
Cannot use `agent: think` on infer tasks because `agent:` is `TaskAction::Agent`.
Use `preset:` as the task-level field name.

---

## SECTION 1: Fix Blockers

### Bug 1: CLAUDE.md error code table wrong
**File**: `tools/nika/CLAUDE.md:97`
**Problem**: Says `160-164 | Policy/Boot errors` but authoritative source `error.rs:23-24` says
160-164 are Parse errors (nika-core), 165-169 are Policy/Boot/Startup.
**Also**: `dx/.claude/rules/nika-workflows.md` has the same error.
**Fix**: Update both files to split the ranges correctly:
```
| 160-164 | Parse errors (Phase 1 parser, nika-core) |
| 165-169 | Policy/Boot/Startup errors |
```
**Test**: Grep for "160-164" across all .md files -- ensure consistency with `error.rs`
**Commit**: `docs(errors): fix NIKA-160-164 range in CLAUDE.md and rules`

### Bug 2: VS Code extension version sync
**File**: `editors/vscode/package.json:6`
**Problem**: `"version": "0.42.0"` while workspace is v0.50.0
**Fix**: Update to `"0.50.0"`
**Commit**: `chore(vscode): bump extension version to 0.50.0`

---

## SECTION 2: Wire agents: to tasks via `preset:` (~223 LOC)

### Task 2.1: Add `preset` field to Task struct
**File**: `tools/nika-engine/src/ast/workflow.rs` (1021 LOC, around line 282 after `structured:`)
**Add**:
```rust
#[serde(default)]
pub preset: Option<String>,
```
**Estimated LOC**: 8
**Commit**: `feat(ast): add preset field to Task struct`

### Task 2.2: Pass ResolvedAssets to TaskExecutor
**File**: `tools/nika-engine/src/runtime/executor/mod.rs` (623 LOC)
Add `resolved_agents: Arc<ResolvedAgents>` field to `TaskExecutor`. Wire through constructor.
**File**: `tools/nika-engine/src/runtime/runner.rs` (6524 LOC)
Pass `self.resolved_assets.agents` when creating executor.
**Estimated LOC**: ~25
**Commit**: `feat(runtime): pass resolved agents to TaskExecutor`

### Task 2.3: Apply preset resolution in run_infer
**File**: `tools/nika-engine/src/runtime/executor/infer.rs` (1283 LOC)
After `infer.validate()` but before provider/model resolution, look up `task.preset` in
`self.resolved_agents`. Apply preset values where task-level fields are None.

**Precedence chain** (3 levels):
1. Task-level explicit fields (highest priority)
2. Agent preset from `agents:` block
3. Workflow defaults (lowest priority)

```rust
let (preset_provider, preset_model, preset_system, preset_temp) = if let Some(ref preset_name) = task.preset {
    match self.resolved_agents.get(preset_name) {
        Some(agent) => (
            Some(agent.provider.clone()),
            agent.model.clone(),
            Some(agent.system.clone()),
            agent.temperature.map(|t| t as f64),
        ),
        None => return Err(NikaError::ValidationError {
            reason: format!("Agent preset '{}' not found in workflow agents: block", preset_name),
        }),
    }
} else {
    (None, None, None, None)
};
```

**Estimated LOC**: ~45
**Commit**: `feat(runtime): resolve agent presets in infer executor`

### Task 2.4: Apply preset resolution in run_agent and run_fetch
**Files**:
- `tools/nika-engine/src/runtime/executor/agent.rs` (428 LOC) -- same pattern
- `tools/nika-engine/src/runtime/executor/fetch.rs` -- only if fetch uses provider/model
**Estimated LOC**: ~30
**Commit**: `feat(runtime): resolve agent presets in agent and fetch executors`

### Task 2.5: Add analyzer validation for preset references
**File**: `tools/nika-core/src/ast/analyzer/` (appropriate validator file)
Validate that `preset:` value references a name in the `agents:` block.
Emit error if preset name is invalid. Use next available code in 140-151 range.
**Estimated LOC**: ~20
**Commit**: `feat(analyzer): validate preset references exist in agents block`

### Task 2.6: Tests (7 test cases)
**Files**: executor tests + workflow parsing tests
1. Parse workflow with `preset: think` on an `infer:` task
2. Preset resolves provider + model correctly
3. Task-level override takes precedence over preset
4. Unknown preset name produces clear error
5. Preset works with `agent:` verb (multi-turn agent inherits)
6. Preset with no task-level overrides uses all preset values
7. Preset system prompt combines with task system prompt
**Estimated LOC**: ~80
**Commit**: `test(runtime): agent preset resolution chain`

### Task 2.7: LSP completion for preset field
**File**: `tools/nika-lsp-core/src/handlers/completion.rs` (1678 LOC)
Add completion items for `preset:` suggesting agent names from the `agents:` block.
**Estimated LOC**: ~15
**Commit**: `feat(lsp): completions for preset field from agents block`

---

## SECTION 3: Registry Bootstrap

### Task 3.1: Change default registry URL to GitHub raw
**File**: `tools/nika-engine/src/registry/api.rs` (685 LOC)
**Change**: `DEFAULT_REGISTRY_URL` from `https://registry.supernovae.studio/api/v1`
to `https://raw.githubusercontent.com/supernovae-st/nika-registry/main/api/v1`
**Estimated LOC**: 1
**Commit**: `fix(registry): point to GitHub static registry`

### Task 3.2: Add graceful fallback when registry unreachable
**File**: `tools/nika-cli/src/pkg.rs` (587 LOC)
Show friendly error message instead of raw network error.
**Estimated LOC**: ~15
**Commit**: `fix(registry): graceful fallback when registry unreachable`

---

## SECTION 4: Documentation Updates

### Task 4.1: Add agents: + preset: to rules
**Files**: `.windsurf/rules/nika.md`, `.roo/rules/nika.md`
Add `agents:` and `preset:` documentation with examples.
**Estimated LOC**: ~40
**Commit**: `docs(rules): document agents block and preset field`

### Task 4.2: Create agents-preset example
**File**: `examples/agents-preset.nika.yaml` (NEW)
Runnable example with `provider: mock`:
```yaml
schema: nika/workflow@0.12
workflow: agents-preset-demo
agents:
  think: { system: "Deep reasoning.", provider: mock, model: mock-think, temperature: 0.3 }
  lite: { system: "Be concise.", provider: mock, model: mock-lite, temperature: 0.8 }
tasks:
  - id: plan
    preset: think
    infer: "Create a 3-step plan for building a REST API"
  - id: implement
    preset: lite
    depends_on: [plan]
    with: { plan: $plan }
    infer: "Implement step 1 from: {{with.plan}}"
```
**Commit**: `docs(examples): add agents-preset demo workflow`

### Task 4.3: Verify llms.txt and llms-syntax.txt are current
**Files**: `docs/llms.txt`, `docs/llms-syntax.txt`
Check they reference @0.12, all 5 verbs, `agents:`, `preset:`. Update if stale.
**Commit**: `docs: update llms.txt with preset field`

---

## E2E Verification Workflows

### test-preset-resolution.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-preset-resolution
description: "E2E: agent preset resolves provider+model+system"
provider: mock

agents:
  think:
    system: "You are a deep thinker."
    provider: mock
    model: mock-think
    temperature: 0.3
  lite:
    system: "Be fast."
    provider: mock
    model: mock-lite
    temperature: 0.9

tasks:
  - id: plan
    preset: think
    infer: "Plan the architecture"
    # Expected: uses mock provider, mock-think model, 0.3 temp

  - id: generate
    preset: lite
    depends_on: [plan]
    with: { plan: $plan }
    infer: "Generate from: {{with.plan}}"
    # Expected: uses mock provider, mock-lite model, 0.9 temp

  - id: override
    preset: think
    provider: mock
    model: mock-override
    temperature: 0.7
    infer: "This overrides the preset"
    # Expected: task-level wins (mock-override, 0.7 temp)
```
**Run**: `nika run test-preset-resolution.nika.yaml --provider mock`

### test-preset-unknown.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-preset-unknown
provider: mock

tasks:
  - id: fail
    preset: nonexistent
    infer: "This should fail"
    # Expected: ValidationError -- preset 'nonexistent' not found
```
**Run**: `nika check test-preset-unknown.nika.yaml` -> error

---

## After All Fixes

```bash
cd tools && cargo check --workspace           # Zero errors
cd tools && cargo clippy --workspace -- -D warnings  # Zero warnings
cd tools && cargo test --workspace --lib      # 8600+ tests, 0 failures
nika check examples/agents-preset.nika.yaml   # Valid
nika showcase list                            # Shows 100+ workflows
nika pkg search "test"                        # Graceful error or results
```

---

## Commit Strategy (14 commits)

```
# Section 1: Blockers
docs(errors): fix NIKA-160-164 range in CLAUDE.md and rules
chore(vscode): bump extension version to 0.50.0

# Section 2: preset: wiring
feat(ast): add preset field to Task struct
feat(runtime): pass resolved agents to TaskExecutor
feat(runtime): resolve agent presets in infer executor
feat(runtime): resolve agent presets in agent and fetch executors
feat(analyzer): validate preset references exist in agents block
test(runtime): agent preset resolution chain
feat(lsp): completions for preset field from agents block

# Section 3: Registry
fix(registry): point to GitHub static registry
fix(registry): graceful fallback when registry unreachable

# Section 4: Documentation
docs(rules): document agents block and preset field
docs(examples): add agents-preset demo workflow
docs: update llms.txt with preset field
```
