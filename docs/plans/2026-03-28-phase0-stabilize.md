> **Naming decision needed**: The plan below proposes `preset:` as the task-level field
> for referencing `agents:` definitions (because `agent:` is already a verb). Alternatives
> considered: `use:`, `from:` (already used inside `agent:` verb), or renaming the
> top-level block from `agents:` to `presets:`. Decide before implementing Section 2.

# Phase 0: Stabilize (v0.50) -- Detailed Implementation Plan

**Date**: 2026-03-28
**Target**: v0.50.0 (workspace already bumped)
**Duration**: 10 working days
**Baseline**: `cargo check --workspace` = zero errors, version 0.50.0, 8457+ tests

---

### Section 1: Blockers (Day 1-2)

#### B1: LSP borrow-after-move -- ALREADY FIXED

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-lsp/src/backend.rs` lines 58-105

**Status**: The bug described in the master plan ("`parse_tx` moved then used") has already been resolved. The current code at line 80 clones `parse_tx` into `worker_parse_tx` before `parse_tx` is moved into the `Self` struct at line 88. The validation worker at line 92 receives `worker_parse_tx` (the clone), not the moved original.

**Evidence**: `cargo check -p nika-lsp` succeeds with zero errors. `cargo clippy -p nika-lsp -- -D warnings` succeeds with zero warnings. Commit `e73f9692b` ("wire AST cache into validation flow") is the most recent change to this file.

**Remaining action**: None needed for the borrow-after-move itself. However, verify the LSP binary is included in the release pipeline and that the VS Code extension bundles it correctly.

**Estimated time**: 0 minutes (already done)
**Verification**: `cargo check -p nika-lsp && cargo clippy -p nika-lsp -- -D warnings`

---

#### B2: VS Code extension stale at v0.42

**File**: `/Users/thibaut/dev/supernovae/nika/editors/vscode/package.json` line 6
**What's wrong**: `"version": "0.42.0"` -- the extension has not been republished since v0.42. The workspace is at v0.50.0. A stale `.vsix` exists at `/Users/thibaut/dev/supernovae/nika/editors/vscode/nika-lang-0.42.0.vsix`.

**Release pipeline**: The release workflow at `/.github/workflows/release.yml` has a `vscode-publish` job (line 605) gated on `secrets.VSCE_PAT`. The job uses `npx vsce publish` (line 665). The blocker is that the VSCE_PAT secret has likely expired (Azure DevOps PATs expire after 1 year).

**Fix**:
1. Update `editors/vscode/package.json` version to `"0.50.0"`
2. Regenerate the VSCE_PAT in Azure DevOps (portal.azure.com > Personal Access Tokens)
3. Update the `VSCE_PAT` GitHub Actions secret
4. Tag and push `v0.50.0` to trigger the release pipeline, or run `workflow_dispatch` manually
5. Optionally also renew `OVSX_PAT` for Open VSX (VSCodium/Cursor)

**Files to modify**:
- `/Users/thibaut/dev/supernovae/nika/editors/vscode/package.json` -- bump version to 0.50.0
- GitHub Actions secrets -- renew VSCE_PAT

**Estimated time**: 1 hour (most is Azure DevOps + GitHub secrets UI)
**Estimated LOC**: ~1 (version string change)
**Verification**: Check VS Code Marketplace for `nika-lang` version 0.50.0

---

#### B3: Error code table wrong in CLAUDE.md

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` line 97

**What's wrong**: CLAUDE.md says:
```
| 160-164 | Policy/Boot errors |
```

But the authoritative source `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/error.rs` says (lines 23-24):
```
//! - NIKA-160-164: Parse errors (Phase 1 parser -- ParseErrorKind in nika-core, DO NOT REUSE)
//! - NIKA-165-169: Startup/Policy/Boot errors (165=Policy, 166=Boot, 167=Startup)
```

The actual error codes are:
- NIKA-160 through NIKA-164: Reserved for `ParseErrorKind` in nika-core (DO NOT REUSE)
- NIKA-165: `PolicyViolation` (line 501 of error.rs)
- NIKA-166: `BootFailed` (line 508)
- NIKA-167: `StartupError` (line 515)

**The same error appears** in the nika-workflows rules file at `/Users/thibaut/dev/supernovae/dx/.claude/rules/nika-workflows.md` which also says `160-164 | Policy/Boot`.

**Fix**: Update CLAUDE.md line 97 from:
```
| 160-164 | Policy/Boot errors |
```
to:
```
| 160-164 | Parse errors (Phase 1 parser, nika-core) |
| 165-169 | Policy/Boot/Startup errors |
```

**Files to modify**:
- `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` line 97
- `/Users/thibaut/dev/supernovae/dx/.claude/rules/nika-workflows.md` (error code table section)

**Estimated time**: 15 minutes
**Estimated LOC**: ~4
**Verification**: Grep for "160-164" across all `.md` files and ensure consistency with `error.rs`

---

### Section 2: Wire agents: to tasks (Day 3-5)

#### Current State

The `agents:` block EXISTS in the workflow AST:
- **Definition type**: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/agent_def.rs` -- `AgentDef` enum with `From`, `External`, `Inline` variants
- **Workflow field**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs` line 67 -- `pub agents: Option<FxHashMap<String, super::agent_def::AgentDef>>`
- **Resolution**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/resolver.rs` -- `resolve_assets()` converts `AgentDef` into `ResolvedAgent` (system, provider, model, max_turns, temperature)
- **Runner wiring**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` lines 1191-1198 -- calls `resolve_assets_analyzed()` and stores in `self.resolved_assets`

However, **no task mechanism references resolved agents as presets**. The `ResolvedAgent` objects are resolved but never consumed by `infer:`, `exec:`, or `fetch:` tasks.

#### Critical Design Issue: Naming Collision

The master plan proposes `agent: think` on `infer:` tasks. This is **impossible with the current AST** because `agent:` is already a TaskAction verb. The `Task` struct uses `#[serde(flatten)] pub action: TaskAction` which means `agent:` in the YAML will be parsed as `TaskAction::Agent { agent: AgentParams }` -- a multi-turn agent loop -- not as a preset reference.

**Proposed solution**: Use `preset:` as the task-level field name instead of `agent:`.

```yaml
agents:
  think: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  lite: { provider: groq, model: llama-3.3-70b-versatile }

tasks:
  - id: plan
    preset: think           # Resolves provider, model, temperature, system from agents: block
    infer: "Plan the landing page"
```

Alternative: `use: think` (shorter but might be confusing).

#### Implementation Steps

**Step 2.1**: Add `preset` field to `Task` struct

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs` line 282 (after `structured:`)

Add:
```rust
/// Agent preset reference
///
/// References a named agent from the workflow's `agents:` block.
/// Inherits provider, model, temperature, and system from the agent definition.
/// Task-level overrides (provider:, model:, temperature:) take precedence.
#[serde(default)]
pub preset: Option<String>,
```

**Estimated LOC**: 8

**Step 2.2**: Pass `ResolvedAssets` to `TaskExecutor`

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs`

Add a `resolved_agents: Arc<ResolvedAgents>` field to `TaskExecutor` (line ~73, after `custom_endpoints`). Wire it through the constructor and through the runner where the executor is created.

**Files to modify**:
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs` -- add field + constructor param
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` -- pass `self.resolved_assets.agents` when creating executor

**Estimated LOC**: ~25

**Step 2.3**: Apply preset resolution in `run_infer`

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/infer.rs`

After the `infer.validate()` call (line 42), before provider/model resolution (line 151-160), insert preset resolution:

```rust
// Apply agent preset if specified on the task
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

Then modify the provider/model/system/temperature resolution to use preset values as fallback between task-level and workflow-level defaults. The precedence chain is:
1. Task-level explicit fields (highest priority)
2. Agent preset from `agents:` block
3. Workflow-level defaults (lowest priority)

**Estimated LOC**: ~45

**Step 2.4**: Apply preset resolution in `run_agent` and `run_fetch`

**Files**:
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/agent.rs` -- same pattern
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/fetch.rs` -- only provider/model relevant if fetch has LLM-based extraction

**Estimated LOC**: ~30

**Step 2.5**: Add analyzer validation for preset references

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzer/` (appropriate validator file)

Validate at analysis time that any `preset:` value references a name that exists in the workflow's `agents:` block. Emit NIKA-150 (or next available in the 140-151 range) if the preset name is invalid.

**Estimated LOC**: ~20

**Step 2.6**: Add tests

**Files**:
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/tests.rs` -- integration tests
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs` -- parsing tests

Test cases:
1. Parse workflow with `preset: think` on an `infer:` task
2. Preset resolves provider + model correctly
3. Task-level override takes precedence over preset
4. Unknown preset name produces clear error
5. Preset works with `agent:` verb (multi-turn agent inherits from preset too)
6. Preset with no task-level overrides uses all preset values
7. Preset system prompt combines with task system prompt (preset first, task appended)

**Estimated LOC**: ~80

**Step 2.7**: Add LSP completion for preset field

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-lsp-core/` -- completion provider

Add completion items for `preset:` that suggest agent names from the workflow's `agents:` block.

**Estimated LOC**: ~15

**Total for Section 2**: ~223 LOC
**Dependencies**: None (agents: already parsed and resolved)
**Verification**:
- `cargo test --workspace --lib` passes
- New test: YAML with `preset: think` on `infer:` task resolves correctly
- New test: Unknown preset produces NIKA error

---

### Section 3: Registry Bootstrap (Day 5-6)

#### Current State

The registry client at `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs` points to `https://registry.supernovae.studio/api/v1` which does not exist. The `nika pkg search` command creates a `RegistryClient` and calls `.search()` which will fail with a network error.

The entire local package infrastructure works (install, list, remove, info, lock files) -- the only broken piece is remote fetching.

#### Plan: GitHub-based Static Registry (Phase 1)

**Repository**: `supernovae/nika-registry` on GitHub

**Structure**:
```
nika-registry/
  index.json              # Package index (all packages, latest versions)
  packages/
    @nika/
      seo-audit/
        manifest.yaml     # Package metadata
        1.0.0.tar.gz      # Tarball
      code-review/
        manifest.yaml
        1.0.0.tar.gz
    @workflows/
      web-research/
        manifest.yaml
        1.0.0.tar.gz
```

**Step 3.1**: Change default registry URL to GitHub raw URL

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs` line 52

Change:
```rust
pub const DEFAULT_REGISTRY_URL: &str = "https://registry.supernovae.studio/api/v1";
```
to:
```rust
pub const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/supernovae-st/nika-registry/main/api/v1";
```

**Estimated LOC**: 1

**Step 3.2**: Adapt API client for static JSON files

The current `RegistryClient` expects a dynamic API server. For a GitHub static registry, the endpoints become:
- `GET /packages/:name` becomes `GET /packages/:name/metadata.json`
- `GET /search?q=:query` becomes `GET /index.json` + client-side filtering
- `GET /packages/:name/:version/download` becomes direct tarball URL

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs`

Add a `StaticRegistryClient` implementation or modify `RegistryClient` to handle GitHub-style responses. Key changes:
- `search()` fetches `index.json` and filters client-side
- `get_package()` fetches `packages/{name}/metadata.json`
- `download_and_extract()` fetches tarball directly

**Estimated LOC**: ~80

**Step 3.3**: Add fallback behavior when registry is unreachable

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/pkg.rs`

For `nika pkg search` and `nika pkg add`, add graceful fallback:
```
[!] Registry unreachable (https://raw.githubusercontent.com/...)
    Cause: {error}

    Packages install from local cache only.
    Set NIKA_REGISTRY_URL to use a custom registry.
```

**Estimated LOC**: ~15

**Step 3.4**: Create the nika-registry repository with seed packages

**First 5 packages to seed** (extracted from showcase workflows):
1. `@nika/seo-audit` -- SEO audit workflow using fetch + infer
2. `@nika/code-review` -- Code review with agent loop
3. `@nika/blog-generator` -- Multi-step content pipeline
4. `@nika/web-research` -- Fetch + extract + summarize
5. `@nika/media-pipeline` -- Image processing with nika:* builtins

**Files**: External repository creation (not in this codebase)
**Estimated LOC**: ~200 (index.json + 5 manifest.yaml files + tarballs)

**Total for Section 3**: ~296 LOC
**Dependencies**: None
**Verification**:
- `nika pkg search "seo"` returns results (after seed)
- `nika pkg search "nonexistent"` returns "No packages found" (not a crash)
- `NIKA_REGISTRY_URL=http://invalid nika pkg search "test"` shows graceful error

---

### Section 4: Showcase + Course CLI (Day 7-8)

#### Current State

Both `nika showcase list` and `nika course status` are **already fully implemented**.

- **Showcase**: `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/showcase.rs` -- `handle_showcase_command()` with `List` and `Extract` actions. Sources: `SHOWCASE_BUILTIN`, `SHOWCASE_LLM`, `SHOWCASE_EXEC` + init workflows. The `all_showcases()` function collects from all sources.

- **Course**: `/Users/thibaut/dev/supernovae/nika/tools/nika-cli/src/course.rs` -- `handle_course_command()` with 8 subcommands: `status`, `next`, `check`, `hint`, `reset`, `run`, `info`, `watch`.

**Remaining work**: Verify these commands work end-to-end and count the actual workflow total.

**Step 4.1**: Verify showcase count

Run `nika showcase list` and count entries. The master plan says 115, but the capacity hint in `all_showcases()` is 120. Verify and update documentation.

**Step 4.2**: Verify `nika showcase extract` works

Test: `nika showcase extract blog-post-generator` in a temp directory. Verify the `.nika.yaml` file is written correctly.

**Step 4.3**: Verify `nika course status` works

Run `nika course status` inside a project with `nika init --course`. Verify the constellation map renders.

**Step 4.4**: Verify `nika course next` works

Run `nika course next` and verify it identifies the next exercise.

**Estimated LOC**: 0 (verification only, commands already exist)
**Verification**: Manual testing of all 4 commands

---

### Section 5: Documentation Updates (Day 9-10)

#### 5.1: Update rules/nika.md with agents: + preset: examples

**Files**:
- `/Users/thibaut/dev/supernovae/nika/.windsurf/rules/nika.md`
- `/Users/thibaut/dev/supernovae/nika/.roo/rules/nika.md`

Add a new section after the "Workflow Skeleton" showing `agents:` and `preset:`:

```yaml
## Agent Presets

agents:
  think:
    provider: anthropic
    model: claude-sonnet-4-6
    system: "You are a deep reasoning assistant"
    temperature: 0.3
    extended_thinking: true
  lite:
    provider: groq
    model: llama-3.3-70b-versatile
    temperature: 0.7

tasks:
  - id: plan
    preset: think
    infer: "Plan the architecture"
  - id: generate
    preset: lite
    infer: "Generate code from: {{with.plan}}"
    depends_on: [plan]
    with: { plan: $plan }
```

Also add `preset:` to the "Task-Level Fields" section.

**Estimated LOC**: ~40 across both rule files

#### 5.2: Fix CLAUDE.md error code table

See Blocker B3 above. Update:
- `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` line 97

**Estimated LOC**: 4

#### 5.3: Verify llms.txt and llms-syntax.txt are current

**Files**:
- `/Users/thibaut/dev/supernovae/nika/docs/llms.txt`
- `/Users/thibaut/dev/supernovae/nika/docs/llms-syntax.txt`

Review both files. Check that they reference schema @0.12, all 5 verbs, `agents:` block, `preset:` field, and current feature set. Update if stale.

**Estimated LOC**: ~20-30

#### 5.4: Create examples/agents-preset.nika.yaml

**File**: `/Users/thibaut/dev/supernovae/nika/examples/agents-preset.nika.yaml`

A runnable example demonstrating agent presets with mock provider for testability:

```yaml
schema: nika/workflow@0.12
workflow: agents-preset-demo

agents:
  think:
    system: "You are a deep reasoning assistant. Think step by step."
    provider: mock
    model: mock-think
    temperature: 0.3
  lite:
    system: "You are a fast, concise assistant."
    provider: mock
    model: mock-lite
    temperature: 0.8

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

**Estimated LOC**: ~25

**Total for Section 5**: ~89-99 LOC

---

### Section 6: Verification Checklist

| # | Check | Command | Expected |
|---|-------|---------|----------|
| V1 | Workspace compiles | `cd tools && cargo check --workspace` | zero errors |
| V2 | Clippy clean | `cd tools && cargo clippy --workspace -- -D warnings` | zero warnings |
| V3 | Test count | `cd tools && cargo test --workspace --lib 2>&1 \| grep "test result"` | 8500+ tests, 0 failures |
| V4 | LSP compiles | `cd tools && cargo check -p nika-lsp` | zero errors |
| V5 | Showcase list | `nika showcase list` | Shows 100+ workflows |
| V6 | Showcase extract | `nika showcase extract blog-post-generator` | Creates .nika.yaml |
| V7 | Course status | `nika course status` (in course project) | Shows constellation |
| V8 | Course next | `nika course next` (in course project) | Shows next exercise |
| V9 | Pkg search graceful | `nika pkg search "test"` | Either results or graceful error |
| V10 | Error codes consistent | Grep "160-164" in all CLAUDE.md files | Says "Parse errors" |
| V11 | VS Code version | Check `editors/vscode/package.json` | "0.50.0" |
| V12 | Preset example | `nika check examples/agents-preset.nika.yaml` | Valid |
| V13 | Rules updated | Check `.windsurf/rules/nika.md` | Contains `agents:` section |

---

### Summary of Changes by File

| File | Changes | LOC |
|------|---------|-----|
| `tools/nika-engine/src/ast/workflow.rs` | Add `preset: Option<String>` to Task + tests | ~30 |
| `tools/nika-engine/src/runtime/executor/mod.rs` | Add `resolved_agents` field + wiring | ~25 |
| `tools/nika-engine/src/runtime/executor/infer.rs` | Preset resolution before provider/model resolution | ~45 |
| `tools/nika-engine/src/runtime/executor/agent.rs` | Preset resolution for agent verb | ~15 |
| `tools/nika-engine/src/runtime/runner.rs` | Pass resolved_agents to executor | ~10 |
| `tools/nika-engine/src/registry/api.rs` | GitHub static registry URL + client adaptation | ~81 |
| `tools/nika-cli/src/pkg.rs` | Graceful fallback on unreachable registry | ~15 |
| `tools/nika/CLAUDE.md` | Fix error code table | ~4 |
| `editors/vscode/package.json` | Bump version to 0.50.0 | ~1 |
| `.windsurf/rules/nika.md` | Add agents: + preset: docs | ~20 |
| `.roo/rules/nika.md` | Add agents: + preset: docs | ~20 |
| `examples/agents-preset.nika.yaml` | New example file | ~25 |
| Tests across executor + workflow | Preset resolution tests | ~80 |
| **Total** | | **~371** |

---

### Critical Files for Implementation
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs` (add `preset` field to Task struct)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/infer.rs` (preset resolution in infer execution path)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs` (wire ResolvedAgents into TaskExecutor)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/registry/api.rs` (GitHub static registry URL + client)
- `/Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md` (error code table fix)
