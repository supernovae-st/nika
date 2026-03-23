# model: Required Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `model:` a required field when workflows use LLM verbs (infer/agent), with helpful error messages showing available models and pricing. Fix the native provider to auto-load models from disk.

**Architecture:** Validation in the AST analyzer (not parser, not schema) so that exec/fetch/invoke-only workflows are unaffected. The analyzer scans tasks for LLM verbs and requires model at workflow-level OR task-level. Error messages list available models with costs. Native provider auto-loads from `~/.nika/models/` based on model: field.

**Tech Stack:** Rust (analyzer, executor, rig_agent_loop), JSON Schema, LSP, YAML templates

**Breaking change:** Yes. Workflows using infer/agent without model: will fail validation.

**Impact:** 81 .nika.yaml files need model: added. 219 exec/fetch/invoke-only files unaffected.

---

## Part 1: Analyzer Validation (NIKA-034 MissingModel)

### Task 1: Add AnalyzeErrorKind::MissingModel

**Files:**
- Modify: `tools/nika/src/ast/analyzer/errors.rs:163` -- add variant to enum
- Modify: `tools/nika-core/src/ast/analyzer/errors.rs` -- same in core

Add after `InvalidBinding` (line 171):

```rust
/// NIKA-034: model: required when infer/agent verb is used
MissingModel,
```

Update the `code()` method to return `"NIKA-034"` and the display/suggestion methods.

**Verify:** `cargo check` compiles (will have unused variant warning until Task 2).

### Task 2: Add model validation in analyze()

**Files:**
- Modify: `tools/nika/src/ast/analyzer/analyze.rs:185` -- add validation after line 185
- Modify: `tools/nika-core/src/ast/analyzer/analyze.rs:185` -- same in core

Insert after line 185 (`workflow.model = raw.model.map(|s| s.value);`):

```rust
// 3b. Validate model is specified when LLM verbs are used
let has_workflow_model = workflow.model.is_some();
for raw_task in &raw.tasks.value {
    let task = &raw_task.value;
    let uses_llm = task.action.as_ref().is_some_and(|a| matches!(
        &a.value,
        RawTaskAction::Infer(_) | RawTaskAction::Agent(_)
    ));
    let has_task_model = task.model.is_some();

    // Provider 'mock' is exempt (no real API calls)
    let provider_name = task.provider.as_ref()
        .map(|p| p.value.as_str())
        .or(raw.provider.as_ref().map(|p| p.value.as_str()))
        .unwrap_or("");
    let is_mock = provider_name == "mock";

    if uses_llm && !has_workflow_model && !has_task_model && !is_mock {
        let span = task.id.as_ref().map(|id| id.span).unwrap_or_default();
        ctx.errors.push(AnalyzeError {
            kind: AnalyzeErrorKind::MissingModel,
            span,
            message: format!(
                "Task '{}' uses {} verb but no model is specified. \
                 Add `model:` at workflow level or on this task. \
                 Example: model: gpt-4o-mini",
                task.id.as_ref().map(|id| id.value.as_str()).unwrap_or("?"),
                if matches!(task.action.as_ref().map(|a| &a.value), Some(RawTaskAction::Agent(_))) { "agent:" } else { "infer:" }
            ),
            suggestion: Some("Add `model: gpt-4o-mini` after `provider:` in your workflow".to_string()),
            related: vec![],
        });
    }
}
```

**Verify:** `cargo test --lib` -- existing tests that use wrap() will FAIL (expected, fix in Task 5).

### Task 3: Update error display for NIKA-034

**Files:**
- Modify: `tools/nika/src/error.rs:202` -- add after NIKA-033

No change needed in error.rs -- the analyzer errors go through AnalyzeError, not NikaError. The display is handled by `AnalyzeError::fmt()` in errors.rs.

But add a hint method for MissingModel in errors.rs:

```rust
AnalyzeErrorKind::MissingModel => {
    "Required: specify `model:` when using infer: or agent: verbs. \
     Run `nika provider list` to see available models."
}
```

**Verify:** `cargo check` compiles.

---

## Part 2: Fix Agent Model Cascade (bug)

### Task 4: Wire workflow model into agent path

**Files:**
- Modify: `tools/nika/src/runtime/executor/verbs.rs:1800` -- inject workflow model
- Modify: `tools/nika/src/runtime/rig_agent_loop/providers.rs:108-112,215-219` -- remove hardcoded defaults
- Modify: `tools/nika/src/runtime/rig_agent_loop/chat.rs:122,214,305,371,440,506` -- remove all unwrap_or
- Modify: `tools/nika/src/runtime/rig_agent_loop/thinking.rs:319` -- remove unwrap_or

In `verbs.rs:1800`, before creating `RigAgentLoop`, inject the workflow model:

```rust
let resolved_agent = AgentParams {
    provider: Some(provider_name.clone()),
    model: resolved_agent.model.clone()
        .or_else(|| self.default_model.as_ref().map(|m| m.to_string())),
    ..resolved_agent
};
```

Then in `providers.rs`, `chat.rs`, `thinking.rs`, change all:
```rust
.unwrap_or_else(|| "claude-sonnet-4-6".to_string())
```
To:
```rust
.expect("model is required -- validated by analyzer")
```

**Verify:** `cargo check` compiles. Tests will fail until Task 5.

---

## Part 3: Update wrap() helper + tests

### Task 5: Fix tests_200_workflows.rs wrap() helper

**Files:**
- Modify: `tools/nika/src/ast/tests_200_workflows.rs:38-44` -- add model to wrap()

Change:
```rust
fn wrap(task_yaml: &str) -> String {
    let indented: String = task_yaml.lines().map(|line| format!("    {line}\n")).collect();
    format!("schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t1\n{indented}")
}
```
To:
```rust
fn wrap(task_yaml: &str) -> String {
    let indented: String = task_yaml.lines().map(|line| format!("    {line}\n")).collect();
    format!("schema: \"nika/workflow@0.12\"\nprovider: mock\nmodel: test-model\ntasks:\n  - id: t1\n{indented}")
}
```

Using `provider: mock` + `model: test-model` makes ALL existing tests pass without needing real API keys and satisfies the new model requirement.

Also update other test helpers that create workflows without model:
- Search for `"schema: \"nika/workflow@0.12\""` across test files
- Add `model: test-model` to each

**Verify:** `cargo test --lib` -- all tests should pass now.

---

## Part 4: Update gate examples (~44 files)

### Task 6: Add model: to gate examples that use infer/agent

**Files:** ~44 files in `tools/nika/examples/gates/`

Script approach:
```bash
cd tools/nika/examples/gates
for f in $(find . -name "*.nika.yaml"); do
  # Skip files that already have model: or only use exec/fetch/invoke
  if grep -q "infer:\|agent:" "$f" && ! grep -q "^model:" "$f" && ! grep -q "provider: mock" "$f"; then
    # Add model: after provider: line
    sed -i '' '/^provider:/a\
model: gpt-4.1-mini' "$f"
  fi
done
```

Then verify: `for f in $(find . -name "*.nika.yaml"); do nika check "$f" 2>&1 | grep -q "V A L I D" || echo "FAIL: $f"; done`

**Verify:** All gates pass `nika check`.

---

## Part 5: Update init templates

### Task 7: Add model: to init templates

**Files:**
- Modify: `tools/nika/src/init/tier2.rs` -- add `model: gpt-4o-mini` after each `provider:` line
- Modify: `tools/nika/src/init/tier3.rs` -- same
- Modify: `tools/nika/src/init/tier4.rs` -- same
- Modify: `tools/nika/src/init/tier5.rs` -- same
- Modify: `tools/nika/src/init/tier6.rs` -- same

For templates that use `provider: auto`, add `model: gpt-4o-mini`.
For templates that don't have `provider:` but use `infer:`, add both `provider: openai\nmodel: gpt-4o-mini`.

**Verify:** `nika init` in temp dir + `nika check` on all generated workflows.

---

## Part 6: Update course workflows

### Task 8: Verify course workflows have model:

**Files:** Check `/Users/thibaut/Desktop/nika-test-034/course/`

Most should already have `model:` from our earlier work. Verify:
```bash
cd /Users/thibaut/Desktop/nika-test-034
for f in $(find course/ -name "*.nika.yaml"); do
  if grep -q "infer:\|agent:" "$f" && ! grep -q "^model:" "$f"; then
    echo "MISSING: $f"
  fi
done
```

Fix any remaining gaps.

---

## Part 7: LSP + Schema updates

### Task 9: Update LSP and schema descriptions

**Files:**
- Modify: `tools/nika-lsp-core/src/handlers/completion.rs:129-133` -- "Optional" -> "Required for LLM tasks"
- Modify: `tools/nika/schemas/nika-workflow.schema.json:32-36` -- update description

---

## Part 8: Final verification

### Task 10: Full test suite + E2E

```bash
# Unit tests
cd tools/nika && cargo test --lib
# Clippy
cargo clippy --all-targets
# nika-core tests
cd tools/nika-core && cargo test --lib
# Gate examples
cd tools/nika && for f in $(find examples/gates -name "*.nika.yaml" | head -20); do nika check "$f" 2>&1 | grep -q "V A L I D" || echo "FAIL: $f"; done
# Course workflows
cd /Users/thibaut/Desktop/nika-test-034 && for f in $(find course/ -name "*.nika.yaml"); do nika check "$f" 2>&1 | grep -q "V A L I D" || echo "FAIL: $f"; done
# E2E with real provider
nika run course/level-01-east-blue/01-hello-world.nika.yaml
```

---

## Execution Order (recommended)

1. **Task 1** -- Add error variant (quick, no impact)
2. **Task 5** -- Fix wrap() helper FIRST (prevents test cascade failures)
3. **Task 2** -- Add analyzer validation (the core change)
4. **Task 3** -- Error display
5. **Task 4** -- Agent cascade fix
6. **Task 6** -- Gate examples (bulk update)
7. **Task 7** -- Init templates
8. **Task 8** -- Course workflows
9. **Task 9** -- LSP + Schema
10. **Task 10** -- Full verify

Commit after each task. Push at end.
