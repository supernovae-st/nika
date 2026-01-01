//! Task output storage with DashMap (v0.1 optimized)
//!
//! Single HashMap design with lock-free concurrent access.

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// Task execution status
#[derive(Debug, Clone)]
pub enum TaskStatus {
    Success,
    Failed(String),
}

/// Task execution result (unified storage)
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Output as JSON Value (String content wrapped as Value::String)
    pub output: Value,
    /// Execution duration
    pub duration: Duration,
    /// Success or failure status
    pub status: TaskStatus,
}

impl TaskResult {
    /// Create a successful result
    pub fn success(output: impl Into<Value>, duration: Duration) -> Self {
        Self {
            output: output.into(),
            duration,
            status: TaskStatus::Success,
        }
    }

    /// Create a successful result from string (converts to Value::String)
    pub fn success_str(output: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Value::String(output.into()),
            duration,
            status: TaskStatus::Success,
        }
    }

    /// Create a failed result
    pub fn failed(error: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Value::Null,
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

    /// Get output as string
    pub fn output_str(&self) -> String {
        match &self.output {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }
}

/// Thread-safe storage for task results (lock-free)
#[derive(Clone, Default)]
pub struct DataStore {
    /// Task results: task_id → TaskResult
    results: Arc<DashMap<String, TaskResult>>,
}

impl DataStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a task result
    pub fn insert(&self, task_id: impl Into<String>, result: TaskResult) {
        self.results.insert(task_id.into(), result);
    }

    /// Get a task result
    pub fn get(&self, task_id: &str) -> Option<TaskResult> {
        self.results.get(task_id).map(|r| r.clone())
    }

    /// Check if task exists
    pub fn contains(&self, task_id: &str) -> bool {
        self.results.contains_key(task_id)
    }

    /// Check if task succeeded
    pub fn is_success(&self, task_id: &str) -> bool {
        self.get(task_id).map(|r| r.is_success()).unwrap_or(false)
    }

    /// Get just the output Value for a task (for JSONPath resolution)
    pub fn get_output(&self, task_id: &str) -> Option<Value> {
        self.results.get(task_id).map(|r| r.output.clone())
    }

    /// Resolve a dot-separated path (e.g., "weather.summary")
    pub fn resolve_path(&self, path: &str) -> Option<Value> {
        let mut parts = path.split('.');
        let task_id = parts.next()?;

        let result = self.results.get(task_id)?;
        let mut value = result.output.clone();

        for segment in parts {
            value = if let Ok(idx) = segment.parse::<usize>() {
                value.get(idx)?.clone()
            } else {
                value.get(segment)?.clone()
            };
        }
        Some(value)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn insert_and_get_result() {
        let store = DataStore::new();
        store.insert("task1", TaskResult::success(json!({"key": "value"}), Duration::from_secs(1)));

        let result = store.get("task1").unwrap();
        assert!(result.is_success());
        assert_eq!(result.output["key"], "value");
    }

    #[test]
    fn success_str_converts_to_value() {
        let store = DataStore::new();
        store.insert("task1", TaskResult::success_str("hello", Duration::from_secs(1)));

        let result = store.get("task1").unwrap();
        assert_eq!(result.output, Value::String("hello".to_string()));
        assert_eq!(result.output_str(), "hello");
    }

    #[test]
    fn failed_result() {
        let store = DataStore::new();
        store.insert("task1", TaskResult::failed("oops", Duration::from_secs(1)));

        let result = store.get("task1").unwrap();
        assert!(!result.is_success());
        assert_eq!(result.error(), Some("oops"));
    }

    #[test]
    fn resolve_simple_path() {
        let store = DataStore::new();
        store.insert("weather", TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)));

        let value = store.resolve_path("weather.summary").unwrap();
        assert_eq!(value, "Sunny");
    }

    #[test]
    fn resolve_nested_path() {
        let store = DataStore::new();
        store.insert("flights", TaskResult::success(
            json!({"cheapest": {"price": 89, "airline": "AF"}}),
            Duration::from_secs(1)
        ));

        assert_eq!(store.resolve_path("flights.cheapest.price").unwrap(), 89);
        assert_eq!(store.resolve_path("flights.cheapest.airline").unwrap(), "AF");
    }

    #[test]
    fn resolve_array_index() {
        let store = DataStore::new();
        store.insert("data", TaskResult::success(
            json!({"items": ["first", "second"]}),
            Duration::from_secs(1)
        ));

        assert_eq!(store.resolve_path("data.items.0").unwrap(), "first");
        assert_eq!(store.resolve_path("data.items.1").unwrap(), "second");
    }

    #[test]
    fn resolve_path_not_found() {
        let store = DataStore::new();
        store.insert("task1", TaskResult::success(json!({"a": 1}), Duration::from_secs(1)));

        assert!(store.resolve_path("task1.nonexistent").is_none());
        assert!(store.resolve_path("unknown.field").is_none());
    }

}
