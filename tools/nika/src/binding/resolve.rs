//! Resolved Bindings - runtime value resolution (v0.1)
//!
//! ResolvedBindings holds resolved values from `use:` blocks for template resolution.
//! Eliminates intermediate storage - values are resolved once and used inline.
//!
//! Unified syntax: `alias: task.path [?? default]`
//!
//! Uses FxHashMap for faster hashing (consistent with FlowGraph).

use rustc_hash::FxHashMap;
use serde_json::Value;

use crate::error::NikaError;
use crate::store::DataStore;
use crate::util::jsonpath;

use super::entry::{UseEntry, WiringSpec};

/// Resolved bindings from use: block (alias -> value)
///
/// Uses FxHashMap for faster hashing on small string keys.
#[derive(Debug, Clone, Default)]
pub struct ResolvedBindings {
    /// Resolved alias -> value mappings from use: block
    resolved: FxHashMap<String, Value>,
}

/// Deprecated alias - use ResolvedBindings instead
#[deprecated(since = "0.3.0", note = "use ResolvedBindings instead")]
pub type UseBindings = ResolvedBindings;

impl ResolvedBindings {
    /// Create empty bindings
    pub fn new() -> Self {
        Self::default()
    }

    /// Build bindings from use: wiring by resolving paths from datastore
    ///
    /// Unified resolution for the new syntax: `task.path [?? default]`
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
            let value = resolve_entry(entry, alias, datastore)?;
            bindings.set(alias, value);
        }

        Ok(bindings)
    }

    /// Deprecated alias - use from_wiring_spec instead
    #[deprecated(since = "0.3.0", note = "use from_wiring_spec instead")]
    pub fn from_use_wiring(
        use_wiring: Option<&WiringSpec>,
        datastore: &DataStore,
    ) -> Result<Self, NikaError> {
        Self::from_wiring_spec(use_wiring, datastore)
    }

    /// Set a resolved value
    pub fn set(&mut self, alias: impl Into<String>, value: Value) {
        self.resolved.insert(alias.into(), value);
    }

    /// Get a resolved value
    pub fn get(&self, alias: &str) -> Option<&Value> {
        self.resolved.get(alias)
    }

    /// Check if context has any resolved values
    #[allow(dead_code)] // Used in tests
    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty()
    }

    /// Serialize context to JSON Value for event logging
    ///
    /// Returns the full resolved inputs as a JSON object.
    /// Used by EventLog for TaskStarted events (inputs field).
    pub fn to_value(&self) -> Value {
        serde_json::to_value(&self.resolved).unwrap_or(Value::Null)
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
}
