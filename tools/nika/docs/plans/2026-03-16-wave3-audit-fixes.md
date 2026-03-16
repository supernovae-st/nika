# Wave 3: Remaining Audit Bug Fixes

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 8 remaining bugs found by the 5-agent deep audit sweep, with TDD and granular commits.

**Architecture:** Three batches grouped by subsystem: AST/Parser, Security, Runtime. Each fix follows RED-GREEN-REFACTOR with a commit per fix.

**Tech Stack:** Rust, rig-core v0.32, tokio, marked_yaml, serde

---

## False Positives Triage (DO NOT FIX)

These findings from the audit agents are **not bugs** -- confirmed by investigation:

| Finding | Verdict | Reasoning |
|---------|---------|-----------|
| 5 dead `RigAgentStatus` variants | By design | Placeholders for confidence routing, token budgets, cost limits |
| Tools consumed via `mem::take` | By design | `dyn ToolDyn` is not `Clone` -- agent loops are single-use |
| Shell mode allows `$()` / backticks | By design | `shell: true` explicitly opts in to shell features |
| Unicode/emoji task IDs accepted | Not harmful | User-facing labels, restricting to ASCII is exclusionary |
| Empty prompts accepted by parser | By design | LLM backend rejects at runtime -- parser stays loose |
| Self-dependency silently filtered | Correct UX | `depends_on: [self]` would cycle -- filtering is better than erroring |
| Pre-existing `secrets::tests` failure | Test bug | Env var race condition in test, not production. Separate fix. |
| NFKC/ZWC/path traversal defenses | Work correctly | Confirmations that security hardening is effective |

---

## Batch 1: AST/Parser Correctness

### Task 1: [HIGH] Fix `goal` vs `prompt` Discrepancy in Agent Verb

**The Bug:** Parser reads `goal` first (line 572), falls back to `prompt` (line 573). But JSON schema requires `prompt`, all examples use `prompt`, and `AgentParams` deserializes `prompt`. The internal struct field is named `goal`. This creates a broken contract between schema and parser.

**Evidence:**
- `schemas/nika-workflow.schema.json:537` -- `"required": ["prompt"]`
- `src/ast/raw/parser.rs:572-573` -- reads `goal` first, `prompt` second
- `src/ast/raw/action.rs:167` -- struct field named `goal`
- `src/ast/agent.rs:90` -- AgentParams uses `prompt`
- All example files use `prompt:`
- Only 1 test uses `goal:` (parser test line 1555)

**Files:**
- Modify: `src/ast/raw/parser.rs:572-578`
- Modify: `src/ast/raw/action.rs:167`
- Modify: `src/ast/lower.rs` (wherever `.goal` is referenced)
- Modify: `src/ast/raw/parser.rs:1555` (test)
- Test: `src/ast/raw/parser.rs` (existing test block)

**Step 1: Write the failing test**

```rust
// In src/ast/raw/parser.rs, add to test module
#[test]
fn test_parse_agent_prompt_is_primary_field() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      prompt: "Research AI trends"
      max_turns: 10
"#;
    let result = parse_workflow_yaml(yaml);
    assert!(result.is_ok(), "agent with prompt: field should parse");
    let wf = result.unwrap();
    let task = &wf.tasks[0];
    match &task.action {
        Some(RawTaskAction::Agent(agent)) => {
            assert_eq!(agent.value.prompt.value, "Research AI trends");
        }
        _ => panic!("Expected Agent action"),
    }
}
```

**Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib test_parse_agent_prompt_is_primary_field 2>&1`
Expected: FAIL -- field is named `goal`, not `prompt`

**Step 3: Implement the fix**

In `src/ast/raw/action.rs:167`:
```rust
// BEFORE:
pub goal: Spanned<String>,
// AFTER:
pub prompt: Spanned<String>,
```

In `src/ast/raw/parser.rs:572-578`:
```rust
// BEFORE:
let goal = get_string_field(file, m, "goal")?
    .or(get_string_field(file, m, "prompt")?)
// AFTER:
let prompt = get_string_field(file, m, "prompt")?
    .or(get_string_field(file, m, "goal")?)
```

Update the `Ok(RawAgentAction { goal, ... })` to `Ok(RawAgentAction { prompt, ... })`.

In `src/ast/lower.rs` -- find all `.goal` references and rename to `.prompt`.

In `src/ast/raw/parser.rs:1555` (test): change `goal: "Research AI trends"` to `prompt: "Research AI trends"`.

**Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib test_parse_agent_prompt 2>&1`
Expected: PASS

**Step 5: Add legacy fallback test**

```rust
#[test]
fn test_parse_agent_goal_legacy_fallback() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      goal: "Research AI trends"
"#;
    let result = parse_workflow_yaml(yaml);
    assert!(result.is_ok(), "agent with goal: should still parse (legacy)");
}
```

**Step 6: Run full test suite**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib 2>&1 | tail -5`
Expected: All pass (the rename is mechanical)

**Step 7: Commit**

```bash
git add src/ast/raw/action.rs src/ast/raw/parser.rs src/ast/lower.rs
git commit -m "fix(ast): rename agent goal to prompt to match schema and examples

The parser read goal as primary field but JSON schema, all examples,
and AgentParams all use prompt. Swap priority: prompt is primary,
goal is legacy fallback.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

### Task 2: [MEDIUM] Detect Multiple Verbs in Task (Silent Drop Bug)

**The Bug:** `parse_action()` uses sequential if-let with early return. When both `infer:` and `exec:` are present, it returns the first one found and silently drops the second. No error raised.

**Evidence:**
- `src/ast/raw/parser.rs:369-405` -- early return on first verb match
- `schemas/nika-workflow.schema.json:272-278` -- schema uses `oneOf` (correct)
- `src/ast/schema_validator.rs:444-451` -- schema validator test expects error (correct)
- Parser has NO test for multi-verb error

**Files:**
- Modify: `src/ast/raw/parser.rs:369-405`
- Test: `src/ast/raw/parser.rs` (test module)

**Step 1: Write the failing test**

```rust
#[test]
fn test_parse_multi_verb_task_returns_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Hello"
    exec: "echo done"
"#;
    let result = parse_workflow_yaml(yaml);
    assert!(result.is_err(), "Multiple verbs in one task should be rejected");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("multiple"), "Error should mention multiple verbs: {msg}");
}
```

**Step 2: Run test to verify it fails**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib test_parse_multi_verb 2>&1`
Expected: FAIL -- parser silently picks infer, no error

**Step 3: Implement the fix**

In `src/ast/raw/parser.rs:369-405`, rewrite `parse_action()`:

```rust
fn parse_action(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
) -> Result<Option<RawTaskAction>, ParseError> {
    // Collect all present verb keys BEFORE parsing any
    let verb_keys: Vec<&str> = ["infer", "exec", "fetch", "invoke", "agent"]
        .iter()
        .filter(|k| map.get_node(k).is_some())
        .copied()
        .collect();

    // Reject multiple verbs
    if verb_keys.len() > 1 {
        let span = node_to_span(file, map.as_node());
        return Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span,
            message: format!(
                "Task has multiple verbs: {}. Each task can only have one action.",
                verb_keys.join(", ")
            ),
        });
    }

    // Parse the single verb (existing dispatch logic)
    if let Some(node) = map.get_node("infer") {
        let action = parse_infer_action(file, node)?;
        let span = node_to_span(file, node);
        return Ok(Some(RawTaskAction::Infer(Spanned::new(action, span))));
    }
    // ... rest unchanged
}
```

**Step 4: Run test to verify it passes**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib test_parse_multi_verb 2>&1`
Expected: PASS

**Step 5: Run full test suite (regression check)**

Run: `CARGO_TARGET_DIR=target-main cargo test -p nika --lib 2>&1 | tail -5`
Expected: All pass -- no existing tests use multi-verb tasks

**Step 6: Commit**

```bash
git add src/ast/raw/parser.rs
git commit -m "fix(ast): reject tasks with multiple verbs instead of silent drop

parse_action() used early return on first verb match, silently
dropping any additional verbs. Now collects all present verb keys
first and errors if more than one is found.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

### Task 3: [LOW] Reject Empty Tasks Array in Analyzer

**The Bug:** `tasks: []` passes parser but fails JSON schema (minItems: 1). Inconsistency between parser and schema validation.

**Evidence:**
- `schemas/nika-workflow.schema.json:62-68` -- `"minItems": 1`
- Parser accepts empty array without validation

**Files:**
- Modify: `src/ast/analyzed/analyzer.rs` (or wherever workflow-level validation happens)
- Test: same file

**Step 1: Write the failing test**

```rust
#[test]
fn test_empty_tasks_array_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: empty
tasks: []
"#;
    let result = parse_and_analyze(yaml);
    assert!(result.is_err(), "Empty tasks should be rejected");
}
```

**Step 2: Implement**

Add check in the analyzer after parsing: if `workflow.tasks.is_empty()`, return error.

**Step 3: Commit**

```bash
git commit -m "fix(ast): reject empty tasks array in analyzer

Schema requires minItems:1 but parser accepted tasks:[].
Now caught in analyzer phase with clear error message.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Batch 2: Security Hardening

### Task 4: [MEDIUM] Block Dangerous Environment Variables in exec Verb

**The Bug:** The `env:` field in exec tasks applies arbitrary env vars to child processes. No blocklist for dangerous vars like `LD_PRELOAD`, `DYLD_INSERT_LIBRARIES`.

**Evidence:**
- `src/runtime/executor/verbs.rs:509-514` (shell mode) -- `cmd.env(key, value)` with no filtering
- `src/runtime/executor/verbs.rs:549-554` (shell-free mode) -- same
- `src/ast/security.rs` -- has command blocklist but NO env var blocklist

**Design Decision:** Two-tier approach:
- **BLOCK** (error): `LD_PRELOAD`, `LD_LIBRARY_PATH`, `DYLD_INSERT_LIBRARIES`, `DYLD_FRAMEWORK_PATH` -- library injection, always dangerous
- **WARN** (allow with tracing::warn): `PATH`, `PYTHONPATH`, `RUBYLIB`, `PERL5LIB`, `NODE_PATH` -- path override, sometimes legitimate

**Files:**
- Modify: `src/ast/security.rs` (add blocklist + validation function)
- Modify: `src/runtime/executor/verbs.rs:509-514, 549-554`
- Test: `src/ast/security.rs` (test module)

**Step 1: Write the failing test**

```rust
// In src/ast/security.rs test module
#[test]
fn test_ld_preload_blocked() {
    let env = FxHashMap::from_iter([
        ("LD_PRELOAD".to_string(), "/tmp/evil.so".to_string()),
    ]);
    let result = validate_env_vars(&env);
    assert!(result.is_err(), "LD_PRELOAD should be blocked");
}

#[test]
fn test_custom_env_var_allowed() {
    let env = FxHashMap::from_iter([
        ("MY_CUSTOM_VAR".to_string(), "value".to_string()),
    ]);
    let result = validate_env_vars(&env);
    assert!(result.is_ok(), "Custom env vars should be allowed");
}

#[test]
fn test_path_override_warns_but_allowed() {
    let env = FxHashMap::from_iter([
        ("PATH".to_string(), "/custom/bin:/usr/bin".to_string()),
    ]);
    let result = validate_env_vars(&env);
    assert!(result.is_ok(), "PATH override should be allowed with warning");
}
```

**Step 2: Implement**

In `src/ast/security.rs`:

```rust
/// Environment variables that are ALWAYS blocked (library injection)
const BLOCKED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_FRAMEWORK_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
];

/// Environment variables that trigger a warning (path hijacking)
const WARNED_ENV_VARS: &[&str] = &[
    "PATH",
    "PYTHONPATH",
    "RUBYLIB",
    "PERL5LIB",
    "NODE_PATH",
    "GEM_PATH",
];

/// Validate environment variables for child process safety.
/// Blocks library injection vars. Warns on path override vars.
pub fn validate_env_vars(
    env: &rustc_hash::FxHashMap<String, String>,
) -> Result<(), NikaError> {
    for key in env.keys() {
        let upper = key.to_uppercase();
        if BLOCKED_ENV_VARS.iter().any(|b| *b == upper) {
            return Err(NikaError::SecurityError {
                reason: format!(
                    "Dangerous environment variable '{}' is blocked \
                     (library injection risk)",
                    key
                ),
            });
        }
        if WARNED_ENV_VARS.iter().any(|w| *w == upper) {
            tracing::warn!(
                env_var = %key,
                "Overriding '{}' in child process may affect command resolution",
                key
            );
        }
    }
    Ok(())
}
```

In `src/runtime/executor/verbs.rs`, call before env loop (both modes):

```rust
if let Some(ref env_vars) = params.env {
    crate::ast::security::validate_env_vars(env_vars)?;
    for (key, value) in env_vars {
        let resolved_value = template_resolve(value, bindings, datastore)?;
        cmd.env(key, resolved_value.as_ref());
    }
}
```

**Step 3: Commit**

```bash
git commit -m "fix(security): block dangerous env vars in exec tasks

LD_PRELOAD, DYLD_INSERT_LIBRARIES and similar library injection
env vars are now blocked in exec tasks. PATH-like overrides emit
a warning but are allowed for legitimate use.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

### Task 5: [MEDIUM] Add Size Limit to WriteTool

**The Bug:** WriteTool accepts unlimited content size. ReadTool has `DEFAULT_LIMIT=2000` lines and `MAX_LINE_LENGTH=2000`, but WriteTool has no equivalent.

**Evidence:**
- `src/tools/write.rs:109` -- `file.write_all(params.content.as_bytes())` with no size check
- `src/tools/write.rs:171-186` -- JSON schema has no `maxLength`
- `src/tools/read.rs:71-75` -- ReadTool defines limits (asymmetric protection)

**Files:**
- Modify: `src/tools/write.rs`
- Modify: `src/util/constants.rs` (add constant)
- Test: `src/tools/write.rs` (test module)

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_write_content_size_limit() {
    let ctx = test_tool_context();
    let tool = WriteTool::new(ctx);
    let huge_content = "X".repeat(MAX_WRITE_SIZE + 1);
    let result = tool.execute(WriteParams {
        file_path: "/tmp/test_huge.txt".to_string(),
        content: huge_content,
    }).await;
    assert!(result.is_err(), "Content exceeding MAX_WRITE_SIZE should fail");
}
```

**Step 2: Implement**

In `src/util/constants.rs`:
```rust
/// Maximum content size for WriteTool (10 MB)
pub const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;
```

In `src/tools/write.rs` execute(), after permission check:
```rust
if params.content.len() > MAX_WRITE_SIZE {
    return Err(NikaError::ToolError {
        code: "NIKA-200".to_string(),
        message: format!(
            "Content too large: {} bytes (max: {} bytes / {} MB)",
            params.content.len(),
            MAX_WRITE_SIZE,
            MAX_WRITE_SIZE / (1024 * 1024)
        ),
    });
}
```

**Step 3: Commit**

```bash
git commit -m "fix(tools): add 10MB size limit to WriteTool

WriteTool accepted unlimited content, creating DoS risk.
Now rejects content > MAX_WRITE_SIZE (10 MB). ReadTool already
had limits; this makes protection symmetric.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

### Task 6: [LOW] Strip API Key Env Vars from exec Child Processes

**The Bug:** Child processes spawned by the `exec:` verb inherit ALL parent env vars, including API keys. Untrusted commands can read them.

**Evidence:**
- `src/runtime/executor/verbs.rs:500-514` -- no `env_clear()` or key stripping
- `src/core/providers.rs:67-256` -- lists 18 known provider env var names
- `src/mcp/rmcp_adapter.rs:195-209` -- same issue for MCP subprocesses

**Design Decision:** Strip all known LLM API key env vars from exec child processes. MCP subprocesses are configured by the workflow author and may legitimately need keys -- document the risk but do not block.

**Files:**
- Modify: `src/runtime/executor/verbs.rs:500-514, 540-555`
- Modify: `src/core/providers.rs` (expose API key list as constant)
- Test: `src/runtime/executor/verbs.rs` (test module)

**Step 1: Write the failing test**

```rust
#[test]
fn test_exec_child_env_excludes_api_keys() {
    let sanitized = sanitize_env_for_child(None);
    assert!(!sanitized.contains_key("ANTHROPIC_API_KEY"));
    assert!(!sanitized.contains_key("OPENAI_API_KEY"));
    // Standard vars should be preserved
    assert!(sanitized.contains_key("PATH") || sanitized.contains_key("HOME"));
}
```

**Step 2: Implement**

In `src/core/providers.rs`, add:
```rust
/// API key env vars that should NOT be inherited by exec child processes
pub const SENSITIVE_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "MISTRAL_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "GEMINI_API_KEY",
    "NEO4J_PASSWORD",
    "GITHUB_TOKEN",
    "SLACK_BOT_TOKEN",
    "PERPLEXITY_API_KEY",
    "FIRECRAWL_API_KEY",
    "SUPADATA_API_KEY",
    "DATAFORSEO_API_KEY",
    "AHREFS_API_KEY",
];
```

In `src/runtime/executor/verbs.rs`:
```rust
/// Build sanitized environment for exec child processes.
/// Strips known API keys to prevent secret leakage.
fn sanitize_env_for_child(
    user_env: Option<&FxHashMap<String, String>>,
) -> HashMap<String, String> {
    use crate::core::providers::SENSITIVE_ENV_VARS;
    let mut env: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| !SENSITIVE_ENV_VARS.contains(&k.as_str()))
        .collect();
    if let Some(user) = user_env {
        for (k, v) in user {
            env.insert(k.clone(), v.clone());
        }
    }
    env
}
```

Then in both shell/shell-free blocks:
```rust
let mut cmd = tokio::process::Command::new("sh");
cmd.env_clear();
let sanitized = sanitize_env_for_child(params.env.as_ref());
for (k, v) in &sanitized {
    cmd.env(k, v);
}
```

**Step 3: Commit**

```bash
git commit -m "fix(security): strip API key env vars from exec child processes

Child processes spawned by exec verb now get a sanitized
environment without known API keys (ANTHROPIC_API_KEY, etc.).
User-specified env vars are applied after sanitization.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Batch 3: Runtime Correctness

### Task 7: [LOW] Validate Dangling depends_on References

**The Bug:** When `depends_on` references a task ID that does not exist in the TaskTable, the DAG builder silently skips the edge. The dependent task runs without waiting.

**Evidence:**
- `src/dag/flow.rs:97-118` -- `if let Some(dep_name) = ...` silently skips None

**Design Decision:** Error on missing deps. If a task declares a dependency, it must exist. Silent skip violates workflow semantics.

**CAUTION:** Check whether for_each expansion creates partial TaskTables intentionally. If so, add validation only for the final (post-expansion) DAG.

**Files:**
- Modify: `src/dag/flow.rs:97-118`
- Test: `src/dag/flow.rs` (test module)

**Step 1: Write the failing test**

```rust
#[test]
fn test_dangling_depends_on_returns_error() {
    // Build workflow where task B depends_on task C, but C does not exist
    let wf = build_test_workflow_with_dangling_dep();
    let result = DagFlow::from_analyzed(&wf);
    assert!(result.is_err(), "Dangling depends_on should be rejected");
}
```

**Step 2: Implement**

In `src/dag/flow.rs`, change the `if let Some` to error on `None`:

```rust
for dep_id in &task.depends_on {
    match workflow.task_table.get_name(*dep_id) {
        Some(dep_name) => {
            self.add_edge(source, get_target_index(&dep_name))?;
        }
        None => {
            return Err(NikaError::DagError {
                message: format!(
                    "Task '{}' depends on unknown task (id: {:?})",
                    task.id, dep_id
                ),
            });
        }
    }
}
```

**Step 3: Verify no false positives**

Run full test suite. If for_each tests fail, the DAG validation must happen AFTER for_each expansion, not before.

**Step 4: Commit**

```bash
git commit -m "fix(dag): reject dangling depends_on references

DAG builder silently skipped edges when depends_on referenced a
non-existent task. Now returns error. Dependent tasks must
reference valid task IDs.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

### Task 8: [LOW] Fix turn_index Ambiguity with Odd Message History

**The Bug:** `(self.history.len() / 2 + 1)` yields the same turn index for history lengths 2 and 3 (both give 2). Integer division makes turn tracking ambiguous for odd history lengths.

**Evidence:**
- `src/runtime/rig_agent_loop/chat.rs` -- 6 identical occurrences at lines 118, 208, 305, 371, 440, 502

**Files:**
- Modify: `src/runtime/rig_agent_loop/chat.rs` (6 locations)
- Modify: `src/runtime/rig_agent_loop/mod.rs` or `types.rs` (add turn_count field)
- Test: `src/runtime/rig_agent_loop/tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_turn_count_increments_correctly() {
    let mut agent = make_test_agent();
    assert_eq!(agent.turn_count, 0);
    // After first chat_continue, turn_count should be 1
    // After second, 2, etc.
    // The event turn_index should match
}
```

**Step 2: Implement**

Add `turn_count: u32` field to `RigAgentLoop` struct (in types.rs or mod.rs). Initialize to 0.

Replace all 6 occurrences in chat.rs:
```rust
// BEFORE:
let turn_index = (self.history.len() / 2 + 1) as u32;
// AFTER:
self.turn_count += 1;
let turn_index = self.turn_count;
```

**Step 3: Commit**

```bash
git commit -m "fix(runtime): use explicit turn counter instead of history-derived index

The formula (history.len() / 2 + 1) was ambiguous for odd history
lengths. Replaced with an explicit turn_count field incremented on
each chat_continue call.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Verification Checklist (After All Tasks)

- [ ] `cargo check -p nika` -- clean compile
- [ ] `cargo test -p nika --lib` -- all tests pass (except pre-existing secrets test)
- [ ] `cargo clippy -p nika -- -D warnings` -- zero warnings
- [ ] `cargo fmt -p nika --check` -- clean formatting
- [ ] Each task has its own commit with `type(scope): description` format
- [ ] All commits have co-author lines
- [ ] `git push` -- pushed to main

---

## Execution Order

```
Task 1 (goal->prompt) -> Task 2 (multi-verb) -> Task 3 (empty tasks) -> verify batch 1
Task 4 (env blocklist) -> Task 5 (write limit) -> Task 6 (key strip) -> verify batch 2
Task 7 (dangling deps) -> Task 8 (turn_index) -> verify batch 3
Final verification -> push
```

Each task is independent within its batch. Commits are sequential (1 fix = 1 commit).
