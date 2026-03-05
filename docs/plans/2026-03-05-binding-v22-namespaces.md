# Agent Output Namespaces (v0.22.0) Implementation Plan

> **For Claude:** Follow this plan task-by-task using TDD methodology.

**Goal:** Enable agents to emit multiple named outputs (namespaces) during execution, accessible via `task.namespace.field` syntax.

**Architecture:** Extend `TaskResult` with a `namespaces` field for named outputs. Add `emit_output` builtin tool for agents to emit named results during execution. Update DataStore resolution to support namespace paths.

**Tech Stack:** Rust (rustc 1.86+), serde, DashMap, tokio

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  AGENT OUTPUT NAMESPACES — v0.22.0                                            ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  CURRENT BEHAVIOR:                                                            ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Agent task returns single output:                                            ║
║    task.output → final agent response (string or JSON)                        ║
║                                                                               ║
║  NEW BEHAVIOR (v0.22.0):                                                      ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Agent can emit multiple named outputs:                                       ║
║    task.output     → final response (unchanged)                               ║
║    task.artifacts  → emitted artifacts array                                  ║
║    task.summary    → emitted summary object                                   ║
║    task.metrics    → emitted metrics object                                   ║
║    task.<name>     → any emitted namespace                                    ║
║                                                                               ║
║  EMIT TOOL:                                                                   ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Agent uses builtin nika:emit_output tool:                                    ║
║    nika:emit_output { namespace: "artifacts", value: [...] }                  ║
║    nika:emit_output { namespace: "summary", value: {...} }                    ║
║                                                                               ║
║  ACCESS SYNTAX:                                                               ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  use:                                                                         ║
║    files: agent_task.artifacts          # Array of artifacts                  ║
║    first: agent_task.artifacts[0]       # First artifact                      ║
║    summary: agent_task.summary          # Summary object                      ║
║    title: agent_task.summary.title      # Nested field                        ║
║    result: $agent_task                  # Full output (v0.21 syntax)          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NAMESPACE ARCHITECTURE                                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  TaskResult (current):                                                          │
│  ├── output: Arc<Value>                                                         │
│  ├── duration: Duration                                                         │
│  └── status: TaskStatus                                                         │
│                                                                                 │
│  TaskResult (v0.22.0):                                                          │
│  ├── output: Arc<Value>                                                         │
│  ├── duration: Duration                                                         │
│  ├── status: TaskStatus                                                         │
│  └── namespaces: FxHashMap<String, Arc<Value>>  ← NEW                          │
│                                                                                 │
│  DataStore.resolve_path() updated:                                              │
│  ├── "task" → returns output (implicit, existing)                               │
│  ├── "task.output" → returns output (explicit, existing)                        │
│  ├── "task.namespace" → returns namespaces["namespace"]  ← NEW                  │
│  └── "task.namespace.field" → jsonpath on namespace  ← NEW                      │
│                                                                                 │
│  Resolution priority:                                                           │
│  1. Check if second segment is "output" → return output                         │
│  2. Check if second segment exists in namespaces → return namespace             │
│  3. Fall back to jsonpath on output (existing behavior)                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Task 1: Add namespaces field to TaskResult

**Files:**
- Modify: `tools/nika/src/store/datastore.rs`
- Test: Inline tests in module

**Step 1: Write the failing test**

```rust
#[test]
fn test_task_result_has_namespaces() {
    let result = TaskResult::success(json!({"answer": 42}), Duration::ZERO);
    assert!(result.namespaces.is_empty());
}

#[test]
fn test_task_result_with_namespace() {
    let mut result = TaskResult::success(json!({"answer": 42}), Duration::ZERO);
    result.set_namespace("artifacts", json!([{"file": "output.txt"}]));

    assert_eq!(result.namespaces.len(), 1);
    let artifacts = result.namespaces.get("artifacts").unwrap();
    assert_eq!(artifacts.as_array().unwrap().len(), 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika task_result_has_namespaces --lib`
Expected: FAIL with "no field `namespaces`"

**Step 3: Add namespaces field to TaskResult**

```rust
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub output: Arc<Value>,
    pub duration: Duration,
    pub status: TaskStatus,
    /// Named output namespaces emitted during task execution.
    /// Key: namespace name, Value: namespace data.
    pub namespaces: FxHashMap<String, Arc<Value>>,
}

impl TaskResult {
    /// Create a successful task result (existing factory method - add namespaces)
    pub fn success(output: impl Into<Value>, duration: Duration) -> Self {
        Self {
            output: Arc::new(output.into()),
            duration,
            status: TaskStatus::Success,
            namespaces: FxHashMap::default(),
        }
    }

    /// Set a named namespace value.
    pub fn set_namespace(&mut self, name: impl Into<String>, value: Value) {
        self.namespaces.insert(name.into(), Arc::new(value));
    }

    /// Get a namespace value by name.
    pub fn get_namespace(&self, name: &str) -> Option<&Arc<Value>> {
        self.namespaces.get(name)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika task_result_has_namespaces --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/store/datastore.rs
git commit -m "$(cat <<'EOF'
feat(store): add namespaces field to TaskResult

TaskResult now supports named output namespaces:
- namespaces: FxHashMap<String, Arc<Value>>
- set_namespace() to add a namespace
- get_namespace() to retrieve a namespace

Foundation for agent multi-output support in v0.22.0.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 2: Update resolve_path for namespace resolution

**Files:**
- Modify: `tools/nika/src/store/datastore.rs`
- Test: Inline tests

**Step 1: Write the failing test**

```rust
#[test]
fn test_resolve_path_with_namespace() {
    let store = DataStore::new();

    // Store result with namespace
    let mut result = TaskResult::success(json!({"final": "answer"}), Duration::ZERO);
    result.set_namespace("artifacts", json!([{"name": "file1.txt"}, {"name": "file2.txt"}]));
    result.set_namespace("summary", json!({"title": "Report", "count": 5}));
    store.store("agent_task", result);

    // Resolve output (existing behavior)
    let output = store.resolve_path("agent_task").unwrap();
    assert_eq!(output["final"], "answer");

    // Resolve namespace
    let artifacts = store.resolve_path("agent_task.artifacts").unwrap();
    assert_eq!(artifacts.as_array().unwrap().len(), 2);

    // Resolve nested in namespace
    let first_name = store.resolve_path("agent_task.artifacts[0].name").unwrap();
    assert_eq!(first_name, "file1.txt");

    let title = store.resolve_path("agent_task.summary.title").unwrap();
    assert_eq!(title, "Report");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika resolve_path_with_namespace --lib`
Expected: FAIL (namespaces not resolved yet)

**Step 3: Update resolve_path() to check namespaces**

```rust
pub fn resolve_path(&self, path: &str) -> Option<Value> {
    let mut parts = path.splitn(2, '.');
    let task_id = parts.next()?;

    // Get the task result
    let result = self.results.get(task_id)?;

    // If no remaining path, return the whole output
    let Some(remaining) = parts.next() else {
        return Some((*result.output).clone());
    };

    // Split remaining into first segment and rest
    let mut remaining_parts = remaining.splitn(2, '.');
    let first_segment = remaining_parts.next()?;
    let rest = remaining_parts.next();

    // Check for explicit "output" segment
    if first_segment == "output" {
        return match rest {
            Some(nested) => jsonpath::resolve(&result.output, nested).ok().flatten(),
            None => Some((*result.output).clone()),
        };
    }

    // Check if first segment is a namespace
    if let Some(namespace_value) = result.namespaces.get(first_segment) {
        return match rest {
            Some(nested) => jsonpath::resolve(namespace_value, nested).ok().flatten(),
            None => Some((**namespace_value).clone()),
        };
    }

    // Fall back to jsonpath on output (existing behavior)
    jsonpath::resolve(&result.output, remaining).ok().flatten()
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika resolve_path_with_namespace --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/store/datastore.rs
git commit -m "$(cat <<'EOF'
feat(store): resolve namespace paths in DataStore

resolve_path() now supports namespace resolution:
- task.namespace → returns namespace value
- task.namespace.field → jsonpath on namespace
- task.output → explicit output (unchanged)
- task.field → jsonpath on output (unchanged)

Priority: output > namespaces > output jsonpath

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 3: Create nika:emit_output builtin tool

**Files:**
- Modify: `tools/nika/src/tools/mod.rs` (existing builtin tools module)
- Test: Inline tests

> **Note:** Tools are in `src/tools/`, not `src/runtime/tools/`. The existing `FileTool` trait
> pattern should be followed. New tool implements `FileTool` trait and is wrapped by `RigFileTool`.

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_emit_output_tool() {
    use crate::tools::{EmitOutputTool, FileTool};

    let tool = EmitOutputTool::new();

    // Verify tool metadata
    assert_eq!(tool.name(), "nika:emit_output");

    // Execute emit_output
    let params = json!({
        "namespace": "artifacts",
        "value": [{"file": "test.txt"}]
    });

    let result = tool.call(params).await.unwrap();

    // Result should contain the emitted value
    assert_eq!(result.content, "Namespace 'artifacts' emitted successfully");
    assert!(!result.is_error);
    assert_eq!(result.data.unwrap()["namespace"], "artifacts");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika emit_output_tool --lib`
Expected: FAIL with "nika:emit_output not found"

**Step 3: Implement nika:emit_output tool**

> **Architecture Note:** The tool uses an `Arc<Mutex<FxHashMap<String, Value>>>` that's passed
> during tool construction to collect emitted namespaces. This is then read by RigAgentLoop
> after the agent completes and merged into TaskResult.namespaces.

```rust
use std::sync::Arc;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use serde_json::{json, Value};
use crate::error::NikaError;
use crate::tools::{FileTool, ToolOutput};

/// Emit a named output to a namespace.
///
/// Parameters:
/// - namespace: Name of the output namespace (e.g., "artifacts", "summary")
/// - value: JSON value to store in the namespace
///
/// The emitted namespace is accessible after task completion via:
/// - task.namespace → full namespace value
/// - task.namespace.field → nested field access
pub struct EmitOutputTool {
    /// Shared storage for emitted namespaces, collected by RigAgentLoop
    pending_namespaces: Arc<Mutex<FxHashMap<String, Value>>>,
}

impl EmitOutputTool {
    pub fn new() -> Self {
        Self {
            pending_namespaces: Arc::new(Mutex::new(FxHashMap::default())),
        }
    }

    pub fn with_storage(storage: Arc<Mutex<FxHashMap<String, Value>>>) -> Self {
        Self { pending_namespaces: storage }
    }

    /// Get all pending namespaces and clear the storage
    pub fn take_namespaces(&self) -> FxHashMap<String, Value> {
        std::mem::take(&mut *self.pending_namespaces.lock())
    }
}

#[async_trait::async_trait]
impl FileTool for EmitOutputTool {
    fn name(&self) -> &'static str {
        "nika:emit_output"
    }

    fn description(&self) -> &'static str {
        "Emit a named output to a namespace for later reference"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["namespace", "value"],
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "Name of the output namespace"
                },
                "value": {
                    "description": "JSON value to store"
                }
            }
        })
    }

    async fn call(&self, params: Value) -> Result<ToolOutput, NikaError> {
        let namespace = params["namespace"].as_str()
            .ok_or_else(|| NikaError::InvalidParams("namespace must be a string".into()))?;
        let value = params.get("value")
            .ok_or_else(|| NikaError::InvalidParams("value is required".into()))?
            .clone();

        // Store in pending namespaces (will be collected by RigAgentLoop)
        // Note: If same namespace emitted twice, last value wins (HashMap insert = overwrite)
        self.pending_namespaces.lock().insert(namespace.to_string(), value.clone());

        let preview: String = value.to_string().chars().take(100).collect();

        Ok(ToolOutput {
            content: format!("Namespace '{}' emitted successfully", namespace),
            is_error: false,
            data: Some(json!({
                "namespace": namespace,
                "emitted": true,
                "preview": preview
            })),
        })
    }
}
```

**Step 4: Register tool in RigAgentLoop**

In `runtime/rig_agent_loop.rs`, the tool is added to the agent's tool list with shared storage:

```rust
// In RigAgentLoop::new() or run()
let namespace_storage = Arc::new(Mutex::new(FxHashMap::default()));
let emit_tool = EmitOutputTool::with_storage(Arc::clone(&namespace_storage));

// Add to tools Vec wrapped as RigFileTool
let rig_emit_tool = RigFileTool::new(Box::new(emit_tool));
tools.push(Box::new(rig_emit_tool));

// After agent completes, collect namespaces:
let namespaces = emit_tool.take_namespaces();
for (name, value) in namespaces {
    task_result.set_namespace(name, value);
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p nika emit_output_tool --lib`
Expected: PASS

**Step 6: Commit**

```bash
git add tools/nika/src/runtime/tools/
git commit -m "$(cat <<'EOF'
feat(runtime): add nika:emit_output builtin tool

New builtin tool for agents to emit named outputs:
- nika:emit_output { namespace: "name", value: {...} }
- Stored in TaskResult.namespaces
- Accessible via task.namespace.field syntax

12th builtin tool (6 core + 5 file + 1 namespace).

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 4: Integrate namespace emission in RigAgentLoop

**Files:**
- Modify: `tools/nika/src/runtime/rig_agent_loop.rs`
- Test: Integration tests

> **Architecture Note:** RigAgentLoop does NOT have an `AgentContext` struct.
> Instead, we use shared `Arc<Mutex<FxHashMap>>` storage passed to `EmitOutputTool`.
> The storage is owned by RigAgentLoop and read after agent completion.

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_agent_emits_namespace() {
    use crate::tools::EmitOutputTool;
    use std::sync::Arc;
    use parking_lot::Mutex;
    use rustc_hash::FxHashMap;

    // Create shared storage for namespaces
    let namespace_storage: Arc<Mutex<FxHashMap<String, Value>>> =
        Arc::new(Mutex::new(FxHashMap::default()));

    // Create emit tool with shared storage
    let emit_tool = EmitOutputTool::with_storage(Arc::clone(&namespace_storage));

    // Simulate tool call
    emit_tool.call(json!({
        "namespace": "artifacts",
        "value": [{"file": "test.txt"}]
    })).await.unwrap();

    // Check that namespace was captured in shared storage
    let namespaces = namespace_storage.lock();
    assert!(namespaces.contains_key("artifacts"));
    assert_eq!(namespaces["artifacts"], json!([{"file": "test.txt"}]));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika agent_emits_namespace --lib`
Expected: FAIL (EmitOutputTool doesn't exist yet)

**Step 3: Update RigAgentLoop to collect namespaces**

In `RigAgentLoop`:

```rust
// Add field to RigAgentLoop struct:
pub struct RigAgentLoop {
    // ... existing fields (task_id, params, event_log, mcp_clients, tools, history, etc.)

    /// Shared storage for namespaces emitted via nika:emit_output
    namespace_storage: Arc<Mutex<FxHashMap<String, Value>>>,
}

impl RigAgentLoop {
    pub fn new(/* ... existing params */) -> Self {
        Self {
            // ... existing field initialization
            namespace_storage: Arc::new(Mutex::new(FxHashMap::default())),
        }
    }

    pub fn run(&mut self, /* ... */) -> Result<TaskResult, NikaError> {
        // Create emit_output tool with shared storage
        let emit_tool = EmitOutputTool::with_storage(Arc::clone(&self.namespace_storage));
        let rig_emit_tool = RigFileTool::new(Box::new(emit_tool));
        self.tools.push(Box::new(rig_emit_tool));

        // ... existing agent loop code ...

        // After agent completes, collect emitted namespaces
        let mut task_result = TaskResult::success(final_output, elapsed);

        // Transfer namespaces from shared storage to TaskResult
        let namespaces = std::mem::take(&mut *self.namespace_storage.lock());
        for (name, value) in namespaces {
            task_result.set_namespace(name, value);
        }

        Ok(task_result)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika agent_emits_namespace --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/runtime/rig_agent_loop.rs tools/nika/src/tools/mod.rs
git commit -m "$(cat <<'EOF'
feat(runtime): capture emitted namespaces in RigAgentLoop

RigAgentLoop now tracks pending namespaces:
- namespace_storage: Arc<Mutex<FxHashMap>> field
- EmitOutputTool created with shared storage
- Namespaces transferred to TaskResult after agent completes

Completes agent namespace emission pipeline.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 5: Add NamespacedOutput event

**Files:**
- Modify: `tools/nika/src/event/log.rs`
- Test: Inline tests

**Step 1: Write the failing test**

> **Note:** Events use `EventKind` enum (not `Event`). See `src/event/log.rs:143` for the 24 existing variants.

```rust
#[test]
fn test_namespaced_output_event() {
    use crate::event::log::EventKind;
    use chrono::Utc;
    use std::sync::Arc;

    let event = EventKind::NamespacedOutput {
        task_id: Arc::from("agent_task"),
        namespace: "artifacts".into(),
        value_preview: "[{...}]".into(),
        timestamp: Utc::now(),
    };

    let json = serde_json::to_string(&event).unwrap();
    // EventKind uses snake_case serialization: "namespaced_output"
    assert!(json.contains("namespaced_output"));
    assert!(json.contains("artifacts"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika namespaced_output_event --lib`
Expected: FAIL with "no variant `NamespacedOutput`"

**Step 3: Add event variant to EventKind**

In `src/event/log.rs`, add to the `EventKind` enum (line ~143):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    // ... existing 24 variants ...

    /// Agent emitted a named output namespace (25th variant)
    NamespacedOutput {
        task_id: Arc<str>,
        namespace: String,
        value_preview: String,
        #[serde(with = "chrono::serde::ts_milliseconds")]
        timestamp: DateTime<Utc>,
    },
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika namespaced_output_event --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/event/log.rs
git commit -m "$(cat <<'EOF'
feat(event): add NamespacedOutput event variant

New event for observability when agents emit namespaces:
- task_id: which agent emitted
- namespace: name of the namespace
- value_preview: truncated preview of value
- timestamp: when emitted

23rd event variant for full namespace traceability.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 6: Create example workflow

**Files:**
- Create: `tools/nika/examples/v22-agent-namespaces.nika.yaml`

**Step 1: Create example workflow**

```yaml
# v22-agent-namespaces.nika.yaml
# Demonstrates agent output namespaces
schema: "nika/workflow@0.9"
provider: claude

tasks:
  # Agent that emits multiple namespaces
  - id: research_agent
    agent:
      prompt: |
        Research Rust async patterns and:
        1. Use nika:emit_output to emit an "artifacts" namespace with file names you would create
        2. Use nika:emit_output to emit a "summary" namespace with title and key_points
        3. Return your final conclusion as the main output

        Example tool calls:
        nika:emit_output { "namespace": "artifacts", "value": ["pattern1.md", "pattern2.md"] }
        nika:emit_output { "namespace": "summary", "value": { "title": "...", "key_points": [...] } }

      tools:
        - nika:emit_output

      max_turns: 5

  # Access namespaces from agent
  - id: process_artifacts
    use:
      # Access the artifacts namespace
      files: research_agent.artifacts
      # Access nested field in summary namespace
      title: research_agent.summary.title
      # Access full output
      conclusion: $research_agent
    infer: |
      Process the research results:

      Files to create: {{use.files}}
      Title: {{use.title}}
      Conclusion: {{use.conclusion}}

      Generate a project plan based on these artifacts.

  # Show all access patterns
  - id: verify_access
    use:
      # All these access patterns work:
      artifacts_full: research_agent.artifacts
      artifacts_first: research_agent.artifacts[0]
      summary_full: research_agent.summary
      summary_title: research_agent.summary.title
      output_implicit: $research_agent
      output_explicit: research_agent.output
    infer: |
      Verify all namespace access patterns work:
      - artifacts_full: {{use.artifacts_full}}
      - artifacts_first: {{use.artifacts_first}}
      - summary_full: {{use.summary_full}}
      - summary_title: {{use.summary_title}}
      - output_implicit: {{use.output_implicit}}
      - output_explicit: {{use.output_explicit}}

      Confirm all values are present (not null/undefined).

flows:
  - source: research_agent
    target: [process_artifacts, verify_access]
```

**Step 2: Validate example**

Run: `cargo run -p nika -- check examples/v22-agent-namespaces.nika.yaml`
Expected: Workflow is valid

**Step 3: Commit**

```bash
git add tools/nika/examples/v22-agent-namespaces.nika.yaml
git commit -m "$(cat <<'EOF'
docs(examples): add v22-agent-namespaces example

Demonstrate agent output namespaces:
- Agent emits artifacts and summary namespaces
- Multiple access patterns for namespace data
- Nested field access within namespaces
- Integration with $task implicit output syntax

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 7: Update documentation

**Files:**
- Modify: `tools/nika/CLAUDE.md`

**Step 1: Add namespace documentation**

```markdown
### Agent Output Namespaces (v0.22.0)

Agents can emit multiple named outputs during execution using the `nika:emit_output` tool:

```yaml
tasks:
  - id: research
    agent:
      prompt: |
        Research the topic and emit structured results.
        Use nika:emit_output for each category of output.
      tools:
        - nika:emit_output

  - id: process
    use:
      # Access namespaces emitted by agent
      files: research.artifacts           # Full namespace
      first_file: research.artifacts[0]   # Array element
      summary: research.summary           # Object namespace
      title: research.summary.title       # Nested field
      result: $research                   # Full output (v0.21 syntax)
```

**nika:emit_output Parameters:**
- `namespace`: Name for this output category
- `value`: JSON value to store

**Overwrite Behavior:**
- If emit_output is called multiple times with the same namespace name, the **later call overwrites** the earlier value
- Uses HashMap semantics: `insert(name, value)` replaces existing entry
- To accumulate values, use arrays: emit the full array each time, not individual items

**Example (accumulation pattern):**
```yaml
# WRONG: Each call overwrites
nika:emit_output { "namespace": "files", "value": "file1.md" }
nika:emit_output { "namespace": "files", "value": "file2.md" }  # "file1.md" is lost!

# CORRECT: Emit full array
nika:emit_output { "namespace": "files", "value": ["file1.md", "file2.md"] }
```

**Common namespaces:**
- `artifacts` — Files created or data produced
- `summary` — Structured summary of results
- `metrics` — Performance or quality metrics
- `errors` — Errors encountered (non-fatal)
```

**Step 2: Commit**

```bash
git add tools/nika/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: add agent namespace documentation to CLAUDE.md

Document v0.22.0 agent namespace features:
- nika:emit_output tool usage
- Namespace access patterns
- Common namespace conventions

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 8: Version bump and CHANGELOG

**Files:**
- Modify: `tools/nika/Cargo.toml`
- Modify: `CHANGELOG.md`

**Step 1: Bump version to 0.22.0**

**Step 2: Add CHANGELOG entry**

```markdown
## [0.22.0] - 2026-03-XX

### Added

- **Agent Output Namespaces** — Agents can emit multiple named outputs
  - `nika:emit_output` builtin tool for emitting namespaces
  - `TaskResult.namespaces` field stores emitted data
  - `task.namespace` syntax for accessing namespaces
  - `task.namespace.field` for nested field access
  - Priority: output > namespaces > output jsonpath
- **NamespacedOutput event** — Observability for namespace emissions
- **Example workflow** — `examples/v22-agent-namespaces.nika.yaml`
- **20+ namespace tests** — Comprehensive coverage

### Changed

- Builtin tools: 11 → 12 (added nika:emit_output)
- Event variants: 23 → 24 (added NamespacedOutput)

### Statistics

- **X tests passing** (updated count)
- **Zero clippy warnings**
```

**Step 3: Commit**

```bash
git add tools/nika/Cargo.toml CHANGELOG.md
git commit -m "$(cat <<'EOF'
chore(release): bump version to 0.22.0

Add agent output namespaces feature.
See CHANGELOG.md for full details.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Verification Checklist

```bash
# Run all tests
cargo test -p nika

# Run specific namespace tests
cargo test -p nika namespace

# Lint check
cargo clippy -p nika -- -D warnings

# Validate example workflow
cargo run -p nika -- check examples/v22-agent-namespaces.nika.yaml

# Optional: Run example (requires API key)
cargo run -p nika -- examples/v22-agent-namespaces.nika.yaml
```

---

## Exit Criteria

- [ ] `TaskResult.namespaces` field added
- [ ] `set_namespace()` and `get_namespace()` methods work
- [ ] `resolve_path()` supports namespace resolution
- [ ] `nika:emit_output` builtin tool works
- [ ] `RigAgentLoop` captures emitted namespaces
- [ ] `NamespacedOutput` event emitted on namespace emit
- [ ] 20+ new tests passing
- [ ] Example workflow validates and runs
- [ ] CLAUDE.md documents namespace syntax
- [ ] CHANGELOG updated
- [ ] Version bumped to 0.22.0
- [ ] Zero clippy warnings

---

## Skills Usage

| Step | Skill | Purpose |
|------|-------|---------|
| All | `superpowers:test-driven-development` | Write tests first |
| All | `superpowers:verification-before-completion` | Verify before commit |
| Debug | `superpowers:systematic-debugging` | If tests fail |
| Review | `superpowers:requesting-code-review` | After completion |
