# v0.14.2 Implementation Plan: context: + include:

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace `memory:` with `context:` (CLEAN BREAK, no aliases) and add `include:` for DAG fusion.

**Architecture:**
- `context:` replaces `memory:` completely (NO backward compat, NO aliases)
- `include:` merges tasks from external workflows into main DAG at parse time (shared DataStore)
- Schema @0.9 is the new minimum for these features

**Tech Stack:** Rust, serde, rustc-hash FxHashMap, petgraph

---

## IMPORTANT: Clean Break Policy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🚨 NO BACKWARD COMPATIBILITY - CLEAN BREAK                                   ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ❌ NO deprecated type aliases (MemoryConfig, LoadedMemory, etc.)             ║
║  ❌ NO memory: field in Workflow struct                                       ║
║  ❌ NO {{memory.*}} binding patterns                                          ║
║  ❌ NO set_memory(), get_memory_file() methods                                ║
║  ❌ NO MemoryLoadError (only ContextLoadError)                                ║
║                                                                               ║
║  ✅ ONLY context: field                                                       ║
║  ✅ ONLY {{context.*}} bindings                                               ║
║  ✅ ONLY ContextConfig, LoadedContext, ContextLoadError                       ║
║  ✅ ONLY set_context(), get_context_file() methods                            ║
║                                                                               ║
║  Old workflows using memory: will get parse errors → users must migrate       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

## Required Skills

- **TDD:** Write failing test → Run to see fail → Implement → Run to see pass → Commit
- **Architecture:** Clean module boundaries, no circular deps
- **Code Review:** After each phase, review for quality
- **Testing:** All features must have tests, run full suite before shipping

---

## Phase 1: context: Field Migration (~6h)

### Task 1.1: Rename AST Module (memory.rs → context.rs)

**Files:**
- Rename: `src/ast/memory.rs` → `src/ast/context.rs`
- Modify: `src/ast/mod.rs`

**Step 1: Rename the file**

```bash
cd /Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika
git mv src/ast/memory.rs src/ast/context.rs
```

**Step 2: Update struct names in context.rs**

Replace in `src/ast/context.rs`:
- `MemoryConfig` → `ContextConfig`
- All doc comments mentioning "memory" → "context"

```rust
//! Context configuration for workflow (v0.9)
//!
//! The `context:` block in a workflow allows loading files at workflow start.
//! Files are loaded into the DataStore and accessible via `{{context.files.alias}}` bindings.
//!
//! # Example
//!
//! ```yaml
//! context:
//!   files:
//!     brand: ./context/brand.md        # Markdown → string
//!     persona: ./context/persona.json  # JSON → parsed object
//!     examples: ./context/*.md         # Glob → array of strings
//!   session: .nika/sessions/prev.json  # Session restore
//! ```

use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Context configuration for workflow (v0.9)
///
/// Defines files to load at workflow start and optional session restoration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContextConfig {
    /// Files to load at workflow start
    ///
    /// Key is the alias, value is the file path (supports glob patterns).
    /// - Single files: loaded as string (markdown, txt) or parsed (json, yaml)
    /// - Glob patterns: loaded as array of strings
    #[serde(default)]
    pub files: FxHashMap<String, String>,

    /// Session file to restore
    ///
    /// Path to a JSON file containing previous session data.
    /// Accessible via `{{context.session.key}}` bindings.
    pub session: Option<String>,
}
```

**Step 3: Update test names in context.rs**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert!(config.files.is_empty());
        assert!(config.session.is_none());
    }

    #[test]
    fn test_context_config_deserialize_empty() {
        let yaml = "";
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap_or_default();
        assert!(config.files.is_empty());
    }

    #[test]
    fn test_context_config_deserialize_files() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  persona: ./context/persona.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert_eq!(
            config.files.get("brand"),
            Some(&"./context/brand.md".to_string())
        );
    }

    #[test]
    fn test_context_config_deserialize_session() {
        let yaml = r#"
session: .nika/sessions/prev.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.session, Some(".nika/sessions/prev.json".to_string()));
    }

    #[test]
    fn test_context_config_deserialize_full() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  examples: ./context/*.md
session: .nika/sessions/prev.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(config.session.is_some());
    }

    #[test]
    fn test_context_config_glob_pattern() {
        let yaml = r#"
files:
  examples: ./context/*.md
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.files.get("examples").unwrap().contains('*'));
    }
}
```

**Step 4: Update mod.rs exports**

In `src/ast/mod.rs`, replace:
```rust
pub mod memory;
pub use memory::MemoryConfig;
```
With:
```rust
pub mod context;
pub use context::ContextConfig;

// Backward compatibility alias (deprecated)
#[deprecated(since = "0.14.2", note = "Use ContextConfig instead")]
pub type MemoryConfig = ContextConfig;
```

**Step 5: Run tests to verify**

```bash
cargo test --lib ast::context -- --nocapture
```
Expected: All 6 tests pass

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ast): rename memory.rs to context.rs

- MemoryConfig → ContextConfig
- memory: → context: in YAML
- Add deprecated MemoryConfig type alias for backward compat

Part of v0.14.2 context: migration"
```

---

### Task 1.2: Update Workflow Struct for Dual Syntax

**Files:**
- Modify: `src/ast/workflow.rs`

**Step 1: Add test for context: parsing**

Add to `src/ast/workflow.rs` tests:

```rust
#[test]
fn test_workflow_parse_v09_with_context() {
    let yaml = r#"
schema: nika/workflow@0.9
workflow: test-context
context:
  files:
    brand: ./context/brand.md
tasks:
  - id: gen
    infer: "Using brand context"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.context.is_some());
    let ctx = workflow.context.unwrap();
    assert_eq!(ctx.files.get("brand"), Some(&"./context/brand.md".to_string()));
}

#[test]
fn test_workflow_parse_memory_deprecated_alias() {
    let yaml = r#"
schema: nika/workflow@0.6
workflow: test-memory-compat
memory:
  files:
    brand: ./context/brand.md
tasks:
  - id: gen
    infer: "Using brand context"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    // memory: should be parsed into context field
    assert!(workflow.context.is_some());
}
```

**Step 2: Run tests to see them fail**

```bash
cargo test --lib ast::workflow::tests::test_workflow_parse_v09 -- --nocapture
```
Expected: FAIL (context field doesn't exist yet)

**Step 3: Update Workflow struct**

In `src/ast/workflow.rs`, find the Workflow struct and update:

```rust
use super::context::ContextConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct Workflow {
    pub schema: String,
    pub workflow: String,
    #[serde(default)]
    pub description: Option<String>,

    // NEW: context: field (preferred)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    // DEPRECATED: memory: field (alias for backward compat)
    #[serde(default)]
    pub memory: Option<ContextConfig>,

    // ... rest of fields
}

impl Workflow {
    /// Get context config, preferring `context:` over deprecated `memory:`
    pub fn get_context(&self) -> Option<&ContextConfig> {
        self.context.as_ref().or(self.memory.as_ref())
    }
}
```

**Step 4: Run tests to verify**

```bash
cargo test --lib ast::workflow -- --nocapture
```
Expected: All tests pass including new context tests

**Step 5: Commit**

```bash
git add src/ast/workflow.rs
git commit -m "feat(ast): add context: field with memory: alias

- Workflow.context: Option<ContextConfig> (new)
- Workflow.memory: Option<ContextConfig> (deprecated alias)
- Workflow.get_context() prefers context: over memory:

Part of v0.14.2 context: migration"
```

---

### Task 1.3: Rename Runtime Loader (memory_loader.rs → context_loader.rs)

**Files:**
- Rename: `src/runtime/memory_loader.rs` → `src/runtime/context_loader.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: Rename the file**

```bash
git mv src/runtime/memory_loader.rs src/runtime/context_loader.rs
```

**Step 2: Update all names in context_loader.rs**

- `LoadedMemory` → `LoadedContext`
- `load_memory` → `load_context`
- All doc comments

**Step 3: Update mod.rs exports**

In `src/runtime/mod.rs`:
```rust
pub mod context_loader;
pub use context_loader::{load_context, LoadedContext};

// Backward compatibility aliases
#[deprecated(since = "0.14.2", note = "Use load_context instead")]
pub use context_loader::load_context as load_memory;
#[deprecated(since = "0.14.2", note = "Use LoadedContext instead")]
pub type LoadedMemory = LoadedContext;
```

**Step 4: Run tests**

```bash
cargo test --lib runtime::context_loader -- --nocapture
```

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor(runtime): rename memory_loader to context_loader

- LoadedMemory → LoadedContext
- load_memory() → load_context()
- Add deprecated aliases for backward compat

Part of v0.14.2 context: migration"
```

---

### Task 1.4: Update Runner to Use Context

**Files:**
- Modify: `src/runtime/runner.rs`

**Step 1: Find and replace memory references**

Search for `memory` in runner.rs and update:
- `use crate::runtime::memory_loader` → `use crate::runtime::context_loader`
- `load_memory(` → `load_context(`
- `workflow.memory` → `workflow.get_context()`

**Step 2: Run tests**

```bash
cargo test --lib runtime::runner -- --nocapture
```

**Step 3: Commit**

```bash
git add src/runtime/runner.rs
git commit -m "refactor(runtime): update runner to use context_loader

Part of v0.14.2 context: migration"
```

---

### Task 1.5: Update DataStore

**Files:**
- Modify: `src/store/datastore.rs`

**Step 1: Add test for context methods**

```rust
#[test]
fn test_datastore_set_context() {
    let store = DataStore::new();
    let mut files = FxHashMap::default();
    files.insert("brand".to_string(), serde_json::json!("Brand content"));

    let context = LoadedContext { files, session: None };
    store.set_context(context);

    assert_eq!(
        store.get_context_file("brand"),
        Some(serde_json::json!("Brand content"))
    );
}

#[test]
fn test_datastore_resolve_context_path() {
    let store = DataStore::new();
    let mut files = FxHashMap::default();
    files.insert("brand".to_string(), serde_json::json!({"name": "Acme"}));

    let context = LoadedContext { files, session: None };
    store.set_context(context);

    // Test path resolution
    let result = store.resolve_context_path("context.files.brand.name");
    assert_eq!(result, Some(serde_json::json!("Acme")));
}
```

**Step 2: Update DataStore methods**

Rename methods:
- `set_memory()` → `set_context()`
- `get_memory_file()` → `get_context_file()`
- `resolve_memory_path()` → `resolve_context_path()`

Add deprecated aliases:
```rust
#[deprecated(since = "0.14.2", note = "Use set_context instead")]
pub fn set_memory(&self, context: LoadedContext) {
    self.set_context(context)
}
```

**Step 3: Run tests**

```bash
cargo test --lib store::datastore -- --nocapture
```

**Step 4: Commit**

```bash
git add src/store/datastore.rs
git commit -m "refactor(store): rename memory methods to context

- set_memory() → set_context()
- get_memory_file() → get_context_file()
- resolve_memory_path() → resolve_context_path()
- Add deprecated aliases

Part of v0.14.2 context: migration"
```

---

### Task 1.6: Update Binding Templates

**Files:**
- Modify: `src/binding/template.rs`

**Step 1: Add test for {{context.*}} binding**

```rust
#[test]
fn test_template_context_binding() {
    let template = "Hello {{context.files.brand}}";
    let store = DataStore::new();
    // ... setup context

    let result = resolve_template(template, &store)?;
    assert!(result.contains("Brand content"));
}

#[test]
fn test_template_memory_binding_deprecated() {
    // Old syntax should still work
    let template = "Hello {{memory.files.brand}}";
    let store = DataStore::new();
    // ... setup context

    let result = resolve_template(template, &store)?;
    assert!(result.contains("Brand content"));
}
```

**Step 2: Update regex patterns**

Add `{{context.*}}` pattern alongside existing `{{memory.*}}`:

```rust
lazy_static! {
    // New pattern (preferred)
    static ref CONTEXT_RE: Regex = Regex::new(r"\{\{context\.([^}]+)\}\}").unwrap();
    // Old pattern (deprecated, still works)
    static ref MEMORY_RE: Regex = Regex::new(r"\{\{memory\.([^}]+)\}\}").unwrap();
}
```

**Step 3: Run tests**

```bash
cargo test --lib binding::template -- --nocapture
```

**Step 4: Commit**

```bash
git add src/binding/template.rs
git commit -m "feat(binding): add {{context.*}} template pattern

- {{context.files.X}} (new, preferred)
- {{memory.files.X}} (deprecated, still works)

Part of v0.14.2 context: migration"
```

---

### Task 1.7: Update Error Types

**Files:**
- Modify: `src/error.rs`

**Step 1: Update error variant name**

```rust
// BEFORE
#[error("[NIKA-250] Failed to load memory file '{alias}'")]
MemoryLoadError { alias: String, path: String, reason: String }

// AFTER
#[error("[NIKA-250] Failed to load context file '{alias}'")]
ContextLoadError { alias: String, path: String, reason: String }
```

Keep error code NIKA-250 for backward compatibility of logs.

**Step 2: Add type alias**

```rust
#[deprecated(since = "0.14.2", note = "Use ContextLoadError")]
pub type MemoryLoadError = ContextLoadError;
```

**Step 3: Run tests**

```bash
cargo test --lib error -- --nocapture
```

**Step 4: Commit**

```bash
git add src/error.rs
git commit -m "refactor(error): rename MemoryLoadError to ContextLoadError

- Error code NIKA-250 unchanged for log compatibility
- Add deprecated type alias

Part of v0.14.2 context: migration"
```

---

### Task 1.8: Update Boot Sequence

**Files:**
- Modify: `src/runtime/boot.rs`

**Step 1: Update references**

Change `memory.yaml` → `context.yaml` where applicable.
Update function calls to use new context_ names.

**Step 2: Run tests**

```bash
cargo test --lib runtime::boot -- --nocapture
```

**Step 3: Commit**

```bash
git add src/runtime/boot.rs
git commit -m "refactor(runtime): update boot to use context naming

Part of v0.14.2 context: migration"
```

---

### Task 1.9: Update CLI init Command

**Files:**
- Modify: `src/main.rs` (or wherever init lives)

**Step 1: Update .nika/ structure**

Change:
```rust
let memory_config_path = nika_dir.join("memory.yaml");
```
To:
```rust
let context_config_path = nika_dir.join("context.yaml");
```

**Step 2: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): nika init creates context.yaml instead of memory.yaml

Part of v0.14.2 context: migration"
```

---

## Phase 2: include: DAG Fusion (~4h)

### Task 2.1: Create IncludeSpec AST

**Files:**
- Create: `src/ast/include.rs`
- Modify: `src/ast/mod.rs`

**Step 1: Write tests first**

Create `src/ast/include.rs`:

```rust
//! Include specification for DAG fusion (v0.9)
//!
//! The `include:` block merges tasks from external workflows into the main DAG.
//!
//! # Example
//!
//! ```yaml
//! include:
//!   - path: ./lib/seo-tasks.nika.yaml
//!     prefix: seo_
//!   - path: ./lib/common.nika.yaml
//! ```

use serde::Deserialize;

/// Include specification for DAG fusion
#[derive(Debug, Clone, Deserialize)]
pub struct IncludeSpec {
    /// Path to the workflow file to include
    pub path: String,

    /// Optional prefix for all task IDs from this include
    /// Prevents ID collisions when including multiple workflows
    #[serde(default)]
    pub prefix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_include_spec_parse_minimal() {
        let yaml = r#"
path: ./lib/tasks.nika.yaml
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.path, "./lib/tasks.nika.yaml");
        assert!(spec.prefix.is_none());
    }

    #[test]
    fn test_include_spec_parse_with_prefix() {
        let yaml = r#"
path: ./lib/seo-tasks.nika.yaml
prefix: seo_
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.path, "./lib/seo-tasks.nika.yaml");
        assert_eq!(spec.prefix, Some("seo_".to_string()));
    }

    #[test]
    fn test_include_spec_parse_array() {
        let yaml = r#"
- path: ./lib/seo.nika.yaml
  prefix: seo_
- path: ./lib/common.nika.yaml
"#;
        let specs: Vec<IncludeSpec> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].prefix, Some("seo_".to_string()));
        assert!(specs[1].prefix.is_none());
    }
}
```

**Step 2: Run tests to verify**

```bash
cargo test --lib ast::include -- --nocapture
```

**Step 3: Update mod.rs**

In `src/ast/mod.rs`:
```rust
pub mod include;
pub use include::IncludeSpec;
```

**Step 4: Commit**

```bash
git add src/ast/include.rs src/ast/mod.rs
git commit -m "feat(ast): add IncludeSpec for DAG fusion

- IncludeSpec { path, prefix }
- Supports array of includes
- prefix prevents ID collisions

Part of v0.14.2 include: feature"
```

---

### Task 2.2: Add include: to Workflow

**Files:**
- Modify: `src/ast/workflow.rs`

**Step 1: Add test**

```rust
#[test]
fn test_workflow_parse_with_include() {
    let yaml = r#"
schema: nika/workflow@0.9
workflow: test-include
include:
  - path: ./lib/seo-tasks.nika.yaml
    prefix: seo_
tasks:
  - id: main
    infer: "Main task"
"#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.include.is_some());
    let includes = workflow.include.unwrap();
    assert_eq!(includes.len(), 1);
    assert_eq!(includes[0].prefix, Some("seo_".to_string()));
}
```

**Step 2: Add field to Workflow**

```rust
use super::include::IncludeSpec;

pub struct Workflow {
    // ... existing fields

    /// External workflows to include (DAG fusion)
    #[serde(default)]
    pub include: Option<Vec<IncludeSpec>>,
}
```

**Step 3: Run tests**

```bash
cargo test --lib ast::workflow::tests::test_workflow_parse_with_include -- --nocapture
```

**Step 4: Commit**

```bash
git add src/ast/workflow.rs
git commit -m "feat(ast): add include: field to Workflow

Part of v0.14.2 include: feature"
```

---

### Task 2.3: Implement Include Resolution

**Files:**
- Create: `src/runtime/include_resolver.rs`
- Modify: `src/runtime/mod.rs`

**Step 1: Write tests**

```rust
//! Include resolver for DAG fusion
//!
//! Resolves `include:` specifications by loading external workflows
//! and merging their tasks into the main DAG.

use crate::ast::{IncludeSpec, Task, Workflow};
use crate::error::NikaError;
use camino::Utf8Path;
use rustc_hash::FxHashSet;
use std::fs;

/// Resolve includes and return merged task list
pub fn resolve_includes(
    workflow: &Workflow,
    base_path: &Utf8Path,
    seen: &mut FxHashSet<String>,
) -> Result<Vec<Task>, NikaError> {
    let mut all_tasks = Vec::new();

    // Add tasks from includes first
    if let Some(includes) = &workflow.include {
        for include in includes {
            let tasks = resolve_single_include(include, base_path, seen)?;
            all_tasks.extend(tasks);
        }
    }

    // Add main workflow tasks
    all_tasks.extend(workflow.tasks.clone());

    Ok(all_tasks)
}

fn resolve_single_include(
    include: &IncludeSpec,
    base_path: &Utf8Path,
    seen: &mut FxHashSet<String>,
) -> Result<Vec<Task>, NikaError> {
    let include_path = base_path.join(&include.path);
    let canonical = include_path.canonicalize_utf8()
        .map_err(|e| NikaError::IncludeError {
            path: include.path.clone(),
            reason: format!("Failed to resolve path: {}", e),
        })?;

    // Circular include detection
    let path_str = canonical.to_string();
    if seen.contains(&path_str) {
        return Err(NikaError::IncludeError {
            path: include.path.clone(),
            reason: "Circular include detected".to_string(),
        });
    }
    seen.insert(path_str.clone());

    // Load included workflow
    let content = fs::read_to_string(&canonical)
        .map_err(|e| NikaError::IncludeError {
            path: include.path.clone(),
            reason: format!("Failed to read file: {}", e),
        })?;

    let included: Workflow = crate::serde_yaml::from_str(&content)
        .map_err(|e| NikaError::IncludeError {
            path: include.path.clone(),
            reason: format!("Failed to parse: {}", e),
        })?;

    // Recursively resolve nested includes
    let parent_path = canonical.parent().unwrap_or(Utf8Path::new("."));
    let mut tasks = resolve_includes(&included, parent_path, seen)?;

    // Apply prefix to task IDs
    if let Some(prefix) = &include.prefix {
        for task in &mut tasks {
            task.id = format!("{}{}", prefix, task.id);
            // Also update depends_on references
            if let Some(deps) = &mut task.depends_on {
                for dep in deps {
                    *dep = format!("{}{}", prefix, dep);
                }
            }
        }
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_resolve_includes_simple() {
        let temp = TempDir::new().unwrap();
        let base = Utf8Path::from_path(temp.path()).unwrap();

        // Create included workflow
        let included_yaml = r#"
schema: nika/workflow@0.9
workflow: included
tasks:
  - id: task1
    infer: "Included task"
"#;
        fs::write(base.join("included.nika.yaml"), included_yaml).unwrap();

        // Create main workflow
        let main_yaml = r#"
schema: nika/workflow@0.9
workflow: main
include:
  - path: ./included.nika.yaml
tasks:
  - id: main_task
    infer: "Main task"
"#;
        let workflow: Workflow = crate::serde_yaml::from_str(main_yaml).unwrap();

        let mut seen = FxHashSet::default();
        let tasks = resolve_includes(&workflow, base, &mut seen).unwrap();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "task1");
        assert_eq!(tasks[1].id, "main_task");
    }

    #[test]
    fn test_resolve_includes_with_prefix() {
        let temp = TempDir::new().unwrap();
        let base = Utf8Path::from_path(temp.path()).unwrap();

        let included_yaml = r#"
schema: nika/workflow@0.9
workflow: seo
tasks:
  - id: analyze
    infer: "SEO analyze"
  - id: optimize
    depends_on: [analyze]
    infer: "SEO optimize"
"#;
        fs::write(base.join("seo.nika.yaml"), included_yaml).unwrap();

        let main_yaml = r#"
schema: nika/workflow@0.9
workflow: main
include:
  - path: ./seo.nika.yaml
    prefix: seo_
tasks:
  - id: main_task
    depends_on: [seo_optimize]
    infer: "Main task"
"#;
        let workflow: Workflow = crate::serde_yaml::from_str(main_yaml).unwrap();

        let mut seen = FxHashSet::default();
        let tasks = resolve_includes(&workflow, base, &mut seen).unwrap();

        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, "seo_analyze");
        assert_eq!(tasks[1].id, "seo_optimize");
        assert_eq!(tasks[1].depends_on, Some(vec!["seo_analyze".to_string()]));
    }

    #[test]
    fn test_resolve_includes_circular_detection() {
        let temp = TempDir::new().unwrap();
        let base = Utf8Path::from_path(temp.path()).unwrap();

        // a.yaml includes b.yaml
        let a_yaml = r#"
schema: nika/workflow@0.9
workflow: a
include:
  - path: ./b.nika.yaml
tasks:
  - id: a_task
    infer: "A"
"#;
        // b.yaml includes a.yaml (circular!)
        let b_yaml = r#"
schema: nika/workflow@0.9
workflow: b
include:
  - path: ./a.nika.yaml
tasks:
  - id: b_task
    infer: "B"
"#;
        fs::write(base.join("a.nika.yaml"), a_yaml).unwrap();
        fs::write(base.join("b.nika.yaml"), b_yaml).unwrap();

        let workflow: Workflow = crate::serde_yaml::from_str(a_yaml).unwrap();

        let mut seen = FxHashSet::default();
        let result = resolve_includes(&workflow, base, &mut seen);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Circular include"));
    }
}
```

**Step 2: Add IncludeError to error.rs**

```rust
#[error("[NIKA-260] Failed to include workflow '{path}'")]
IncludeError { path: String, reason: String },
```

**Step 3: Update mod.rs**

```rust
pub mod include_resolver;
pub use include_resolver::resolve_includes;
```

**Step 4: Run tests**

```bash
cargo test --lib runtime::include_resolver -- --nocapture
```

**Step 5: Commit**

```bash
git add src/runtime/include_resolver.rs src/runtime/mod.rs src/error.rs
git commit -m "feat(runtime): implement include resolver for DAG fusion

- resolve_includes() merges tasks from external workflows
- Circular include detection
- Prefix support for ID collision prevention
- Recursive resolution for nested includes

Part of v0.14.2 include: feature"
```

---

### Task 2.4: Wire Include Resolution to Runner

**Files:**
- Modify: `src/runtime/runner.rs`

**Step 1: Update workflow loading**

In the runner, after parsing the workflow but before building the DAG:

```rust
use crate::runtime::include_resolver::resolve_includes;
use rustc_hash::FxHashSet;

// In run() or similar:
let workflow: Workflow = parse_workflow(&content)?;

// Resolve includes
let base_path = workflow_path.parent().unwrap_or(Utf8Path::new("."));
let mut seen = FxHashSet::default();
let all_tasks = resolve_includes(&workflow, base_path, &mut seen)?;

// Build DAG with merged tasks
let dag = build_dag(&all_tasks)?;
```

**Step 2: Run integration tests**

```bash
cargo test --lib runtime::runner -- --nocapture
```

**Step 3: Commit**

```bash
git add src/runtime/runner.rs
git commit -m "feat(runtime): wire include resolution to runner

Part of v0.14.2 include: feature"
```

---

## Phase 3: Schema @0.9 Update

### Task 3.1: Add Schema Version @0.9

**Files:**
- Modify: `src/ast/workflow.rs` (schema constants)
- Modify: `schemas/nika-workflow.schema.json`

**Step 1: Add schema constant**

```rust
pub const SCHEMA_V09: &str = "nika/workflow@0.9";

// Update SUPPORTED_SCHEMAS array
pub const SUPPORTED_SCHEMAS: &[&str] = &[
    SCHEMA_V01, SCHEMA_V02, SCHEMA_V03, SCHEMA_V04,
    SCHEMA_V05, SCHEMA_V06, SCHEMA_V07, SCHEMA_V08,
    SCHEMA_V09,
];
```

**Step 2: Update JSON Schema**

Add to `schemas/nika-workflow.schema.json`:

```json
{
  "properties": {
    "context": {
      "$ref": "#/$defs/ContextConfig"
    },
    "include": {
      "type": "array",
      "items": {
        "$ref": "#/$defs/IncludeSpec"
      }
    }
  },
  "$defs": {
    "ContextConfig": {
      "type": "object",
      "properties": {
        "files": {
          "type": "object",
          "additionalProperties": { "type": "string" }
        },
        "session": { "type": "string" }
      }
    },
    "IncludeSpec": {
      "type": "object",
      "required": ["path"],
      "properties": {
        "path": { "type": "string" },
        "prefix": { "type": "string" }
      }
    }
  }
}
```

**Step 3: Commit**

```bash
git add src/ast/workflow.rs schemas/nika-workflow.schema.json
git commit -m "feat(schema): add @0.9 with context: and include:

- nika/workflow@0.9 schema version
- context: replaces memory: (with backward compat)
- include: for DAG fusion
- JSON Schema updated

Part of v0.14.2 schema bump"
```

---

## Phase 4: Documentation & Examples

### Task 4.1: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`
- Modify: `tools/nika/CLAUDE.md`

Add schema @0.9 documentation with context: and include: examples.

**Commit:**
```bash
git commit -m "docs: update CLAUDE.md for v0.14.2 features"
```

---

### Task 4.2: Create Example Workflows

**Files:**
- Create: `examples/context-demo.nika.yaml`
- Create: `examples/include-demo.nika.yaml`
- Create: `examples/lib/seo-tasks.nika.yaml`

**Example context-demo.nika.yaml:**
```yaml
schema: nika/workflow@0.9
workflow: context-demo
description: Demonstrates context: field for file loading

context:
  files:
    brand: ./context/brand.md
    persona: ./context/persona.json

tasks:
  - id: generate
    infer: |
      Using brand context: {{context.files.brand}}
      Generate a tagline.
```

**Commit:**
```bash
git commit -m "docs(examples): add context: and include: demo workflows"
```

---

### Task 4.3: Update CHANGELOG

**Files:**
- Modify: `CHANGELOG.md`

Add v0.14.2 section with:
- `context:` field (memory: deprecated)
- `include:` DAG fusion
- Schema @0.9

**Commit:**
```bash
git commit -m "docs: update CHANGELOG for v0.14.2"
```

---

## Phase 5: Final Verification

### Task 5.1: Run Full Test Suite

```bash
cargo test --all-features
cargo clippy -- -D warnings
cargo fmt --check
```

### Task 5.2: Bump Version

**Files:**
- Modify: `Cargo.toml`

```toml
version = "0.14.2"
```

**Commit:**
```bash
git commit -m "chore: bump version to 0.14.2"
```

---

## Summary

| Phase | Tasks | Effort |
|-------|-------|--------|
| 1. context: migration | 9 tasks | ~6h |
| 2. include: DAG fusion | 4 tasks | ~4h |
| 3. Schema @0.9 | 1 task | ~30min |
| 4. Documentation | 3 tasks | ~1h |
| 5. Verification | 2 tasks | ~30min |
| **Total** | **19 tasks** | **~12h** |

---

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>
