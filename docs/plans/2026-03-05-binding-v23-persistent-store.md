# Persistent Datastore (v0.23.0) Implementation Plan

> **For Claude:** Follow this plan task-by-task using TDD methodology.

**Goal:** Enable workflow task results to persist between executions, allowing workflows to resume or reuse previous results.

**Architecture:** Create `PersistentStore` module with file-backed storage using atomic writes. Integrate with `DataStore` via `persist:` workflow field. Support import/export for portability.

**Tech Stack:** Rust (rustc 1.86+), serde, serde_json, tokio::fs, parking_lot

---

## Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  PERSISTENT DATASTORE — v0.23.0                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  CURRENT BEHAVIOR:                                                            ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • DataStore lives in memory only                                             ║
║  • All results lost when workflow completes                                   ║
║  • Must re-run all tasks on every execution                                   ║
║                                                                               ║
║  NEW BEHAVIOR (v0.23.0):                                                      ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  • Optional persist: field enables storage                                    ║
║  • Results saved to .nika/store/<workflow_id>.json                            ║
║  • Results loaded on workflow start                                           ║
║  • Tasks can check if result already exists                                   ║
║  • Export/import for sharing between machines                                 ║
║                                                                               ║
║  YAML SYNTAX:                                                                 ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  persist: true                    # Enable persistence                        ║
║  persist:                                                                     ║
║    enabled: true                                                              ║
║    path: ./custom/store.json      # Custom path                               ║
║    ttl: 86400                     # Time-to-live in seconds (24h)             ║
║    scope: workflow                # workflow | task                           ║
║                                                                               ║
║  TASK-LEVEL:                                                                  ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  tasks:                                                                       ║
║    - id: expensive_task                                                       ║
║      cache: true                  # Use cached result if available            ║
║      infer: "..."                                                             ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  PERSISTENT STORE ARCHITECTURE                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  File Structure:                                                                │
│  .nika/                                                                         │
│  └── store/                                                                     │
│      ├── my-workflow.json          ← Workflow results                          │
│      ├── my-workflow.lock          ← Lock file for atomic ops                  │
│      └── another-workflow.json                                                  │
│                                                                                 │
│  Store File Format (JSON):                                                      │
│  {                                                                              │
│    "version": 1,                                                                │
│    "workflow_id": "my-workflow",                                                │
│    "created_at": "2026-03-05T10:00:00Z",                                       │
│    "updated_at": "2026-03-05T12:30:00Z",                                       │
│    "results": {                                                                 │
│      "task1": {                                                                 │
│        "output": {...},                                                         │
│        "namespaces": {...},                                                     │
│        "duration_ms": 1234,                                                     │
│        "stored_at": "2026-03-05T10:05:00Z"                                     │
│      },                                                                         │
│      "task2": {...}                                                             │
│    }                                                                            │
│  }                                                                              │
│                                                                                 │
│  Module Structure:                                                              │
│  store/                                                                         │
│  ├── datastore.rs       ← Existing in-memory store                              │
│  ├── persistent.rs      ← NEW: PersistentStore                                  │
│  └── mod.rs             ← Export both                                           │
│                                                                                 │
│  Integration:                                                                   │
│  Runner                                                                         │
│   ├── Check persist: config                                                     │
│   ├── Load PersistentStore if enabled                                           │
│   ├── Hydrate DataStore from persistent                                         │
│   ├── Execute workflow                                                          │
│   └── Save updated results to persistent                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Task 1: Create PersistentStore struct

**Files:**
- Create: `tools/nika/src/store/persistent.rs`
- Modify: `tools/nika/src/store/mod.rs`
- Test: Inline tests

**Step 1: Write the failing test**

```rust
#[test]
fn test_persistent_store_new() {
    let store = PersistentStore::new("test-workflow");
    assert_eq!(store.workflow_id(), "test-workflow");
    assert!(store.results().is_empty());
}

#[test]
fn test_persistent_store_set_get() {
    let mut store = PersistentStore::new("test-workflow");

    let result = TaskResult::new(json!({"value": 42}));
    store.set("task1", result);

    let retrieved = store.get("task1").unwrap();
    assert_eq!(retrieved.output["value"], 42);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika persistent_store_new --lib`
Expected: FAIL with "cannot find type `PersistentStore`"

**Step 3: Create persistent.rs**

```rust
//! Persistent storage for workflow task results.
//!
//! Provides file-backed persistence with atomic writes and
//! optional TTL for cache expiration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::store::datastore::TaskResult;
use crate::error::NikaError;

/// Version of the persistent store format.
const STORE_VERSION: u32 = 1;

/// Serializable task result for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedResult {
    pub output: Value,
    pub namespaces: FxHashMap<String, Value>,
    pub duration_ms: u64,
    pub stored_at: DateTime<Utc>,
}

impl From<&TaskResult> for PersistedResult {
    fn from(result: &TaskResult) -> Self {
        Self {
            output: (*result.output).clone(),
            namespaces: result.namespaces.iter()
                .map(|(k, v)| (k.clone(), (**v).clone()))
                .collect(),
            duration_ms: result.duration.as_millis() as u64,
            stored_at: Utc::now(),
        }
    }
}

impl From<PersistedResult> for TaskResult {
    fn from(persisted: PersistedResult) -> Self {
        let mut result = TaskResult::new(persisted.output);
        result.duration = std::time::Duration::from_millis(persisted.duration_ms);
        for (name, value) in persisted.namespaces {
            result.set_namespace(name, value);
        }
        result
    }
}

/// Serializable store file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreFile {
    pub version: u32,
    pub workflow_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub results: FxHashMap<String, PersistedResult>,
}

impl StoreFile {
    fn new(workflow_id: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            version: STORE_VERSION,
            workflow_id: workflow_id.into(),
            created_at: now,
            updated_at: now,
            results: FxHashMap::default(),
        }
    }
}

/// Persistent store for workflow results.
///
/// Provides file-backed storage with atomic writes.
pub struct PersistentStore {
    workflow_id: String,
    path: PathBuf,
    data: RwLock<StoreFile>,
    ttl_secs: Option<u64>,
}

impl PersistentStore {
    /// Create a new persistent store for a workflow.
    pub fn new(workflow_id: impl Into<String>) -> Self {
        let id = workflow_id.into();
        let path = Self::default_path(&id);
        Self {
            workflow_id: id.clone(),
            path,
            data: RwLock::new(StoreFile::new(id)),
            ttl_secs: None,
        }
    }

    /// Create with a custom path.
    pub fn with_path(workflow_id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let id = workflow_id.into();
        Self {
            workflow_id: id.clone(),
            path: path.into(),
            data: RwLock::new(StoreFile::new(id)),
            ttl_secs: None,
        }
    }

    /// Set TTL in seconds for cached results.
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = Some(ttl_secs);
        self
    }

    /// Get the workflow ID.
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Get all stored results.
    pub fn results(&self) -> FxHashMap<String, TaskResult> {
        let data = self.data.read();
        data.results.iter()
            .filter(|(_, r)| self.is_valid(r))
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect()
    }

    /// Check if a result is still valid (not expired).
    fn is_valid(&self, result: &PersistedResult) -> bool {
        match self.ttl_secs {
            Some(ttl) => {
                let age = Utc::now().signed_duration_since(result.stored_at);
                age.num_seconds() < ttl as i64
            }
            None => true,
        }
    }

    /// Get a result by task ID.
    pub fn get(&self, task_id: &str) -> Option<TaskResult> {
        let data = self.data.read();
        data.results.get(task_id)
            .filter(|r| self.is_valid(r))
            .map(|r| r.clone().into())
    }

    /// Check if a task result exists and is valid.
    pub fn has(&self, task_id: &str) -> bool {
        let data = self.data.read();
        data.results.get(task_id)
            .map(|r| self.is_valid(r))
            .unwrap_or(false)
    }

    /// Set a task result.
    pub fn set(&self, task_id: impl Into<String>, result: TaskResult) {
        let mut data = self.data.write();
        data.results.insert(task_id.into(), (&result).into());
        data.updated_at = Utc::now();
    }

    /// Remove a task result.
    pub fn remove(&self, task_id: &str) -> Option<TaskResult> {
        let mut data = self.data.write();
        data.results.remove(task_id).map(|r| r.into())
    }

    /// Clear all results.
    pub fn clear(&self) {
        let mut data = self.data.write();
        data.results.clear();
        data.updated_at = Utc::now();
    }

    /// Get the default path for a workflow store.
    pub fn default_path(workflow_id: &str) -> PathBuf {
        PathBuf::from(".nika/store").join(format!("{}.json", workflow_id))
    }

    /// Get the store file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_persistent_store_new() {
        let store = PersistentStore::new("test-workflow");
        assert_eq!(store.workflow_id(), "test-workflow");
        assert!(store.results().is_empty());
    }

    #[test]
    fn test_persistent_store_set_get() {
        let store = PersistentStore::new("test-workflow");
        let result = TaskResult::new(json!({"value": 42}));
        store.set("task1", result);

        let retrieved = store.get("task1").unwrap();
        assert_eq!(retrieved.output["value"], 42);
    }

    #[test]
    fn test_persistent_store_with_namespaces() {
        let store = PersistentStore::new("test-workflow");
        let mut result = TaskResult::new(json!({"answer": "yes"}));
        result.set_namespace("artifacts", json!(["file1.txt", "file2.txt"]));
        store.set("task1", result);

        let retrieved = store.get("task1").unwrap();
        assert_eq!(retrieved.namespaces.len(), 1);
        assert!(retrieved.namespaces.contains_key("artifacts"));
    }
}
```

**Step 4: Update mod.rs**

```rust
mod datastore;
mod persistent;

pub use datastore::{DataStore, TaskResult, TaskStatus};
pub use persistent::{PersistentStore, PersistedResult, StoreFile};
```

**Step 5: Run tests**

Run: `cargo test -p nika persistent_store --lib`
Expected: PASS

**Step 6: Commit**

```bash
git add tools/nika/src/store/persistent.rs tools/nika/src/store/mod.rs
git commit -m "$(cat <<'EOF'
feat(store): create PersistentStore module

Add file-backed persistence for workflow results:
- PersistentStore struct with in-memory cache
- PersistedResult for serialization
- StoreFile format with version tracking
- Optional TTL for cache expiration
- Namespace support

Foundation for v0.23.0 persistent datastore.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 2: Add atomic file operations

**Files:**
- Modify: `tools/nika/src/store/persistent.rs`
- Test: Inline tests

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_persistent_store_save_load() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("test.json");

    let store = PersistentStore::with_path("test-workflow", &path);
    store.set("task1", TaskResult::new(json!({"value": 42})));

    // Save to disk
    store.save().await.unwrap();

    // Create new store and load
    let store2 = PersistentStore::with_path("test-workflow", &path);
    store2.load().await.unwrap();

    let result = store2.get("task1").unwrap();
    assert_eq!(result.output["value"], 42);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika persistent_store_save_load --lib`
Expected: FAIL with "method `save` not found"

**Step 3: Add save/load methods with file locking**

> **Note:** Add `fs2 = "0.4"` to Cargo.toml for cross-platform file locking.

```rust
impl PersistentStore {
    /// Save the store to disk atomically with file locking.
    ///
    /// Uses:
    /// - File locking (fs2) to prevent race conditions between processes
    /// - Temp file + rename pattern for crash safety
    ///
    /// # Race Condition Prevention
    /// If two Nika processes try to save the same store simultaneously:
    /// 1. First process acquires exclusive lock on .lock file
    /// 2. Second process blocks waiting for lock
    /// 3. First process completes save and releases lock
    /// 4. Second process acquires lock and proceeds
    pub async fn save(&self) -> Result<(), NikaError> {
        use fs2::FileExt;
        use std::fs::OpenOptions;
        use tokio::fs;
        use tokio::io::AsyncWriteExt;

        // Ensure directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                NikaError::PersistentStoreError(format!(
                    "Failed to create directory {:?}: {}",
                    parent, e
                ))
            })?;
        }

        // Acquire exclusive file lock (blocking)
        let lock_path = self.path.with_extension("json.lock");
        let lock_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&lock_path)
            .map_err(|e| {
                NikaError::PersistentStoreError(format!(
                    "Failed to create lock file {:?}: {}",
                    lock_path, e
                ))
            })?;

        lock_file.lock_exclusive().map_err(|e| {
            NikaError::PersistentStoreError(format!(
                "Failed to acquire lock on {:?}: {}",
                lock_path, e
            ))
        })?;

        // Serialize data (while holding lock)
        let data = self.data.read();
        let json = serde_json::to_string_pretty(&*data).map_err(|e| {
            NikaError::PersistentStoreError(format!("Serialization failed: {}", e))
        })?;
        drop(data);

        // Write to temp file
        let temp_path = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&temp_path).await.map_err(|e| {
            NikaError::PersistentStoreError(format!(
                "Failed to create temp file {:?}: {}",
                temp_path, e
            ))
        })?;

        file.write_all(json.as_bytes()).await.map_err(|e| {
            NikaError::PersistentStoreError(format!("Write failed: {}", e))
        })?;

        file.sync_all().await.map_err(|e| {
            NikaError::PersistentStoreError(format!("Sync failed: {}", e))
        })?;

        drop(file);

        // Atomic rename
        fs::rename(&temp_path, &self.path).await.map_err(|e| {
            NikaError::PersistentStoreError(format!("Rename failed: {}", e))
        })?;

        // Release lock (implicit when lock_file is dropped)
        drop(lock_file);
        // Optionally clean up lock file
        let _ = std::fs::remove_file(&lock_path);

        Ok(())
    }

    /// Load the store from disk.
    pub async fn load(&self) -> Result<(), NikaError> {
        use tokio::fs;

        // Check if file exists
        if !self.path.exists() {
            return Ok(());
        }

        // Read file
        let json = fs::read_to_string(&self.path).await.map_err(|e| {
            NikaError::PersistentStoreError(format!(
                "Failed to read {:?}: {}",
                self.path, e
            ))
        })?;

        // Deserialize
        let file: StoreFile = serde_json::from_str(&json).map_err(|e| {
            NikaError::PersistentStoreError(format!("Deserialization failed: {}", e))
        })?;

        // Version check
        if file.version != STORE_VERSION {
            return Err(NikaError::PersistentStoreError(format!(
                "Store version mismatch: expected {}, got {}",
                STORE_VERSION, file.version
            )));
        }

        // Update data
        *self.data.write() = file;

        Ok(())
    }

    /// Check if the store file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}
```

**Step 4: Add error variant**

Add to `error.rs`:

```rust
/// NIKA-290: Persistent store error
#[error("NIKA-290: Persistent store error: {0}")]
PersistentStoreError(String),
```

**Step 5: Run tests**

Run: `cargo test -p nika persistent_store_save_load --lib`
Expected: PASS

**Step 6: Commit**

```bash
git add tools/nika/src/store/persistent.rs tools/nika/src/error.rs
git commit -m "$(cat <<'EOF'
feat(store): add atomic save/load to PersistentStore

Implement file operations with crash safety:
- save() uses temp file + fsync + atomic rename
- load() reads and validates version
- exists() checks for store file
- NIKA-290 error code for persistence errors

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 3: Add persist field to Workflow AST

**Files:**
- Modify: `tools/nika/src/ast/workflow.rs`
- Test: Inline tests

**Step 1: Write the failing test**

```rust
#[test]
fn test_workflow_persist_boolean() {
    let yaml = r#"
        schema: "nika/workflow@0.10"
        persist: true
        tasks: []
    "#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    assert!(workflow.persist.is_some());
    assert!(workflow.persist.unwrap().enabled);
}

#[test]
fn test_workflow_persist_config() {
    let yaml = r#"
        schema: "nika/workflow@0.10"
        persist:
          enabled: true
          path: ./custom/store.json
          ttl: 3600
        tasks: []
    "#;
    let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
    let persist = workflow.persist.unwrap();
    assert!(persist.enabled);
    assert_eq!(persist.path.as_deref(), Some("./custom/store.json"));
    assert_eq!(persist.ttl, Some(3600));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika workflow_persist --lib`
Expected: FAIL with "unknown field `persist`"

**Step 3: Add PersistConfig to workflow.rs**

```rust
/// Configuration for persistent datastore.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PersistConfig {
    /// Whether persistence is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Custom path for the store file.
    #[serde(default)]
    pub path: Option<String>,

    /// Time-to-live in seconds for cached results.
    #[serde(default)]
    pub ttl: Option<u64>,

    /// Scope of persistence: "workflow" or "task".
    #[serde(default)]
    pub scope: Option<String>,
}

/// Wrapper to allow both boolean and object syntax.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PersistSpec {
    /// Simple boolean: `persist: true`
    Boolean(bool),
    /// Full config: `persist: { enabled: true, ... }`
    Config(PersistConfig),
}

impl PersistSpec {
    pub fn to_config(&self) -> PersistConfig {
        match self {
            PersistSpec::Boolean(enabled) => PersistConfig {
                enabled: *enabled,
                ..Default::default()
            },
            PersistSpec::Config(config) => config.clone(),
        }
    }
}

// In Workflow struct:
pub struct Workflow {
    // ... existing fields

    /// Persistent datastore configuration.
    #[serde(default)]
    pub persist: Option<PersistSpec>,
}
```

**Step 4: Run tests**

Run: `cargo test -p nika workflow_persist --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/ast/workflow.rs
git commit -m "$(cat <<'EOF'
feat(ast): add persist field to Workflow

Support persistent datastore configuration:
- persist: true (shorthand)
- persist: { enabled, path, ttl, scope }
- PersistSpec for flexible deserialization
- PersistConfig for typed access

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 4: Add cache field to Task

**Files:**
- Modify: `tools/nika/src/ast/task.rs`
- Test: Inline tests

**Step 1: Write the failing test**

```rust
#[test]
fn test_task_cache_field() {
    let yaml = r#"
        id: expensive_task
        cache: true
        infer: "Expensive computation"
    "#;
    let task: Task = serde_yaml::from_str(yaml).unwrap();
    assert!(task.cache.unwrap_or(false));
}
```

**Step 2: Run test to verify it fails**

Expected: FAIL with "unknown field `cache`"

**Step 3: Add cache field to Task**

```rust
pub struct Task {
    // ... existing fields

    /// Whether to use cached result if available.
    /// Requires workflow-level persist: true.
    #[serde(default)]
    pub cache: Option<bool>,
}
```

**Step 4: Run tests**

Run: `cargo test -p nika task_cache_field --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/ast/task.rs
git commit -m "$(cat <<'EOF'
feat(ast): add cache field to Task

Task-level opt-in for cached results:
- cache: true → use cached if available
- cache: false → always re-execute
- Requires persist: true at workflow level

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 5: Integrate PersistentStore with Runner

**Files:**
- Modify: `tools/nika/src/runtime/runner.rs`
- Test: Integration tests

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_runner_with_persistence() {
    let yaml = r#"
        schema: "nika/workflow@0.10"
        persist: true
        tasks:
          - id: task1
            cache: true
            exec: "echo hello"
    "#;

    let workflow = parse_workflow(yaml).unwrap();
    let mut runner = Runner::new(workflow);

    // First run
    runner.run().await.unwrap();

    // Second run should use cache
    let mut runner2 = Runner::new(parse_workflow(yaml).unwrap());
    let result = runner2.run().await.unwrap();

    // Verify task was cached
    assert!(runner2.was_cached("task1"));
}
```

**Step 2: Run test to verify it fails**

Expected: FAIL (persistence not integrated)

**Step 3: Integrate PersistentStore in Runner**

```rust
impl Runner {
    /// Create runner with optional persistence.
    pub fn new(workflow: Workflow) -> Self {
        let persistent_store = workflow.persist.as_ref().map(|spec| {
            let config = spec.to_config();
            if config.enabled {
                let workflow_id = workflow.id.as_deref().unwrap_or("unnamed");
                let mut store = match &config.path {
                    Some(path) => PersistentStore::with_path(workflow_id, path),
                    None => PersistentStore::new(workflow_id),
                };
                if let Some(ttl) = config.ttl {
                    store = store.with_ttl(ttl);
                }
                Some(store)
            } else {
                None
            }
        }).flatten();

        Self {
            workflow,
            persistent_store,
            cached_tasks: FxHashSet::default(),
            // ...
        }
    }

    /// Run the workflow with persistence support.
    pub async fn run(&mut self) -> Result<RunResult, NikaError> {
        // Load persistent store if available
        if let Some(store) = &self.persistent_store {
            store.load().await?;

            // Hydrate DataStore with cached results
            for (task_id, result) in store.results() {
                self.datastore.store(&task_id, result);
            }
        }

        // Execute workflow...
        let result = self.execute_workflow().await?;

        // Save persistent store
        if let Some(store) = &self.persistent_store {
            // Update store with new/changed results
            for (task_id, result) in self.datastore.all_results() {
                store.set(&task_id, result);
            }
            store.save().await?;
        }

        Ok(result)
    }

    /// Check if task should use cached result.
    fn should_use_cache(&self, task: &Task) -> bool {
        let cache_enabled = task.cache.unwrap_or(false);
        if !cache_enabled {
            return false;
        }

        if let Some(store) = &self.persistent_store {
            return store.has(&task.id);
        }

        false
    }

    /// Check if a task was served from cache.
    pub fn was_cached(&self, task_id: &str) -> bool {
        self.cached_tasks.contains(task_id)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p nika runner_with_persistence --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika/src/runtime/runner.rs
git commit -m "$(cat <<'EOF'
feat(runner): integrate PersistentStore

Wire persistence into workflow execution:
- Load store on run start
- Hydrate DataStore from cache
- Check should_use_cache() per task
- Save store on run completion
- Track which tasks used cache

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 6: Update JSON Schema

**Files:**
- Modify: `tools/nika/schemas/nika-workflow.schema.json`

**Step 1: Add persist and cache definitions**

```json
{
  "definitions": {
    "PersistConfig": {
      "oneOf": [
        { "type": "boolean" },
        {
          "type": "object",
          "properties": {
            "enabled": { "type": "boolean", "default": false },
            "path": { "type": "string" },
            "ttl": { "type": "integer", "minimum": 0 },
            "scope": { "enum": ["workflow", "task"] }
          },
          "additionalProperties": false
        }
      ]
    }
  },
  "properties": {
    "persist": { "$ref": "#/definitions/PersistConfig" }
  }
}
```

Add to task definition:

```json
{
  "properties": {
    "cache": { "type": "boolean" }
  }
}
```

**Step 2: Commit**

```bash
git add tools/nika/schemas/nika-workflow.schema.json
git commit -m "$(cat <<'EOF'
feat(schema): add persist and cache to JSON Schema

Update workflow schema for v0.23.0:
- persist: boolean or PersistConfig object
- cache: task-level caching flag
- TTL, path, scope options

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 7: Create example workflow

**Files:**
- Create: `tools/nika/examples/v23-persistent-store.nika.yaml`

**Step 1: Create example**

```yaml
# v23-persistent-store.nika.yaml
# Demonstrates persistent datastore between runs
schema: "nika/workflow@0.10"
provider: claude

# Enable persistence for this workflow
persist:
  enabled: true
  ttl: 86400  # Cache for 24 hours

tasks:
  # Expensive task that benefits from caching
  - id: research
    cache: true  # Use cached result if available
    agent:
      prompt: |
        Research the latest developments in Rust async programming.
        Create a comprehensive summary with:
        1. Key innovations in 2024-2025
        2. New crates and libraries
        3. Best practices
      tools:
        - nika:emit_output
      max_turns: 10

  # This task always runs fresh
  - id: current_analysis
    cache: false  # Always re-execute
    use:
      research: $research
    infer: |
      Based on this research: {{use.research}}

      Provide today's specific recommendations for a new Rust project.
      Consider current ecosystem state.

  # Report uses cached research + fresh analysis
  - id: report
    use:
      research: $research
      analysis: current_analysis
    infer: |
      Generate a final report combining:
      - Cached research: {{use.research}}
      - Fresh analysis: {{use.analysis}}

flows:
  - source: research
    target: current_analysis
  - source: [research, current_analysis]
    target: report
```

**Step 2: Validate**

Run: `cargo run -p nika -- check examples/v23-persistent-store.nika.yaml`
Expected: Valid

**Step 3: Commit**

```bash
git add tools/nika/examples/v23-persistent-store.nika.yaml
git commit -m "$(cat <<'EOF'
docs(examples): add v23-persistent-store example

Demonstrate persistent datastore:
- persist: with TTL configuration
- cache: true for expensive tasks
- cache: false for fresh execution
- Mixed cached + fresh workflow

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 8: Add export/import functionality

**Files:**
- Modify: `tools/nika/src/store/persistent.rs`
- Test: Integration tests

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_export_import() {
    let store = PersistentStore::new("test");
    store.set("task1", TaskResult::new(json!({"value": 1})));
    store.set("task2", TaskResult::new(json!({"value": 2})));

    // Export to JSON string
    let exported = store.export_json().unwrap();

    // Import into new store
    let store2 = PersistentStore::new("test2");
    store2.import_json(&exported).unwrap();

    assert!(store2.has("task1"));
    assert!(store2.has("task2"));
}
```

**Step 2: Add export/import methods**

```rust
impl PersistentStore {
    /// Export store contents as JSON string.
    pub fn export_json(&self) -> Result<String, NikaError> {
        let data = self.data.read();
        serde_json::to_string_pretty(&*data).map_err(|e| {
            NikaError::PersistentStoreError(format!("Export failed: {}", e))
        })
    }

    /// Import store contents from JSON string.
    pub fn import_json(&self, json: &str) -> Result<(), NikaError> {
        let file: StoreFile = serde_json::from_str(json).map_err(|e| {
            NikaError::PersistentStoreError(format!("Import failed: {}", e))
        })?;

        let mut data = self.data.write();
        for (task_id, result) in file.results {
            data.results.insert(task_id, result);
        }
        data.updated_at = Utc::now();

        Ok(())
    }
}
```

**Step 3: Run tests and commit**

```bash
git add tools/nika/src/store/persistent.rs
git commit -m "$(cat <<'EOF'
feat(store): add export/import for PersistentStore

Enable sharing cached results between machines:
- export_json() → serialized store contents
- import_json() → merge into current store
- Useful for CI caching and team sharing

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 9: Update documentation

**Files:**
- Modify: `tools/nika/CLAUDE.md`

**Step 1: Add persistence documentation**

```markdown
### Persistent Datastore (v0.23.0)

Cache workflow results between executions:

```yaml
# Enable persistence for the workflow
persist:
  enabled: true
  ttl: 86400      # Optional: cache expiry in seconds (24h)
  path: ./cache   # Optional: custom store location

tasks:
  - id: expensive_research
    cache: true   # Use cached result if available
    agent:
      prompt: "Research..."

  - id: always_fresh
    cache: false  # Always re-execute
    infer: "..."
```

**Store location:** `.nika/store/<workflow_id>.json`

**Cache behavior:**
- `cache: true` — Skip execution if valid cached result exists
- `cache: false` — Always execute (default)
- TTL expiry removes stale results automatically

**CLI commands:**
```bash
# Clear cache for a workflow
nika cache clear my-workflow

# Export cache for sharing
nika cache export my-workflow > cache.json

# Import cache
nika cache import my-workflow < cache.json
```
```

**Step 2: Commit**

```bash
git add tools/nika/CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: add persistent datastore documentation

Document v0.23.0 persistence features:
- persist: workflow configuration
- cache: task-level opt-in
- TTL and custom paths
- CLI cache commands
- Store file format

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
EOF
)"
```

---

## Task 10: Version bump and CHANGELOG

**Files:**
- Modify: `tools/nika/Cargo.toml`
- Modify: `CHANGELOG.md`

**Step 1: Bump version to 0.23.0**

**Step 2: Add CHANGELOG entry**

```markdown
## [0.23.0] - 2026-03-XX

### Added

- **Persistent Datastore** — Cache workflow results between executions
  - `persist:` workflow field for enabling persistence
  - `cache:` task field for per-task opt-in
  - `PersistentStore` module with atomic file operations
  - TTL support for automatic cache expiration
  - Export/import for sharing between machines
  - `.nika/store/<workflow_id>.json` storage format
- **Error code NIKA-290** — Persistent store errors
- **Example workflow** — `examples/v23-persistent-store.nika.yaml`
- **25+ persistence tests** — Comprehensive coverage

### Changed

- Runner integrates PersistentStore for workflow execution
- JSON Schema updated with persist and cache definitions

### Statistics

- **X tests passing** (updated count)
- **Zero clippy warnings**
```

**Step 3: Commit**

```bash
git add tools/nika/Cargo.toml CHANGELOG.md
git commit -m "$(cat <<'EOF'
chore(release): bump version to 0.23.0

Add persistent datastore feature.
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

# Run specific persistence tests
cargo test -p nika persistent

# Lint check
cargo clippy -p nika -- -D warnings

# Validate example workflow
cargo run -p nika -- check examples/v23-persistent-store.nika.yaml

# Test persistence manually
cargo run -p nika -- examples/v23-persistent-store.nika.yaml
# Run again - should use cache
cargo run -p nika -- examples/v23-persistent-store.nika.yaml
```

---

## Exit Criteria

- [ ] `PersistentStore` module with save/load
- [ ] Atomic file writes (temp + fsync + rename)
- [ ] `persist:` workflow field parsed
- [ ] `cache:` task field parsed
- [ ] Runner integrates persistence
- [ ] Results loaded on workflow start
- [ ] Results saved on workflow completion
- [ ] TTL expiration works
- [ ] Export/import functions work
- [ ] 25+ new tests passing
- [ ] Example workflow demonstrates persistence
- [ ] CLAUDE.md documents feature
- [ ] CHANGELOG updated
- [ ] Version bumped to 0.23.0
- [ ] Zero clippy warnings

---

## Skills Usage

| Step | Skill | Purpose |
|------|-------|---------|
| All | `superpowers:test-driven-development` | Write tests first |
| All | `superpowers:verification-before-completion` | Verify before commit |
| Debug | `superpowers:systematic-debugging` | If tests fail |
| Review | `superpowers:requesting-code-review` | After completion |
