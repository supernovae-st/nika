# Code Quality Improvement Plans - Nika v0.1

> Generated: 2026-01-02 | Status: Ready for execution
> Source: Deep codebase analysis (4032 lines, 20 files)

---

## Plan P1: Critical Consistency Fixes (5 items)

**Goal:** Unify hash collection types across codebase for consistency and performance.

**Research Summary:**
- FxHashMap/FxHashSet: 20-50% faster for non-crypto hashing (rustc-hash crate)
- Best for: trusted inputs, internal caches, small string keys
- Already used in: `flow_graph.rs`, `use_bindings.rs`, `interner.rs`, `datastore.rs`

### P1.1: Convert `use_wiring.rs` HashMap → FxHashMap

**File:** `src/use_wiring.rs:10-13`
**Current:**
```rust
use std::collections::HashMap;
pub type UseWiring = HashMap<String, UseEntry>;
```
**Target:**
```rust
use rustc_hash::FxHashMap;
pub type UseWiring = FxHashMap<String, UseEntry>;
```
**Impact:** Consistent with `use_bindings.rs` which consumes UseWiring

---

### P1.2: Convert `task_action.rs` HashMap → FxHashMap

**File:** `src/task_action.rs:6,33`
**Current:**
```rust
use std::collections::HashMap;
// ...
pub headers: HashMap<String, String>,
```
**Target:**
```rust
use rustc_hash::FxHashMap;
// ...
pub headers: FxHashMap<String, String>,
```
**Impact:** HTTP headers are small maps, FxHashMap is faster

---

### P1.3: Convert `validator.rs` HashSet → FxHashSet

**File:** `src/validator.rs:8,17`
**Current:**
```rust
use std::collections::HashSet;
let all_task_ids: HashSet<String> = ...
```
**Target:**
```rust
use rustc_hash::FxHashSet;
let all_task_ids: FxHashSet<String> = ...
```
**Impact:** Task ID lookups are frequent, FxHashSet is faster

---

### P1.4: Convert `template.rs` HashSet → FxHashSet

**File:** `src/template.rs:211`
**Current:**
```rust
pub fn validate_refs(
    template: &str,
    declared_aliases: &std::collections::HashSet<String>,
    task_id: &str,
) -> Result<(), NikaError>
```
**Target:**
```rust
use rustc_hash::FxHashSet;
pub fn validate_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
) -> Result<(), NikaError>
```
**Impact:** Consistent with validator.rs which will call this

---

### P1.5: Add `NikaError::InvalidSchema` variant

**File:** `src/error.rs` (add new variant)
**Current:** Uses `NikaError::Template(format!(...))` for schema errors
**Target:**
```rust
// In error.rs, after NIKA-082
#[error("NIKA-010: Invalid schema version: expected '{expected}', got '{actual}'")]
InvalidSchema { expected: String, actual: String },
```

**File:** `src/workflow.rs:58-64`
**Current:**
```rust
return Err(NikaError::Template(format!(
    "Invalid schema: expected '{}', got '{}'",
    SCHEMA_V01, self.schema
)));
```
**Target:**
```rust
return Err(NikaError::InvalidSchema {
    expected: SCHEMA_V01.to_string(),
    actual: self.schema.clone(),
});
```

**File:** `src/error.rs` (add fix suggestion)
```rust
NikaError::InvalidSchema { .. } => {
    Some("Use 'nika/workflow@0.1' as the schema version")
}
```

---

## Plan P2: Performance Optimizations (4 items)

**Goal:** Reduce allocations and improve async consistency.

### P2.1: Optimize `interner.rs` - avoid unnecessary Arc creation

**File:** `src/interner.rs:37-49`
**Current:** Creates `Arc<str>` before checking if exists
**Target:**
```rust
pub fn intern(&self, s: &str) -> Arc<str> {
    // Fast path: check without allocation
    if let Some(existing) = self.strings.get(s) {
        return Arc::clone(existing.key());
    }

    // Slow path: create and insert
    let key: Arc<str> = Arc::from(s);
    self.strings.insert(Arc::clone(&key), ());
    key
}
```
**Note:** Requires changing DashMap key type or using raw_entry API

---

### P2.2: Optimize `datastore.rs` - use value() instead of map

**File:** `src/datastore.rs:103-105`
**Current:**
```rust
pub fn get(&self, task_id: &str) -> Option<TaskResult> {
    self.results.get(task_id).map(|r| r.clone())
}
```
**Target:**
```rust
pub fn get(&self, task_id: &str) -> Option<TaskResult> {
    self.results.get(task_id).map(|r| r.value().clone())
}
```
**Impact:** Clearer intent, same performance

---

### P2.3: Use async file read in `runner.rs` schema validation

**File:** `src/runner.rs:328-334`
**Current:**
```rust
fn validate_schema(value: &Value, schema_path: &str) -> Result<(), NikaError> {
    let schema_str = std::fs::read_to_string(schema_path).map_err(|e| {...})?;
```
**Target:**
```rust
async fn validate_schema(value: &Value, schema_path: &str) -> Result<(), NikaError> {
    let schema_str = tokio::fs::read_to_string(schema_path).await.map_err(|e| {...})?;
```
**Impact:** Consistent async, doesn't block tokio runtime

---

### P2.4: Use `&str` instead of `String` in template validate_refs

**File:** `src/template.rs:209-213`
**Current:**
```rust
pub fn validate_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,  // After P1.4
    task_id: &str,
) -> Result<(), NikaError>
```
**Target:**
```rust
pub fn validate_refs<S: AsRef<str> + std::hash::Hash + Eq>(
    template: &str,
    declared_aliases: &FxHashSet<S>,
    task_id: &str,
) -> Result<(), NikaError>
```
**Alternative:** Keep `FxHashSet<String>` for simplicity - evaluate after P1

---

## Plan P3: Documentation & Cleanup (4 items)

**Goal:** Improve code clarity and remove stale TODOs.

### P3.1: Add module documentation to `lib.rs`

**File:** `src/lib.rs`
**Target:**
```rust
//! Nika - DAG workflow runner for AI tasks
//!
//! ## Architecture
//!
//! - `workflow`: YAML parsing and task definitions
//! - `runner`: DAG execution with tokio
//! - `datastore`: Thread-safe task output storage (DashMap)
//! - `flow_graph`: Dependency graph with FxHashMap optimization
//! - `template`: Single-pass {{use.alias}} resolution
//! - `use_bindings`: Resolved values from use: blocks
//! - `validator`: Static workflow validation
//! - `provider`: LLM provider abstraction (Claude, OpenAI)
//! - `event_log`: Event sourcing for audit trail
//! - `interner`: String interning for task IDs
```

---

### P3.2: Address TODO in `task_executor.rs:141`

**File:** `src/task_executor.rs:141`
**Current:**
```rust
tokens_used: None, // TODO: if provider returns token count
```
**Options:**
1. Implement token counting (add to Provider trait)
2. Remove TODO if not planned for v0.1
3. Convert to FIXME with issue link

**Recommendation:** Keep TODO but add context:
```rust
tokens_used: None, // TODO(v0.2): Add token counting to Provider trait
```

---

### P3.3: Unify JSONPath validation logic

**Files:** `src/validator.rs:102-149` and `src/jsonpath.rs:31-84`
**Issue:** Both files validate JSONPath syntax separately
**Target:**
- Move validation to `jsonpath::validate(path) -> Result<(), NikaError>`
- Have `validator.rs` call `jsonpath::validate()` instead of `validate_jsonpath()`
- Delete `validate_jsonpath`, `is_valid_identifier`, `is_valid_array_segment` from validator.rs

---

### P3.4: Add Provider trait documentation

**File:** `src/provider/mod.rs:16-23`
**Target:**
```rust
/// LLM provider abstraction for inference operations
///
/// Implementations:
/// - `ClaudeProvider`: Anthropic Claude API
/// - `OpenAIProvider`: OpenAI API
/// - `MockProvider`: Testing mock
///
/// # Example
/// ```rust,ignore
/// let provider = create_provider("claude")?;
/// let response = provider.infer("Hello", "claude-sonnet-4-5").await?;
/// ```
#[async_trait]
pub trait Provider: Send + Sync {
```

---

## Plan P4: Future Improvements (4 items)

**Goal:** Performance and API improvements for future versions.

### P4.1: EventLog::events() return reference

**File:** `src/event_log.rs:155-159`
**Current:** Returns `Vec<Event>` (clones entire vector)
**Future:**
```rust
/// Get read access to events (no clone)
pub fn events(&self) -> parking_lot::RwLockReadGuard<'_, Vec<Event>> {
    self.events.read()
}

/// Get owned copy of events (when needed)
pub fn events_cloned(&self) -> Vec<Event> {
    self.events.read().clone()
}
```
**Impact:** Avoids clone when just iterating

---

### P4.2: Cache parsed JSONPath segments

**Concept:** Memoize `jsonpath::parse()` for repeated paths
**Implementation:**
```rust
use dashmap::DashMap;
use once_cell::sync::Lazy;

static PATH_CACHE: Lazy<DashMap<String, Vec<Segment>>> = Lazy::new(DashMap::new);

pub fn parse_cached(path: &str) -> Result<Vec<Segment>, NikaError> {
    if let Some(cached) = PATH_CACHE.get(path) {
        return Ok(cached.clone());
    }
    let segments = parse(path)?;
    PATH_CACHE.insert(path.to_string(), segments.clone());
    Ok(segments)
}
```

---

### P4.3: TaskResult::output → Arc<Value>

**Concept:** Zero-copy cloning for large outputs
**Current:** `pub output: Value` (cloned on each access)
**Future:** `pub output: Arc<Value>` (Arc::clone is O(1))
**Breaking:** Requires API changes in datastore, runner

---

### P4.4: Add NikaError error codes enum

**Concept:** Structured error codes for programmatic handling
```rust
pub enum ErrorCode {
    // Schema errors (NIKA-01x)
    InvalidSchema = 10,

    // Path errors (NIKA-05x)
    InvalidPath = 50,
    TaskNotFound = 51,
    PathNotFound = 52,

    // etc...
}

impl NikaError {
    pub fn code(&self) -> ErrorCode { ... }
}
```

---

## Execution Order

```
P1 (Critical)     ──▶ Execute NOW with subagents
P2 (Optimization) ──▶ Execute after P1 verified
P3 (Documentation)──▶ Execute after P2
P4 (Future)       ──▶ Defer to v0.2 planning
```

## Verification Checklist

After each plan:
- [ ] `cargo build` passes
- [ ] `cargo test` passes (103 tests)
- [ ] `cargo clippy -- -W clippy::all` no warnings
- [ ] Commit with conventional commit message
