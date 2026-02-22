//! Resolved Bindings - runtime value resolution (v0.5)
//!
//! ResolvedBindings holds resolved values from `use:` blocks for template resolution.
//! Supports both eager (immediate) and lazy (deferred) resolution.
//!
//! Unified syntax: `alias: task.path [?? default]`
//! Extended syntax: `alias: {path: task.path, lazy: true}`
//!
//! Uses FxHashMap for faster hashing (consistent with FlowGraph).

use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::error::NikaError;
use crate::store::DataStore;
use crate::util::jsonpath;

use super::entry::{UseEntry, WiringSpec};

/// Lazy binding state - either resolved or pending (v0.5)
#[derive(Debug, Clone)]
pub enum LazyBinding {
    /// Already resolved to a concrete value (eager bindings)
    Resolved(Value),
    /// Pending resolution - stores path and default for deferred resolution
    Pending {
        path: String,
        default: Option<Value>,
    },
}

impl LazyBinding {
    /// Check if this binding is pending resolution
    pub fn is_pending(&self) -> bool {
        matches!(self, LazyBinding::Pending { .. })
    }

    /// Get the value if already resolved
    pub fn get_value(&self) -> Option<&Value> {
        match self {
            LazyBinding::Resolved(v) => Some(v),
            LazyBinding::Pending { .. } => None,
        }
    }
}

/// Resolved bindings from use: block (alias -> value or pending)
///
/// Uses FxHashMap for faster hashing on small string keys.
/// Supports both eager and lazy bindings (v0.5).
#[derive(Debug, Clone, Default)]
pub struct ResolvedBindings {
    /// Alias -> binding mappings (resolved or pending)
    bindings: FxHashMap<String, LazyBinding>,
}

impl ResolvedBindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self::default()
    }

    /// Build bindings from use: wiring by resolving paths from datastore (v0.5)
    ///
    /// Unified resolution for both syntax styles:
    /// - String: `task.path [?? default]` → eager resolution
    /// - Object: `{path, lazy?, default?}` → lazy or eager based on flag
    ///
    /// Lazy bindings are stored as Pending and resolved on first access.
    /// Eager bindings are resolved immediately and fail if source is missing.
    ///
    /// Returns empty bindings if use_wiring is None.
    pub fn from_wiring_spec(
        wiring_spec: Option<&WiringSpec>,
        datastore: &DataStore,
    ) -> Result<Self, NikaError> {
        let Some(wiring) = wiring_spec else {
            return Ok(Self::new());
        };

        let mut bindings = Self::new();

        for (alias, entry) in wiring {
            if entry.is_lazy() {
                // Lazy binding - defer resolution
                bindings.bindings.insert(
                    alias.clone(),
                    LazyBinding::Pending {
                        path: entry.path.clone(),
                        default: entry.default.clone(),
                    },
                );
            } else {
                // Eager binding - resolve immediately
                let value = resolve_entry(entry, alias, datastore)?;
                bindings
                    .bindings
                    .insert(alias.clone(), LazyBinding::Resolved(value));
            }
        }

        Ok(bindings)
    }

    /// Set a resolved value (always eager)
    pub fn set(&mut self, alias: impl Into<String>, value: Value) {
        self.bindings
            .insert(alias.into(), LazyBinding::Resolved(value));
    }

    /// Get a resolved value (only works for already-resolved bindings)
    ///
    /// For lazy bindings that haven't been resolved yet, returns None.
    /// Use `get_resolved()` to force resolution of lazy bindings.
    pub fn get(&self, alias: &str) -> Option<&Value> {
        self.bindings.get(alias).and_then(|b| b.get_value())
    }

    /// Get a resolved value, resolving lazy bindings on demand (v0.5)
    ///
    /// For eager bindings, returns the pre-resolved value.
    /// For lazy bindings, resolves from datastore on first call.
    ///
    /// Note: This doesn't cache the resolution - each call re-resolves.
    /// This is intentional to support changing datastore values.
    pub fn get_resolved(&self, alias: &str, datastore: &DataStore) -> Result<Value, NikaError> {
        match self.bindings.get(alias) {
            Some(LazyBinding::Resolved(value)) => Ok(value.clone()),
            Some(LazyBinding::Pending { path, default }) => {
                // Resolve on demand
                let entry = UseEntry {
                    path: path.clone(),
                    default: default.clone(),
                    lazy: true,
                };
                resolve_entry(&entry, alias, datastore)
            }
            None => Err(NikaError::BindingNotFound {
                alias: alias.to_string(),
            }),
        }
    }

    /// Check if a binding is lazy (pending resolution) (v0.5)
    pub fn is_lazy(&self, alias: &str) -> bool {
        self.bindings
            .get(alias)
            .map(|b| b.is_pending())
            .unwrap_or(false)
    }

    /// Check if context has any bindings
    #[allow(dead_code)] // Used in tests
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Iterate over resolved bindings (alias, value pairs)
    ///
    /// Only returns already-resolved bindings. Pending lazy bindings are skipped.
    /// Use this for event logging where we want to capture resolved values.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.bindings
            .iter()
            .filter_map(|(alias, binding)| binding.get_value().map(|value| (alias.as_str(), value)))
    }

    /// Serialize context to JSON Value for event logging
    ///
    /// Returns the full resolved inputs as a JSON object.
    /// Lazy bindings that haven't been resolved are represented as null.
    /// Used by EventLog for TaskStarted events (inputs field).
    pub fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (alias, binding) in &self.bindings {
            match binding {
                LazyBinding::Resolved(v) => {
                    map.insert(alias.clone(), v.clone());
                }
                LazyBinding::Pending { path, default: _ } => {
                    // Represent pending as a marker object
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": path}),
                    );
                }
            }
        }
        Value::Object(map)
    }
}

/// Resolve a single UseEntry to a Value
///
/// Unified resolution logic:
/// 1. Extract task_id from path (first segment)
/// 2. Get task output from datastore
/// 3. Resolve remaining path within output
/// 4. Apply default if value is null/missing
fn resolve_entry(entry: &UseEntry, alias: &str, datastore: &DataStore) -> Result<Value, NikaError> {
    let path = &entry.path;

    // Split path into task_id and remaining path
    let (task_id, field_path) = split_path(path);

    // Resolve the value from task output
    let value = match datastore.get_output(task_id) {
        Some(output) => {
            if let Some(fp) = field_path {
                jsonpath::resolve(&output, fp)?
            } else {
                Some((*output).clone())
            }
        }
        None => None,
    };

    // Apply default if value is null or missing
    match value {
        Some(v) if !v.is_null() => Ok(v),
        Some(_) => entry
            .default
            .as_ref()
            .cloned()
            .ok_or_else(|| NikaError::NullValue {
                path: path.clone(),
                alias: alias.to_string(),
            }),
        None => entry
            .default
            .as_ref()
            .cloned()
            .ok_or_else(|| NikaError::PathNotFound { path: path.clone() }),
    }
}

/// Split a path into task_id and remaining field path
///
/// Examples:
/// - "weather" -> ("weather", None)
/// - "weather.summary" -> ("weather", Some("summary"))
/// - "weather.data.temp" -> ("weather", Some("data.temp"))
fn split_path(path: &str) -> (&str, Option<&str>) {
    if let Some(dot_idx) = path.find('.') {
        let task_id = &path[..dot_idx];
        let field_path = &path[dot_idx + 1..];
        (task_id, Some(field_path))
    } else {
        (path, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::TaskResult;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    // ═══════════════════════════════════════════════════════════════
    // Basic tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn set_and_get() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("forecast", json!("Sunny"));

        assert_eq!(bindings.get("forecast"), Some(&json!("Sunny")));
        assert_eq!(bindings.get("unknown"), None);
    }

    #[test]
    fn is_empty() {
        let mut bindings = ResolvedBindings::new();
        assert!(bindings.is_empty());

        bindings.set("key", json!("value"));
        assert!(!bindings.is_empty());
    }

    #[test]
    fn from_wiring_spec_none() {
        let store = DataStore::new();
        let bindings = ResolvedBindings::from_wiring_spec(None, &store).unwrap();
        assert!(bindings.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // Unified syntax tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_simple_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("forecast".to_string(), UseEntry::new("weather.summary"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("Sunny")));
    }

    #[test]
    fn resolve_entire_task_output() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(
                json!({"summary": "Sunny", "temp": 25}),
                Duration::from_secs(1),
            ),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("data".to_string(), UseEntry::new("weather"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(
            bindings.get("data"),
            Some(&json!({"summary": "Sunny", "temp": 25}))
        );
    }

    #[test]
    fn resolve_nested_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(
                json!({"data": {"temp": {"celsius": 25}}}),
                Duration::from_secs(1),
            ),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "temp".to_string(),
            UseEntry::new("weather.data.temp.celsius"),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("temp"), Some(&json!(25)));
    }

    #[test]
    fn resolve_with_default_on_missing() {
        let store = DataStore::new();
        // No weather task in store

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "forecast".to_string(),
            UseEntry::with_default("weather.summary", json!("Unknown")),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("Unknown")));
    }

    #[test]
    fn resolve_with_default_on_null() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": null}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "forecast".to_string(),
            UseEntry::with_default("weather.summary", json!("N/A")),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("N/A")));
    }

    #[test]
    fn resolve_with_default_object() {
        let store = DataStore::new();
        // No settings task

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "cfg".to_string(),
            UseEntry::with_default("settings", json!({"debug": false})),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("cfg"), Some(&json!({"debug": false})));
    }

    #[test]
    fn resolve_with_default_array() {
        let store = DataStore::new();
        // No meta task

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "tags".to_string(),
            UseEntry::with_default("meta.tags", json!(["default"])),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("tags"), Some(&json!(["default"])));
    }

    // ═══════════════════════════════════════════════════════════════
    // Error cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_path_not_found_error() {
        let store = DataStore::new();

        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new("missing.path"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052"));
    }

    #[test]
    fn resolve_null_strict_error() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": null}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("forecast".to_string(), UseEntry::new("weather.summary"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    // ═══════════════════════════════════════════════════════════════
    // JSONPath tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_jsonpath_array_index() {
        let store = DataStore::new();
        store.insert(
            Arc::from("data"),
            TaskResult::success(
                json!({"items": [{"name": "first"}, {"name": "second"}]}),
                Duration::from_secs(1),
            ),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("first".to_string(), UseEntry::new("data.items[0].name"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("first"), Some(&json!("first")));
    }

    // ═══════════════════════════════════════════════════════════════
    // split_path() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn split_path_task_only() {
        let (task_id, field_path) = split_path("weather");
        assert_eq!(task_id, "weather");
        assert_eq!(field_path, None);
    }

    #[test]
    fn split_path_with_field() {
        let (task_id, field_path) = split_path("weather.summary");
        assert_eq!(task_id, "weather");
        assert_eq!(field_path, Some("summary"));
    }

    #[test]
    fn split_path_nested() {
        let (task_id, field_path) = split_path("weather.data.temp.celsius");
        assert_eq!(task_id, "weather");
        assert_eq!(field_path, Some("data.temp.celsius"));
    }

    // ═══════════════════════════════════════════════════════════════
    // to_value() for event logging
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn to_value_serializes_resolved_inputs() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("weather", json!("sunny"));
        bindings.set("temp", json!(25));
        bindings.set("nested", json!({"key": "value"}));

        let value = bindings.to_value();

        assert!(value.is_object());
        assert_eq!(value["weather"], "sunny");
        assert_eq!(value["temp"], 25);
        assert_eq!(value["nested"]["key"], "value");
    }

    #[test]
    fn to_value_empty_bindings() {
        let bindings = ResolvedBindings::new();
        let value = bindings.to_value();

        assert!(value.is_object());
        assert!(value.as_object().unwrap().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // LazyBinding::is_pending() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn lazy_binding_resolved_not_pending() {
        let binding = LazyBinding::Resolved(json!("value"));
        assert!(!binding.is_pending());
    }

    #[test]
    fn lazy_binding_pending_is_pending() {
        let binding = LazyBinding::Pending {
            path: "task.path".to_string(),
            default: None,
        };
        assert!(binding.is_pending());
    }

    #[test]
    fn lazy_binding_pending_with_default_is_pending() {
        let binding = LazyBinding::Pending {
            path: "task.path".to_string(),
            default: Some(json!("fallback")),
        };
        assert!(binding.is_pending());
    }

    // ═══════════════════════════════════════════════════════════════
    // LazyBinding::get_value() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn lazy_binding_get_value_resolved() {
        let binding = LazyBinding::Resolved(json!("resolved"));
        assert_eq!(binding.get_value(), Some(&json!("resolved")));
    }

    #[test]
    fn lazy_binding_get_value_pending() {
        let binding = LazyBinding::Pending {
            path: "task.path".to_string(),
            default: None,
        };
        assert_eq!(binding.get_value(), None);
    }

    #[test]
    fn lazy_binding_get_value_complex_value() {
        let complex = json!({"nested": {"value": 42}, "array": [1, 2, 3]});
        let binding = LazyBinding::Resolved(complex.clone());
        assert_eq!(binding.get_value(), Some(&complex));
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::new() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn new_creates_empty_bindings() {
        let bindings = ResolvedBindings::new();
        assert!(bindings.is_empty());
        assert_eq!(bindings.get("anything"), None);
    }

    #[test]
    fn default_creates_empty_bindings() {
        let bindings = ResolvedBindings::default();
        assert!(bindings.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::set() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn set_multiple_values() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("key1", json!("value1"));
        bindings.set("key2", json!(42));
        bindings.set("key3", json!({"nested": true}));

        assert_eq!(bindings.get("key1"), Some(&json!("value1")));
        assert_eq!(bindings.get("key2"), Some(&json!(42)));
        assert_eq!(bindings.get("key3"), Some(&json!({"nested": true})));
    }

    #[test]
    fn set_overwrites_previous_value() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("key", json!("old"));
        bindings.set("key", json!("new"));

        assert_eq!(bindings.get("key"), Some(&json!("new")));
    }

    #[test]
    fn set_with_string_into() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("literal", json!("value"));
        assert_eq!(bindings.get("literal"), Some(&json!("value")));
    }

    #[test]
    fn set_null_value() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("nullable", json!(null));
        assert_eq!(bindings.get("nullable"), Some(&json!(null)));
    }

    #[test]
    fn set_array_value() {
        let mut bindings = ResolvedBindings::new();
        let arr = json!([1, 2, 3, "mixed", {"obj": true}]);
        bindings.set("array", arr.clone());
        assert_eq!(bindings.get("array"), Some(&arr));
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::get() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_nonexistent_returns_none() {
        let bindings = ResolvedBindings::new();
        assert_eq!(bindings.get("nonexistent"), None);
    }

    #[test]
    fn get_does_not_resolve_lazy() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "result"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "lazy_bind".to_string(),
            UseEntry::lazy_with_default("task.value", json!("default")),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        // get() should NOT resolve lazy bindings
        assert_eq!(bindings.get("lazy_bind"), None);
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::get_resolved() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_resolved_eager_binding() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "result"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("eager".to_string(), UseEntry::new("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        let result = bindings.get_resolved("eager", &store).unwrap();
        assert_eq!(result, json!("result"));
    }

    #[test]
    fn get_resolved_lazy_binding() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "lazy_result"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        let result = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result, json!("lazy_result"));
    }

    #[test]
    fn get_resolved_nonexistent_binding() {
        let store = DataStore::new();
        let bindings = ResolvedBindings::new();
        let result = bindings.get_resolved("missing", &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-042")); // BindingNotFound
    }

    #[test]
    fn get_resolved_lazy_with_default() {
        let store = DataStore::new();
        // No task in store - should use default

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "lazy_default".to_string(),
            UseEntry::lazy_with_default("missing.path", json!("fallback")),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        let result = bindings.get_resolved("lazy_default", &store).unwrap();
        assert_eq!(result, json!("fallback"));
    }

    #[test]
    fn get_resolved_re_resolves_on_each_call() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"counter": 1}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("task.counter"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        // First call
        let result1 = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result1, json!(1));

        // Update store
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"counter": 2}), Duration::from_secs(1)),
        );

        // Second call - should reflect new value (lazy bindings don't cache)
        let result2 = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result2, json!(2));
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::is_lazy() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn is_lazy_for_eager_binding() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "test"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("eager".to_string(), UseEntry::new("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert!(!bindings.is_lazy("eager"));
    }

    #[test]
    fn is_lazy_for_lazy_binding() {
        let store = DataStore::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert!(bindings.is_lazy("lazy"));
    }

    #[test]
    fn is_lazy_for_nonexistent_binding() {
        let bindings = ResolvedBindings::new();
        assert!(!bindings.is_lazy("missing"));
    }

    #[test]
    fn is_lazy_after_resolution() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "result"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        // Even after calling get_resolved(), the binding is still marked as lazy
        let _ = bindings.get_resolved("lazy", &store);
        assert!(bindings.is_lazy("lazy"));
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::iter() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn iter_empty_bindings() {
        let bindings = ResolvedBindings::new();
        let count = bindings.iter().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn iter_only_resolved_bindings() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": "result"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("eager".to_string(), UseEntry::new("task.value"));
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        // iter() should only return eager bindings, not lazy ones
        let items: Vec<_> = bindings.iter().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "eager");
        assert_eq!(items[0].1, &json!("result"));
    }

    #[test]
    fn iter_multiple_resolved_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("first", json!(1));
        bindings.set("second", json!(2));
        bindings.set("third", json!(3));

        let items: Vec<_> = bindings.iter().collect();
        assert_eq!(items.len(), 3);

        // Check all items are present (order may vary due to FxHashMap)
        let aliases: Vec<_> = items.iter().map(|(alias, _)| *alias).collect();
        assert!(aliases.contains(&"first"));
        assert!(aliases.contains(&"second"));
        assert!(aliases.contains(&"third"));
    }

    #[test]
    fn iter_with_various_value_types() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("str", json!("text"));
        bindings.set("num", json!(42));
        bindings.set("obj", json!({"key": "value"}));
        bindings.set("arr", json!([1, 2, 3]));
        bindings.set("bool", json!(true));

        let items: Vec<_> = bindings.iter().collect();
        assert_eq!(items.len(), 5);

        // Verify all values are accessible
        for (alias, value) in &items {
            match *alias {
                "str" => assert_eq!(*value, &json!("text")),
                "num" => assert_eq!(*value, &json!(42)),
                "obj" => assert_eq!(*value, &json!({"key": "value"})),
                "arr" => assert_eq!(*value, &json!([1, 2, 3])),
                "bool" => assert_eq!(*value, &json!(true)),
                _ => panic!("unexpected alias: {}", alias),
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::to_value() with lazy bindings
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn to_value_with_lazy_bindings() {
        let _store = DataStore::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("eager".to_string(), UseEntry::new("missing"));
        wiring.insert("lazy".to_string(), UseEntry::new_lazy("missing"));

        // The eager binding will use its default (none here, so it should be present in attempt)
        // For this test, we'll skip the eager binding which would fail
        let mut bindings = ResolvedBindings::new();
        bindings.set("eager", json!("eager_value"));

        // Insert a lazy binding manually for testing
        bindings.bindings.insert(
            "lazy".to_string(),
            LazyBinding::Pending {
                path: "task.path".to_string(),
                default: Some(json!("lazy_default")),
            },
        );

        let value = bindings.to_value();
        assert!(value.is_object());

        let obj = value.as_object().unwrap();
        assert_eq!(obj["eager"], json!("eager_value"));

        // Lazy bindings are represented as {__lazy__: true, path: "..."}
        let lazy_marker = &obj["lazy"];
        assert!(lazy_marker.is_object());
        assert_eq!(lazy_marker["__lazy__"], true);
        assert_eq!(lazy_marker["path"], "task.path");
    }

    // ═══════════════════════════════════════════════════════════════
    // Error handling in from_wiring_spec()
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn from_wiring_spec_eager_missing_path() {
        let store = DataStore::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new("nonexistent.path"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
    }

    #[test]
    fn from_wiring_spec_lazy_does_not_fail_on_missing() {
        let store = DataStore::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new_lazy("nonexistent.path"));

        // Lazy bindings don't fail during from_wiring_spec - they fail on get_resolved()
        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_ok());
    }

    #[test]
    fn from_wiring_spec_preserves_all_entries() {
        let store = DataStore::new();
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"a": 1}), Duration::from_secs(1)),
        );
        store.insert(
            Arc::from("task2"),
            TaskResult::success(json!({"b": 2}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("binding1".to_string(), UseEntry::new("task1.a"));
        wiring.insert("binding2".to_string(), UseEntry::new_lazy("task2.b"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        // Both bindings should exist
        assert_eq!(bindings.get("binding1"), Some(&json!(1)));
        assert!(bindings.is_lazy("binding2"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Mixed eager and lazy bindings
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn mixed_eager_and_lazy_workflow() {
        let store = DataStore::new();
        store.insert(
            Arc::from("quick"),
            TaskResult::success(json!({"result": "fast"}), Duration::from_secs(1)),
        );
        store.insert(
            Arc::from("slow"),
            TaskResult::success(json!({"result": "slow_value"}), Duration::from_secs(5)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("quick_bind".to_string(), UseEntry::new("quick.result"));
        wiring.insert("slow_bind".to_string(), UseEntry::new_lazy("slow.result"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        // Eager should be available immediately
        assert_eq!(bindings.get("quick_bind"), Some(&json!("fast")));

        // Lazy should still be pending
        assert!(bindings.is_lazy("slow_bind"));
        assert_eq!(bindings.get("slow_bind"), None);

        // But can be resolved on demand
        let resolved = bindings.get_resolved("slow_bind", &store).unwrap();
        assert_eq!(resolved, json!("slow_value"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases with special values
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn binding_with_empty_string() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("empty", json!(""));
        assert_eq!(bindings.get("empty"), Some(&json!("")));
    }

    #[test]
    fn binding_with_zero() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("zero", json!(0));
        assert_eq!(bindings.get("zero"), Some(&json!(0)));
    }

    #[test]
    fn binding_with_false() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("falsy", json!(false));
        assert_eq!(bindings.get("falsy"), Some(&json!(false)));
    }

    #[test]
    fn binding_with_empty_array() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("empty_arr", json!([]));
        assert_eq!(bindings.get("empty_arr"), Some(&json!([])));
    }

    #[test]
    fn binding_with_empty_object() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("empty_obj", json!({}));
        assert_eq!(bindings.get("empty_obj"), Some(&json!({})));
    }
}
