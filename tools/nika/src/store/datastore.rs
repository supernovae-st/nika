//! DataStore - task output storage with DashMap (v0.1 optimized)
//!
//! Single HashMap design with lock-free concurrent access.
//! Path resolution unified with jsonpath module.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use serde_json::Value;

use crate::util::jsonpath;

/// Task execution status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Success,
    Failed(String),
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

    /// Check if task succeeded
    pub fn is_success(&self) -> bool {
        matches!(self.status, TaskStatus::Success)
    }

    /// Get error message if failed
    pub fn error(&self) -> Option<&str> {
        match &self.status {
            TaskStatus::Failed(e) => Some(e),
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
/// Uses Arc<str> keys for zero-cost cloning with same Arc used in events.
#[derive(Clone, Default)]
pub struct DataStore {
    /// Task results: task_id → TaskResult
    results: Arc<DashMap<Arc<str>, TaskResult>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a task result (accepts Arc<str> for zero-cost key reuse)
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

    /// Get just the output Value for a task (for JSONPath resolution)
    /// Returns Arc<Value> for O(1) cloning instead of deep copy
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
}
