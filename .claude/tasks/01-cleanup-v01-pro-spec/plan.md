# Implementation Plan: v0.1 Rust Types

## Overview

Implement `use:` and `output:` blocks from SPEC-v0.1.md in the Nika CLI codebase.

---

## Phase 1: New Types (src/use_block.rs)

### Task 1.1: Create UseEntry enum

```rust
// src/use_block.rs
use serde::Deserialize;
use serde_json::Value;

pub type UseBlock = std::collections::HashMap<String, UseEntry>;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UseEntry {
    Path(String),
    Batch(Vec<String>),
    Advanced(UseAdvanced),
}

#[derive(Debug, Clone, Deserialize)]
pub struct UseAdvanced {
    #[serde(default)]
    pub pick: Option<Vec<String>>,
    #[serde(default)]
    pub default: Option<Value>,
}
```

**Files:** New `src/use_block.rs`
**Tests:** Parse 3 forms from YAML

### Task 1.2: Create OutputPolicy

```rust
// src/output_policy.rs
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputPolicy {
    #[serde(default)]
    pub format: OutputFormat,
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
```

**Files:** New `src/output_policy.rs`
**Tests:** Parse format + schema

---

## Phase 2: Update Task Struct (src/workflow.rs)

### Task 2.1: Add use/output fields

```rust
#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: String,
    #[serde(default, rename = "use")]
    pub use_block: Option<UseBlock>,
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    #[serde(flatten)]
    pub action: TaskAction,
}
```

**Files:** `src/workflow.rs:22-27`
**Tests:** Parse task with use/output blocks

---

## Phase 3: DataStore v2 (src/datastore.rs)

### Task 3.1: Add Value support

```rust
pub struct DataStore {
    outputs: Arc<RwLock<HashMap<String, Value>>>,
    resolved: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
}
```

### Task 3.2: Add path resolution

```rust
pub fn resolve_path(&self, path: &str) -> Option<Value>
```

### Task 3.3: Add resolved inputs

```rust
pub fn set_resolved(&self, task_id: &str, alias: &str, value: Value)
pub fn get_resolved(&self, task_id: &str, alias: &str) -> Option<Value>
```

**Files:** `src/datastore.rs` (major rewrite)
**Tests:** Path resolution, resolved inputs

---

## Phase 4: Template Update (src/template.rs)

### Task 4.1: New regex pattern

```rust
static USE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)\s*\}\}").unwrap()
});
```

### Task 4.2: Resolve from resolved_inputs

```rust
pub fn resolve_use_template(
    template: &str,
    task_id: &str,
    datastore: &DataStore,
) -> Result<String, NikaError>
```

**Files:** `src/template.rs:8-85`
**Tests:** Template resolution with use.alias

---

## Phase 5: Runner Integration (src/runner.rs)

### Task 5.1: Resolve use block before execution

```rust
// Before executing task:
if let Some(use_block) = &task.use_block {
    resolve_use_block(use_block, &task.id, &datastore)?;
}
```

### Task 5.2: Apply output policy after execution

```rust
// After execution:
if let Some(policy) = &task.output {
    apply_output_policy(output, policy)?;
}
```

**Files:** `src/runner.rs:168-180`
**Tests:** End-to-end workflow with use/output

---

## Phase 6: Error Codes (src/error.rs)

### Task 6.1: Add NIKA-050+ errors

```rust
#[derive(Error, Debug)]
pub enum NikaError {
    #[error("NIKA-050: Invalid path syntax: {0}")]
    InvalidPath(String),

    #[error("NIKA-051: Task not found: {0}")]
    TaskNotFound(String),

    #[error("NIKA-052: Path not found: {0}")]
    PathNotFound(String),

    #[error("NIKA-060: Invalid JSON output")]
    InvalidJson,

    #[error("NIKA-061: Schema validation failed: {0}")]
    SchemaFailed(String),

    #[error("NIKA-070: Duplicate alias: {0}")]
    DuplicateAlias(String),
    // ...
}
```

**Files:** `src/error.rs`
**Tests:** Error messages and fix suggestions

---

## Execution Order

```
Phase 1 (Types)     → No dependencies
Phase 2 (Workflow)  → Depends on Phase 1
Phase 3 (DataStore) → No dependencies
Phase 4 (Template)  → Depends on Phase 3
Phase 5 (Runner)    → Depends on Phase 1-4
Phase 6 (Errors)    → Can run in parallel
```

**Parallel tracks:**
- Track A: Phase 1 → Phase 2 → Phase 5
- Track B: Phase 3 → Phase 4
- Track C: Phase 6

---

## Testing Strategy

1. **Unit tests** per phase
2. **Integration test** with `use-output-demo.nika.yaml`
3. **Error case tests** for NIKA-050+

---

## Estimated Changes

| File | Lines Changed | Type |
|------|---------------|------|
| `src/use_block.rs` | +50 | NEW |
| `src/output_policy.rs` | +30 | NEW |
| `src/workflow.rs` | +10 | UPDATE |
| `src/datastore.rs` | +80 | REWRITE |
| `src/template.rs` | +40 | UPDATE |
| `src/runner.rs` | +30 | UPDATE |
| `src/error.rs` | +30 | UPDATE |
| `src/lib.rs` | +4 | UPDATE |
| **Total** | ~275 | - |
