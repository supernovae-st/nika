//! DataStore - task output storage with DashMap (v0.1 optimized)
//!
//! Single HashMap design with lock-free concurrent access.
//! Path resolution unified with jsonpath module.
//!
//! v0.14.2: Added context storage for workflow `context:` block.
//! v0.19.4: Added inputs storage for workflow `inputs:` block.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::runtime::context_loader::LoadedContext;
use crate::binding::jsonpath;

/// Task execution status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Success,
    Failed(String),
    /// Task cannot run because a dependency failed (v0.24)
    DependencyFailed {
        /// ID of the failed dependency
        dependency: String,
    },
    /// Task was skipped (not executed)
    Skipped {
        /// Reason for skipping
        reason: String,
    },
}

/// Task execution result (unified storage)
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Output as JSON Value (Arc for O(1) cloning of large JSON structures)
    pub output: Arc<Value>,
    /// Execution duration
    pub duration: Duration,
    /// Success or failure status
    pub status: TaskStatus,
}

impl TaskResult {
    /// Create a successful result
    pub fn success(output: impl Into<Value>, duration: Duration) -> Self {
        Self {
            output: Arc::new(output.into()),
            duration,
            status: TaskStatus::Success,
        }
    }

    /// Create a successful result from string (converts to Value::String)
    pub fn success_str(output: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::String(output.into())),
            duration,
            status: TaskStatus::Success,
        }
    }

    /// Create a failed result
    pub fn failed(error: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration,
            status: TaskStatus::Failed(error.into()),
        }
    }

    /// Create a result for a task that cannot run because its dependency failed (v0.24)
    ///
    /// This is distinct from `failed()` because the task itself didn't fail -
    /// it simply cannot run because an upstream dependency failed.
    pub fn dependency_failed(dependency: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskStatus::DependencyFailed {
                dependency: dependency.into(),
            },
        }
    }

    /// Create a skipped result (v0.24)
    ///
    /// Used when a task is skipped due to cancellation or other reasons.
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskStatus::Skipped {
                reason: reason.into(),
            },
        }
    }

    /// Check if task succeeded
    pub fn is_success(&self) -> bool {
        matches!(self.status, TaskStatus::Success)
    }

    /// Check if task failed due to a dependency failure (v0.24)
    pub fn is_dependency_failed(&self) -> bool {
        matches!(self.status, TaskStatus::DependencyFailed { .. })
    }

    /// Check if task was skipped (v0.24)
    pub fn is_skipped(&self) -> bool {
        matches!(self.status, TaskStatus::Skipped { .. })
    }

    /// Check if task is in a terminal state (not pending)
    ///
    /// Returns true for Success, Failed, DependencyFailed, and Skipped.
    pub fn is_terminal(&self) -> bool {
        true // All TaskStatus variants are terminal states
    }

    /// Get the failed dependency name if this is a DependencyFailed result (v0.24)
    pub fn failed_dependency(&self) -> Option<&str> {
        match &self.status {
            TaskStatus::DependencyFailed { dependency } => Some(dependency),
            _ => None,
        }
    }

    /// Get error message if failed
    pub fn error(&self) -> Option<&str> {
        match &self.status {
            TaskStatus::Failed(e) => Some(e),
            TaskStatus::DependencyFailed { dependency } => Some(dependency),
            TaskStatus::Skipped { reason } => Some(reason),
            TaskStatus::Success => None,
        }
    }

    /// Get output as string (zero-copy for String values)
    pub fn output_str(&self) -> Cow<'_, str> {
        match &*self.output {
            Value::String(s) => Cow::Borrowed(s),
            other => Cow::Owned(other.to_string()),
        }
    }
}

/// Thread-safe storage for task results (lock-free)
///
/// Uses `Arc<str>` keys for zero-cost cloning with same Arc used in events.
///
/// v0.14.2: Added context storage for workflow `context:` block.
/// v0.19.4: Added inputs storage for workflow `inputs:` block.
#[derive(Clone, Default)]
pub struct DataStore {
    /// Task results: task_id → TaskResult
    results: Arc<DashMap<Arc<str>, TaskResult>>,

    /// Context loaded at workflow start (v0.14.2)
    ///
    /// Contains files loaded from the `context:` block.
    /// Accessible via `{{context.files.alias}}` bindings.
    context: Arc<RwLock<LoadedContext>>,

    /// Input parameters with defaults (v0.19.4)
    ///
    /// Contains input definitions from the `inputs:` block.
    /// Accessible via `{{inputs.param}}` bindings.
    inputs: Arc<RwLock<FxHashMap<String, Value>>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a task result (accepts `Arc<str>` for zero-cost key reuse)
    pub fn insert(&self, task_id: Arc<str>, result: TaskResult) {
        self.results.insert(task_id, result);
    }

    /// Get a task result
    pub fn get(&self, task_id: &str) -> Option<TaskResult> {
        self.results.get(task_id).map(|r| r.value().clone())
    }

    /// Check if task exists
    pub fn contains(&self, task_id: &str) -> bool {
        self.results.contains_key(task_id)
    }

    /// Check if task succeeded
    pub fn is_success(&self, task_id: &str) -> bool {
        self.get(task_id).is_some_and(|r| r.is_success())
    }

    /// Check if task failed (either directly or due to dependency failure) (v0.24)
    pub fn is_failed(&self, task_id: &str) -> bool {
        self.get(task_id).is_some_and(|r| {
            matches!(
                r.status,
                TaskStatus::Failed(_) | TaskStatus::DependencyFailed { .. }
            )
        })
    }

    /// Check if task failed due to a dependency failure (v0.24)
    pub fn is_dependency_failed(&self, task_id: &str) -> bool {
        self.get(task_id).is_some_and(|r| r.is_dependency_failed())
    }

    /// Get the failed dependency name if task has DependencyFailed status (v0.24)
    pub fn get_failed_dependency(&self, task_id: &str) -> Option<String> {
        self.get(task_id)
            .and_then(|r| r.failed_dependency().map(String::from))
    }

    /// Get just the output Value for a task (for JSONPath resolution)
    /// Returns `Arc<Value>` for O(1) cloning instead of deep copy
    pub fn get_output(&self, task_id: &str) -> Option<Arc<Value>> {
        self.results.get(task_id).map(|r| Arc::clone(&r.output))
    }

    /// Resolve a dot-separated path (e.g., "weather.summary")
    ///
    /// Uses jsonpath module internally for unified path resolution.
    /// Supports both simple dot notation and array indices.
    pub fn resolve_path(&self, path: &str) -> Option<Value> {
        let mut parts = path.splitn(2, '.');
        let task_id = parts.next()?;

        let output = self.get_output(task_id)?;

        // If no remaining path, return the whole output (clone from Arc)
        let Some(remaining) = parts.next() else {
            return Some((*output).clone());
        };

        // Use jsonpath for path resolution (handles both dots and array indices)
        // Arc<Value> derefs to &Value, so this works without changes
        jsonpath::resolve(&output, remaining).ok().flatten()
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONTEXT STORAGE (v0.14.2 Schema @0.9)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set workflow context (v0.14.2)
    ///
    /// Called by Runner at workflow start after loading context files.
    pub fn set_context(&self, context: LoadedContext) {
        *self.context.write() = context;
    }

    /// Get a context file by alias (v0.14.2)
    ///
    /// Returns the loaded value for `{{context.files.alias}}` bindings.
    pub fn get_context_file(&self, alias: &str) -> Option<Value> {
        self.context.read().get_file(alias).cloned()
    }

    /// Get session data (v0.14.2)
    ///
    /// Returns the loaded session for `{{context.session.key}}` bindings.
    pub fn get_context_session(&self) -> Option<Value> {
        self.context.read().get_session().cloned()
    }

    /// Check if context is loaded (v0.14.2)
    pub fn has_context(&self) -> bool {
        !self.context.read().is_empty()
    }

    /// Resolve a context path (v0.14.2)
    ///
    /// Supports:
    /// - `context.files.alias` → file content
    /// - `context.files.alias.field` → nested field
    /// - `context.session` → session data
    /// - `context.session.field` → session field
    pub fn resolve_context_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let context = self.context.read();

        match parts[1] {
            "files" => {
                if parts.len() < 3 {
                    return None;
                }
                let alias = parts[2];
                let value = context.get_file(alias)?;

                if parts.len() == 3 {
                    // context.files.alias → full file content
                    Some(value.clone())
                } else {
                    // context.files.alias.field → nested path
                    let remaining = parts[3..].join(".");
                    jsonpath::resolve(value, &remaining).ok().flatten()
                }
            }
            "session" => {
                let session = context.get_session()?;

                if parts.len() == 2 {
                    // context.session → full session
                    Some(session.clone())
                } else {
                    // context.session.field → nested path
                    let remaining = parts[2..].join(".");
                    jsonpath::resolve(session, &remaining).ok().flatten()
                }
            }
            _ => None,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // INPUTS STORAGE (v0.19.4 Schema @0.10)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set workflow inputs (v0.19.4)
    ///
    /// Called by Runner at workflow start with input definitions.
    /// Each input is a JSON object with `type`, `default`, `description`, etc.
    pub fn set_inputs(&self, inputs: FxHashMap<String, Value>) {
        *self.inputs.write() = inputs;
    }

    /// Get an input's value by name (v0.19.4)
    ///
    /// Supports two formats:
    /// - Full form: `{ type: string, default: "value" }` → extracts `default` field
    /// - Shorthand: `"value"` or `123` or `true` → uses value directly
    ///
    /// Returns `None` if input doesn't exist.
    pub fn get_input_default(&self, name: &str) -> Option<Value> {
        let inputs = self.inputs.read();
        let definition = inputs.get(name)?;

        // Check if this is a full input definition with a `default` field
        // or a shorthand value (string, number, bool, array)
        if let Some(obj) = definition.as_object() {
            // Full form: { type, default, description, ... }
            // Check for 'default' field, or 'value' as alternative
            if let Some(default_val) = obj.get("default").or_else(|| obj.get("value")) {
                return Some(default_val.clone());
            }
            // If object has type/description but no default, return None
            if obj.contains_key("type") || obj.contains_key("description") {
                return None;
            }
        }

        // Shorthand: the value itself is the default
        // e.g., `name: "TestUser"` or `count: 5`
        Some(definition.clone())
    }

    /// Check if inputs are loaded (v0.19.4)
    pub fn has_inputs(&self) -> bool {
        !self.inputs.read().is_empty()
    }

    /// Resolve an input path (v0.19.4)
    ///
    /// Supports:
    /// - `inputs.param` → default value of parameter
    /// - `inputs.param.field` → nested field in default value (if object)
    pub fn resolve_input_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() || parts[0] != "inputs" {
            return None;
        }
        if parts.len() < 2 {
            return None;
        }

        let param_name = parts[1];
        let default_value = self.get_input_default(param_name)?;

        if parts.len() == 2 {
            // inputs.param → full default value
            Some(default_value)
        } else {
            // inputs.param.field → nested path in default value
            let remaining = parts[2..].join(".");
            jsonpath::resolve(&default_value, &remaining).ok().flatten()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn insert_and_get_result() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"key": "value"}), Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert!(result.is_success());
        assert_eq!(result.output["key"], "value");
    }

    #[test]
    fn success_str_converts_to_value() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task1"),
            TaskResult::success_str("hello", Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert_eq!(*result.output, Value::String("hello".to_string()));
        assert_eq!(result.output_str(), "hello");
    }

    #[test]
    fn failed_result() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task1"),
            TaskResult::failed("oops", Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert!(!result.is_success());
        assert_eq!(result.error(), Some("oops"));
    }

    #[test]
    fn resolve_simple_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
        );

        let value = store.resolve_path("weather.summary").unwrap();
        assert_eq!(value, "Sunny");
    }

    #[test]
    fn resolve_nested_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("flights"),
            TaskResult::success(
                json!({"cheapest": {"price": 89, "airline": "AF"}}),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("flights.cheapest.price").unwrap(), 89);
        assert_eq!(
            store.resolve_path("flights.cheapest.airline").unwrap(),
            "AF"
        );
    }

    #[test]
    fn resolve_array_index() {
        let store = DataStore::new();
        store.insert(
            Arc::from("data"),
            TaskResult::success(
                json!({"items": ["first", "second"]}),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("data.items.0").unwrap(), "first");
        assert_eq!(store.resolve_path("data.items.1").unwrap(), "second");
    }

    #[test]
    fn resolve_path_not_found() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"a": 1}), Duration::from_secs(1)),
        );

        assert!(store.resolve_path("task1.nonexistent").is_none());
        assert!(store.resolve_path("unknown.field").is_none());
    }

    // =========================================================================
    // Concurrent Access Tests (v0.5.0 - Plan B Test Coverage)
    // =========================================================================

    #[test]
    fn concurrent_writes_all_stored() {
        use std::thread;

        let store = DataStore::new();
        let store_arc = Arc::new(store);

        let handles: Vec<_> = (0..100)
            .map(|i| {
                let store = Arc::clone(&store_arc);
                thread::spawn(move || {
                    store.insert(
                        Arc::from(format!("task_{}", i)),
                        TaskResult::success(json!({"index": i}), Duration::from_millis(i)),
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All 100 keys should exist
        for i in 0..100 {
            assert!(
                store_arc.contains(&format!("task_{}", i)),
                "task_{} should exist",
                i
            );
        }
    }

    #[test]
    fn concurrent_reads_during_writes() {
        use std::thread;

        let store = Arc::new(DataStore::new());

        // Pre-populate some data
        for i in 0..50 {
            store.insert(
                Arc::from(format!("initial_{}", i)),
                TaskResult::success(json!({"value": i}), Duration::from_millis(i)),
            );
        }

        let store_writer = Arc::clone(&store);
        let store_reader = Arc::clone(&store);

        // Spawn writer thread
        let writer = thread::spawn(move || {
            for i in 0..100 {
                store_writer.insert(
                    Arc::from(format!("new_{}", i)),
                    TaskResult::success(json!({"new": i}), Duration::from_millis(i)),
                );
            }
        });

        // Spawn reader thread - should not block
        let reader = thread::spawn(move || {
            let mut read_count = 0;
            for i in 0..50 {
                if store_reader.get(&format!("initial_{}", i)).is_some() {
                    read_count += 1;
                }
            }
            read_count
        });

        writer.join().unwrap();
        let reads = reader.join().unwrap();

        // Reader should have been able to read existing data
        assert_eq!(reads, 50, "Should read all 50 initial entries");

        // Verify writer completed
        for i in 0..100 {
            assert!(store.contains(&format!("new_{}", i)));
        }
    }

    #[test]
    fn overwrite_existing_task() {
        let store = DataStore::new();

        // Insert initial value
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"version": 1}), Duration::from_secs(1)),
        );

        // Overwrite with new value
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"version": 2}), Duration::from_secs(2)),
        );

        let result = store.get("task1").unwrap();
        assert_eq!(result.output["version"], 2);
        assert_eq!(result.duration, Duration::from_secs(2));
    }

    // =========================================================================
    // Edge Case Tests (v0.5.0 - Plan B Test Coverage)
    // =========================================================================

    #[test]
    fn contains_and_is_success() {
        let store = DataStore::new();

        // Non-existent task
        assert!(!store.contains("nonexistent"));
        assert!(!store.is_success("nonexistent"));

        // Successful task
        store.insert(
            Arc::from("success"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );
        assert!(store.contains("success"));
        assert!(store.is_success("success"));

        // Failed task
        store.insert(
            Arc::from("failed"),
            TaskResult::failed("error", Duration::from_secs(1)),
        );
        assert!(store.contains("failed"));
        assert!(!store.is_success("failed"));
    }

    #[test]
    fn get_output_returns_arc() {
        let store = DataStore::new();

        let big_json = json!({
            "large": "data".repeat(1000),
            "nested": {"deep": {"value": 42}}
        });

        store.insert(
            Arc::from("big"),
            TaskResult::success(big_json.clone(), Duration::from_secs(1)),
        );

        // get_output should return Arc (cheap clone)
        let output1 = store.get_output("big").unwrap();
        let output2 = store.get_output("big").unwrap();

        // Both should point to same data (Arc comparison)
        assert!(Arc::ptr_eq(&output1, &output2));
    }

    #[test]
    fn resolve_task_only_returns_full_output() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"a": 1, "b": 2}), Duration::from_secs(1)),
        );

        // Just task name should return full output
        let full = store.resolve_path("task").unwrap();
        assert_eq!(full, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn resolve_deeply_nested_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("deep"),
            TaskResult::success(
                json!({"level1": {"level2": {"level3": {"level4": "found"}}}}),
                Duration::from_secs(1),
            ),
        );

        let value = store
            .resolve_path("deep.level1.level2.level3.level4")
            .unwrap();
        assert_eq!(value, "found");
    }

    #[test]
    fn resolve_mixed_array_object_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("mixed"),
            TaskResult::success(
                json!({
                    "users": [
                        {"name": "Alice", "scores": [90, 85, 92]},
                        {"name": "Bob", "scores": [78, 82]}
                    ]
                }),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("mixed.users.0.name").unwrap(), "Alice");
        assert_eq!(store.resolve_path("mixed.users.1.name").unwrap(), "Bob");
        assert_eq!(store.resolve_path("mixed.users.0.scores.2").unwrap(), 92);
    }

    #[test]
    fn output_str_cow_borrowed_for_strings() {
        let result = TaskResult::success_str("hello", Duration::from_secs(1));

        let cow = result.output_str();
        // Should be borrowed (no allocation for string values)
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*cow, "hello");
    }

    #[test]
    fn output_str_cow_owned_for_non_strings() {
        let result = TaskResult::success(json!({"num": 42}), Duration::from_secs(1));

        let cow = result.output_str();
        // Should be owned (converted to string)
        assert!(matches!(cow, std::borrow::Cow::Owned(_)));
        assert!(cow.contains("42"));
    }

    #[test]
    fn empty_task_id_resolves_nothing() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );

        // Empty path should return None
        assert!(store.resolve_path("").is_none());
    }

    #[test]
    fn clone_is_shallow() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": 42}), Duration::from_secs(1)),
        );

        // Clone the store
        let cloned = store.clone();

        // Both should see the same data (shared Arc<DashMap>)
        assert_eq!(
            store.get("task").unwrap().output,
            cloned.get("task").unwrap().output
        );

        // Insert into original
        store.insert(
            Arc::from("new"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );

        // Clone should also see it (same underlying DashMap)
        assert!(cloned.contains("new"));
    }

    // =========================================================================
    // Context Storage Tests (v0.14.2 Schema @0.9)
    // =========================================================================

    #[test]
    fn test_context_default_is_empty() {
        let store = DataStore::new();
        assert!(!store.has_context());
    }

    #[test]
    fn test_set_and_get_context_file() {
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("# Brand Guide"));

        store.set_context(context);

        assert!(store.has_context());
        assert_eq!(
            store.get_context_file("brand"),
            Some(json!("# Brand Guide"))
        );
        assert!(store.get_context_file("nonexistent").is_none());
    }

    #[test]
    fn test_set_and_get_context_session() {
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.session = Some(json!({"focus_areas": ["rust", "ai"]}));

        store.set_context(context);

        assert!(store.has_context());
        let session = store.get_context_session().unwrap();
        assert!(session["focus_areas"].is_array());
    }

    #[test]
    fn test_resolve_context_path_files() {
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.files.insert(
            "persona".to_string(),
            json!({"name": "Agent", "role": "assistant"}),
        );

        store.set_context(context);

        // Full file
        assert_eq!(
            store.resolve_context_path("context.files.persona"),
            Some(json!({"name": "Agent", "role": "assistant"}))
        );

        // Nested field
        assert_eq!(
            store.resolve_context_path("context.files.persona.name"),
            Some(json!("Agent"))
        );

        // Missing file
        assert!(store
            .resolve_context_path("context.files.missing")
            .is_none());
    }

    #[test]
    fn test_resolve_context_path_session() {
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.session = Some(json!({"focus": "rust", "level": 3}));

        store.set_context(context);

        // Full session
        assert_eq!(
            store.resolve_context_path("context.session"),
            Some(json!({"focus": "rust", "level": 3}))
        );

        // Nested field
        assert_eq!(
            store.resolve_context_path("context.session.focus"),
            Some(json!("rust"))
        );
        assert_eq!(
            store.resolve_context_path("context.session.level"),
            Some(json!(3))
        );
    }

    #[test]
    fn test_resolve_context_path_invalid() {
        let store = DataStore::new();

        let mut context = LoadedContext::new();
        context.files.insert("brand".to_string(), json!("content"));

        store.set_context(context);

        // Invalid paths
        assert!(store.resolve_context_path("context").is_none());
        assert!(store.resolve_context_path("context.invalid").is_none());
        assert!(store.resolve_context_path("context.files").is_none());
        assert!(store.resolve_context_path("other.path").is_none());
    }

    // =========================================================================
    // Inputs Storage Tests (v0.19.4 Schema @0.10)
    // =========================================================================

    #[test]
    fn test_inputs_default_is_empty() {
        let store = DataStore::new();
        assert!(!store.has_inputs());
    }

    #[test]
    fn test_set_and_get_input_default() {
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "description": "Research topic",
                "default": "AI QR code generation"
            }),
        );

        store.set_inputs(inputs);

        assert!(store.has_inputs());
        assert_eq!(
            store.get_input_default("topic"),
            Some(json!("AI QR code generation"))
        );
        assert!(store.get_input_default("nonexistent").is_none());
    }

    #[test]
    fn test_get_input_default_without_default() {
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        // Input without default field
        inputs.insert(
            "required_param".to_string(),
            json!({
                "type": "string",
                "description": "A required parameter"
            }),
        );

        store.set_inputs(inputs);

        // Should return None for input without default
        assert!(store.get_input_default("required_param").is_none());
    }

    #[test]
    fn test_resolve_input_path_simple() {
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI trends 2025"
            }),
        );
        inputs.insert(
            "depth".to_string(),
            json!({
                "type": "string",
                "default": "comprehensive"
            }),
        );

        store.set_inputs(inputs);

        // Resolve inputs.topic
        assert_eq!(
            store.resolve_input_path("inputs.topic"),
            Some(json!("AI trends 2025"))
        );

        // Resolve inputs.depth
        assert_eq!(
            store.resolve_input_path("inputs.depth"),
            Some(json!("comprehensive"))
        );

        // Missing input
        assert!(store.resolve_input_path("inputs.missing").is_none());
    }

    #[test]
    fn test_resolve_input_path_nested() {
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "config".to_string(),
            json!({
                "type": "object",
                "default": {
                    "theme": "dark",
                    "version": 2,
                    "nested": {
                        "deep": "value"
                    }
                }
            }),
        );

        store.set_inputs(inputs);

        // Resolve nested fields
        assert_eq!(
            store.resolve_input_path("inputs.config.theme"),
            Some(json!("dark"))
        );
        assert_eq!(
            store.resolve_input_path("inputs.config.version"),
            Some(json!(2))
        );
        assert_eq!(
            store.resolve_input_path("inputs.config.nested.deep"),
            Some(json!("value"))
        );
    }

    #[test]
    fn test_resolve_input_path_invalid() {
        let store = DataStore::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "test"
            }),
        );

        store.set_inputs(inputs);

        // Invalid paths
        assert!(store.resolve_input_path("inputs").is_none());
        assert!(store.resolve_input_path("other.path").is_none());
        assert!(store.resolve_input_path("").is_none());
    }
}
