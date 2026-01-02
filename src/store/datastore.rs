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
        assert_eq!(store.resolve_path("flights.cheapest.airline").unwrap(), "AF");
    }

    #[test]
    fn resolve_array_index() {
        let store = DataStore::new();
        store.insert(
            Arc::from("data"),
            TaskResult::success(json!({"items": ["first", "second"]}), Duration::from_secs(1)),
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
}
