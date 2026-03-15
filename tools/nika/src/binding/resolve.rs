//! Resolved Bindings - runtime value resolution
//!
//! ResolvedBindings holds resolved values from `use:` / `with:` blocks for template resolution.
//! Supports both eager (immediate) and lazy (deferred) resolution.
//!
//! ## Old system (use:) — WiringSpec + UseEntry
//!
//! Unified syntax: `alias: task.path [?? default]`
//! Extended syntax: `alias: {path: task.path, lazy: true}`
//!
//! ## New system (with:) — WithSpec + WithEntry
//!
//! Rich typed paths with transforms:
//! ```yaml
//! with:
//!   summary: $step1.abstract | lower | trim ?? "No abstract"
//!   data:
//!     from: $step1.data
//!     type: object
//!     transform: sort_keys
//! ```
//!
//! Data flow:
//! ```text
//! WithEntry.source (BindingPath)
//!     ↓ dispatch by BindingSource
//!     ├── Task(id) → datastore.get_output(id) + navigate PathSegments
//!     ├── Input(sub) → datastore.resolve_input_path("inputs.{sub}")
//!     ├── Context(sub) → datastore.get_context_file/session
//!     ├── Env(var) → std::env::var(var)
//!     └── LoopVar(_) → error (should be pre-resolved)
//!     ↓
//! Apply WithEntry.transform (TransformExpr pipeline)
//!     ↓
//! Apply WithEntry.default (if value is null/missing)
//!     ↓
//! Validate WithEntry.binding_type (BindingType constraint)
//!     ↓
//! LazyBinding::Resolved(value) or error
//! ```
//!
//! Uses FxHashMap for faster hashing (consistent with Dag).

use rustc_hash::FxHashMap;
use serde_json::Value;

use super::jsonpath;
use crate::error::NikaError;
use crate::store::RunContext;

use super::entry::{UseEntry, WiringSpec, WithEntry, WithSpec};
use super::transform::TransformExpr;
use super::types::{BindingPath, BindingSource, BindingType, PathSegment};

/// Lazy binding state - either resolved or pending
///
/// Pending now stores `BindingPath` + optional `TransformExpr` + optional default
/// instead of raw `String` path. This enables typed source dispatch and transform
/// application during lazy resolution.
#[derive(Debug, Clone)]
pub enum LazyBinding {
    /// Already resolved to a concrete value (eager bindings)
    Resolved(Value),
    /// Pending resolution — stores raw path string
    Pending {
        path: String,
        default: Option<Value>,
    },
    /// Pending resolution (new system) — stores typed BindingPath + transforms
    PendingWithEntry {
        source: BindingPath,
        binding_type: BindingType,
        default: Option<Value>,
        transform: Option<TransformExpr>,
    },
}

impl LazyBinding {
    /// Check if this binding is pending resolution
    pub fn is_pending(&self) -> bool {
        matches!(
            self,
            LazyBinding::Pending { .. } | LazyBinding::PendingWithEntry { .. }
        )
    }

    /// Get the value if already resolved
    pub fn get_value(&self) -> Option<&Value> {
        match self {
            LazyBinding::Resolved(v) => Some(v),
            LazyBinding::Pending { .. } | LazyBinding::PendingWithEntry { .. } => None,
        }
    }
}

/// Resolved bindings from use:/with: block (alias -> value or pending)
///
/// Uses FxHashMap for faster hashing on small string keys.
/// Supports both eager and lazy bindings.
/// Also provides `from_with_spec()` for the typed binding system.
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

    // ═══════════════════════════════════════════════════════════════
    // Old system: from_wiring_spec (use: block)
    // ═══════════════════════════════════════════════════════════════

    /// Build bindings from use: wiring by resolving paths from datastore
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
        datastore: &RunContext,
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

    // ═══════════════════════════════════════════════════════════════
    // New system: from_with_spec (with: block)
    // ═══════════════════════════════════════════════════════════════

    /// Build bindings from with: spec by resolving typed BindingPaths
    ///
    /// Resolution order per entry:
    /// 1. Dispatch by BindingSource (Task/Input/Context/Env)
    /// 2. Navigate PathSegments for nested value access
    /// 3. Apply TransformExpr pipeline (if present)
    /// 4. Apply default value (if result is null/missing)
    /// 5. Validate BindingType constraint
    ///
    /// Lazy bindings are stored as PendingWithEntry for later resolution.
    pub fn from_with_spec(
        with_spec: Option<&WithSpec>,
        datastore: &RunContext,
    ) -> Result<Self, NikaError> {
        let Some(spec) = with_spec else {
            return Ok(Self::new());
        };

        let mut bindings = Self::new();

        for (alias, entry) in spec {
            if entry.is_lazy() {
                bindings.bindings.insert(
                    alias.clone(),
                    LazyBinding::PendingWithEntry {
                        source: entry.source.clone(),
                        binding_type: entry.binding_type,
                        default: entry.default.clone(),
                        transform: entry.transform.clone(),
                    },
                );
            } else {
                let value = resolve_with_entry(entry, alias, datastore)?;
                bindings
                    .bindings
                    .insert(alias.clone(), LazyBinding::Resolved(value));
            }
        }

        Ok(bindings)
    }

    // ═══════════════════════════════════════════════════════════════
    // Common API
    // ═══════════════════════════════════════════════════════════════

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

    /// Get a resolved value, resolving lazy bindings on demand
    ///
    /// For eager bindings, returns the pre-resolved value.
    /// For lazy bindings, resolves from datastore on first call.
    ///
    /// Note: This doesn't cache the resolution - each call re-resolves.
    /// This is intentional to support changing datastore values.
    pub fn get_resolved(&self, alias: &str, datastore: &RunContext) -> Result<Value, NikaError> {
        match self.bindings.get(alias) {
            Some(LazyBinding::Resolved(value)) => Ok(value.clone()),
            Some(LazyBinding::Pending { path, default }) => {
                // Old system: resolve via UseEntry
                let entry = UseEntry {
                    path: path.clone(),
                    default: default.clone(),
                    lazy: true,
                };
                resolve_entry(&entry, alias, datastore)
            }
            Some(LazyBinding::PendingWithEntry {
                source,
                binding_type,
                default,
                transform,
            }) => {
                // New system: resolve via WithEntry
                let entry = WithEntry {
                    source: source.clone(),
                    binding_type: *binding_type,
                    default: default.clone(),
                    lazy: true,
                    transform: transform.clone(),
                };
                resolve_with_entry(&entry, alias, datastore)
            }
            None => Err(NikaError::BindingNotFound {
                alias: alias.to_string(),
            }),
        }
    }

    /// Check if a binding is lazy (pending resolution)
    pub fn is_lazy(&self, alias: &str) -> bool {
        self.bindings
            .get(alias)
            .map(|b| b.is_pending())
            .unwrap_or(false)
    }

    /// Check if context has any bindings
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
    /// Lazy bindings that haven't been resolved are represented as marker objects.
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
                LazyBinding::PendingWithEntry {
                    source, default: _, ..
                } => {
                    // Represent pending with typed path
                    map.insert(
                        alias.clone(),
                        serde_json::json!({"__lazy__": true, "path": source.to_string()}),
                    );
                }
            }
        }
        Value::Object(map)
    }
}

// ═══════════════════════════════════════════════════════════════
// Old resolution: UseEntry (use: block)
// ═══════════════════════════════════════════════════════════════

/// Resolve a single UseEntry to a Value
///
/// Unified resolution logic:
/// 1. Check for inputs.* path (workflow inputs support)
/// 2. Extract task_id from path (first segment)
/// 3. Get task output from datastore
/// 4. Resolve remaining path within output
/// 5. Apply default if value is null/missing
fn resolve_entry(
    entry: &UseEntry,
    alias: &str,
    datastore: &RunContext,
) -> Result<Value, NikaError> {
    let path = &entry.path;

    // Check for inputs.* path first
    if path.starts_with("inputs.") {
        let value = datastore.resolve_input_path(path);
        return match value {
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
        };
    }

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

// ═══════════════════════════════════════════════════════════════
// New resolution: WithEntry (with: block)
// ═══════════════════════════════════════════════════════════════

/// Resolve a single WithEntry to a Value using typed BindingPath dispatch
///
/// Resolution pipeline:
/// 1. Dispatch by BindingSource to get raw value
/// 2. Navigate PathSegments for nested access
/// 3. Apply transform pipeline (if present)
/// 4. Apply default (if value is null/missing, AFTER transforms)
/// 5. Validate BindingType constraint
fn resolve_with_entry(
    entry: &WithEntry,
    alias: &str,
    datastore: &RunContext,
) -> Result<Value, NikaError> {
    let path_str = entry.source.to_string();

    // Step 1+2: Dispatch by source and navigate segments
    let raw_value = resolve_binding_path(&entry.source, alias, datastore)?;

    // Step 3: Apply transforms
    let transformed = match (&raw_value, &entry.transform) {
        (Some(v), Some(expr)) if !v.is_null() => {
            Some(expr.apply(v).map_err(|e| NikaError::PathNotFound {
                path: format!("{} (transform error: {})", path_str, e),
            })?)
        }
        _ => raw_value,
    };

    // Step 4: Apply default if null/missing
    let value = match transformed {
        Some(v) if !v.is_null() => v,
        Some(_null) => {
            // Value is null — use default or error
            match &entry.default {
                Some(d) => d.clone(),
                None => {
                    return Err(NikaError::NullValue {
                        path: path_str,
                        alias: alias.to_string(),
                    });
                }
            }
        }
        None => {
            // Value not found — use default or error
            match &entry.default {
                Some(d) => d.clone(),
                None => {
                    return Err(NikaError::PathNotFound { path: path_str });
                }
            }
        }
    };

    // Step 5: Validate BindingType constraint
    validate_binding_type(&value, entry.binding_type, alias, &path_str)?;

    Ok(value)
}

/// Dispatch resolution by BindingSource variant
///
/// Returns the raw value before transforms/defaults are applied.
/// Returns `Ok(None)` if the source exists but the specific path is missing.
fn resolve_binding_path(
    binding_path: &BindingPath,
    alias: &str,
    datastore: &RunContext,
) -> Result<Option<Value>, NikaError> {
    match &binding_path.source {
        BindingSource::Task(task_id) => {
            let output = match datastore.get_output(task_id) {
                Some(o) => o,
                None => return Ok(None),
            };

            // Navigate path segments through the value
            navigate_segments(&output, &binding_path.segments).map(|opt| opt.cloned())
        }

        BindingSource::Input(sub_path) => {
            // RunContext.resolve_input_path expects "inputs.{sub_path}" format
            let full_path = format!("inputs.{}", sub_path);
            Ok(datastore.resolve_input_path(&full_path))
        }

        BindingSource::Context(sub_path) => {
            let sub = sub_path.as_ref();
            if sub == "session" {
                // $context.session → datastore.get_context_session()
                Ok(datastore.get_context_session())
            } else if let Some(file_alias) = sub.strip_prefix("files.") {
                // $context.files.brand → datastore.get_context_file("brand")
                Ok(datastore.get_context_file(file_alias))
            } else {
                // Unrecognized context sub-path
                Ok(None)
            }
        }

        BindingSource::Env(var_name) => match std::env::var(var_name.as_ref()) {
            Ok(val) => Ok(Some(Value::String(val))),
            Err(_) => Ok(None),
        },

        BindingSource::LoopVar(name) => {
            // Loop variables should be pre-resolved by the executor before reaching here.
            // If we get here, it means the loop variable wasn't set.
            Err(NikaError::BindingNotFound {
                alias: format!("{} (loop variable '{}' not pre-resolved)", alias, name),
            })
        }
    }
}

/// Navigate a sequence of PathSegments through a JSON value
///
/// Returns `Ok(None)` if a segment doesn't match (missing field, out-of-bounds index).
fn navigate_segments<'a>(
    value: &'a Value,
    segments: &[PathSegment],
) -> Result<Option<&'a Value>, NikaError> {
    if segments.is_empty() {
        return Ok(Some(value));
    }

    let mut current = value;
    for segment in segments {
        match segment {
            PathSegment::Field(name) => match current {
                Value::Object(map) => match map.get(name.as_ref()) {
                    Some(v) => current = v,
                    None => return Ok(None),
                },
                _ => return Ok(None),
            },
            PathSegment::Index(idx) => match current {
                Value::Array(arr) => match arr.get(*idx) {
                    Some(v) => current = v,
                    None => return Ok(None),
                },
                _ => return Ok(None),
            },
        }
    }

    Ok(Some(current))
}

/// Validate that a value matches the expected BindingType constraint
///
/// BindingType::Any always passes. Other types check the JSON value variant.
fn validate_binding_type(
    value: &Value,
    binding_type: BindingType,
    alias: &str,
    path: &str,
) -> Result<(), NikaError> {
    let matches = match binding_type {
        BindingType::Any => true,
        BindingType::String => value.is_string(),
        BindingType::Number => value.is_number(),
        BindingType::Integer => value.is_i64() || value.is_u64(),
        BindingType::Boolean => value.is_boolean(),
        BindingType::Array => value.is_array(),
        BindingType::Object => value.is_object(),
    };

    if !matches {
        return Err(NikaError::BindingTypeMismatch {
            expected: binding_type.to_string(),
            actual: json_type_name(value).to_string(),
            path: format!("{} (alias: {})", path, alias),
        });
    }

    Ok(())
}

/// Get a human-readable type name for a JSON value
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::types::BindingPath;
    use crate::store::TaskResult;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    // ═══════════════════════════════════════════════════════════════
    // Basic tests (common API)
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
        let store = RunContext::new();
        let bindings = ResolvedBindings::from_wiring_spec(None, &store).unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn from_with_spec_none() {
        let store = RunContext::new();
        let bindings = ResolvedBindings::from_with_spec(None, &store).unwrap();
        assert!(bindings.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // Old system: from_wiring_spec tests (use: block)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_simple_path() {
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();

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
        let store = RunContext::new();
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
        let store = RunContext::new();

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
        let store = RunContext::new();

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
        let store = RunContext::new();

        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new("missing.path"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052"));
    }

    #[test]
    fn resolve_null_strict_error() {
        let store = RunContext::new();
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
        let store = RunContext::new();
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

    #[test]
    fn lazy_binding_pending_with_entry_is_pending() {
        let binding = LazyBinding::PendingWithEntry {
            source: BindingPath::parse("$step1.data").unwrap(),
            binding_type: BindingType::Any,
            default: None,
            transform: None,
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
    fn lazy_binding_get_value_pending_with_entry() {
        let binding = LazyBinding::PendingWithEntry {
            source: BindingPath::parse("$step1").unwrap(),
            binding_type: BindingType::Any,
            default: None,
            transform: None,
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
        let bindings = ResolvedBindings::new();
        let result = bindings.get_resolved("missing", &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-042")); // BindingNotFound
    }

    #[test]
    fn get_resolved_lazy_with_default() {
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
        let store = RunContext::new();
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
    // to_value() with lazy bindings
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn to_value_with_lazy_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("eager", json!("eager_value"));

        // Insert old-style lazy binding manually
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

    #[test]
    fn to_value_with_pending_with_entry() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("eager", json!("eager_value"));

        // Insert new-style lazy binding
        bindings.bindings.insert(
            "lazy_new".to_string(),
            LazyBinding::PendingWithEntry {
                source: BindingPath::parse("$step1.data").unwrap(),
                binding_type: BindingType::Object,
                default: None,
                transform: None,
            },
        );

        let value = bindings.to_value();
        let obj = value.as_object().unwrap();

        let lazy_marker = &obj["lazy_new"];
        assert_eq!(lazy_marker["__lazy__"], true);
        assert_eq!(lazy_marker["path"], "$step1.data");
    }

    // ═══════════════════════════════════════════════════════════════
    // Error handling in from_wiring_spec()
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn from_wiring_spec_eager_missing_path() {
        let store = RunContext::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new("nonexistent.path"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
    }

    #[test]
    fn from_wiring_spec_lazy_does_not_fail_on_missing() {
        let store = RunContext::new();
        let mut wiring = WiringSpec::default();
        wiring.insert("x".to_string(), UseEntry::new_lazy("nonexistent.path"));

        // Lazy bindings don't fail during from_wiring_spec - they fail on get_resolved()
        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_ok());
    }

    #[test]
    fn from_wiring_spec_preserves_all_entries() {
        let store = RunContext::new();
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
        let store = RunContext::new();
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

    // ═══════════════════════════════════════════════════════════════
    // inputs.* binding support
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_inputs_simple() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI trends 2025"
            }),
        );
        store.set_inputs(inputs);

        let mut wiring = WiringSpec::default();
        wiring.insert("topic_val".to_string(), UseEntry::new("inputs.topic"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("topic_val"), Some(&json!("AI trends 2025")));
    }

    #[test]
    fn resolve_inputs_nested_field() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

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

        let mut wiring = WiringSpec::default();
        wiring.insert("theme".to_string(), UseEntry::new("inputs.config.theme"));
        wiring.insert(
            "deep".to_string(),
            UseEntry::new("inputs.config.nested.deep"),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("theme"), Some(&json!("dark")));
        assert_eq!(bindings.get("deep"), Some(&json!("value")));
    }

    #[test]
    fn resolve_inputs_with_default_on_missing() {
        let store = RunContext::new();

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "fallback".to_string(),
            UseEntry::with_default("inputs.missing", json!("default_value")),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("fallback"), Some(&json!("default_value")));
    }

    #[test]
    fn resolve_inputs_missing_no_default() {
        let store = RunContext::new();

        let mut wiring = WiringSpec::default();
        wiring.insert("missing".to_string(), UseEntry::new("inputs.missing"));

        let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052")); // PathNotFound
    }

    #[test]
    fn resolve_inputs_lazy_binding() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "lazy_input".to_string(),
            json!({
                "type": "string",
                "default": "lazy_value"
            }),
        );
        store.set_inputs(inputs);

        let mut wiring = WiringSpec::default();
        wiring.insert(
            "lazy_alias".to_string(),
            UseEntry::new_lazy("inputs.lazy_input"),
        );

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        assert!(bindings.is_lazy("lazy_alias"));
        assert_eq!(bindings.get("lazy_alias"), None);

        let resolved = bindings.get_resolved("lazy_alias", &store).unwrap();
        assert_eq!(resolved, json!("lazy_value"));
    }

    #[test]
    fn resolve_inputs_mixed_with_task_outputs() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI"
            }),
        );
        store.set_inputs(inputs);

        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"result": "generated"}), Duration::from_secs(1)),
        );

        let mut wiring = WiringSpec::default();
        wiring.insert("from_input".to_string(), UseEntry::new("inputs.topic"));
        wiring.insert("from_task".to_string(), UseEntry::new("step1.result"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

        assert_eq!(bindings.get("from_input"), Some(&json!("AI")));
        assert_eq!(bindings.get("from_task"), Some(&json!("generated")));
    }

    #[test]
    fn resolve_inputs_array_value() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "items".to_string(),
            json!({
                "type": "array",
                "default": ["a", "b", "c"]
            }),
        );
        store.set_inputs(inputs);

        let mut wiring = WiringSpec::default();
        wiring.insert("all_items".to_string(), UseEntry::new("inputs.items"));

        let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
        assert_eq!(bindings.get("all_items"), Some(&json!(["a", "b", "c"])));
    }

    // ═══════════════════════════════════════════════════════════════
    // from_with_spec tests (with: block)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_task_simple() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"title": "Hello"}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        spec.insert(
            "title".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.title").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("title"), Some(&json!("Hello")));
    }

    #[test]
    fn with_spec_task_entire_output() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"a": 1, "b": 2}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        spec.insert(
            "data".to_string(),
            WithEntry::simple(BindingPath::parse("$step1").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("data"), Some(&json!({"a": 1, "b": 2})));
    }

    #[test]
    fn with_spec_task_nested_path() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(
                json!({"data": {"items": [{"name": "first"}]}}),
                Duration::from_secs(1),
            ),
        );

        let mut spec = WithSpec::default();
        spec.insert(
            "first_name".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.data.items[0].name").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("first_name"), Some(&json!("first")));
    }

    #[test]
    fn with_spec_task_with_default_on_missing() {
        let store = RunContext::new();
        // No step1 task in store

        let mut spec = WithSpec::default();
        spec.insert(
            "result".to_string(),
            WithEntry::with_default(
                BindingPath::parse("$step1.data").unwrap(),
                json!("fallback"),
            ),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("result"), Some(&json!("fallback")));
    }

    #[test]
    fn with_spec_task_with_default_on_null() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"data": null}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        spec.insert(
            "result".to_string(),
            WithEntry::with_default(
                BindingPath::parse("$step1.data").unwrap(),
                json!("fallback"),
            ),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("result"), Some(&json!("fallback")));
    }

    #[test]
    fn with_spec_task_missing_no_default_error() {
        let store = RunContext::new();

        let mut spec = WithSpec::default();
        spec.insert(
            "result".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.data").unwrap()),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("NIKA-052")); // PathNotFound
    }

    #[test]
    fn with_spec_task_null_no_default_error() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"data": null}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        spec.insert(
            "result".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.data").unwrap()),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("NIKA-072")); // NullValue
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Input source tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_input_simple() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({"type": "string", "default": "AI trends"}),
        );
        store.set_inputs(inputs);

        let mut spec = WithSpec::default();
        spec.insert(
            "topic".to_string(),
            WithEntry::simple(BindingPath::parse("$inputs.topic").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("topic"), Some(&json!("AI trends")));
    }

    #[test]
    fn with_spec_input_nested() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "config".to_string(),
            json!({"type": "object", "default": {"theme": "dark", "nested": {"deep": "val"}}}),
        );
        store.set_inputs(inputs);

        let mut spec = WithSpec::default();
        spec.insert(
            "theme".to_string(),
            WithEntry::simple(BindingPath::parse("$inputs.config.theme").unwrap()),
        );
        spec.insert(
            "deep".to_string(),
            WithEntry::simple(BindingPath::parse("$inputs.config.nested.deep").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("theme"), Some(&json!("dark")));
        assert_eq!(bindings.get("deep"), Some(&json!("val")));
    }

    #[test]
    fn with_spec_input_missing_with_default() {
        let store = RunContext::new();
        // No inputs set

        let mut spec = WithSpec::default();
        spec.insert(
            "fallback".to_string(),
            WithEntry::with_default(
                BindingPath::parse("$inputs.missing").unwrap(),
                json!("default_val"),
            ),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("fallback"), Some(&json!("default_val")));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Env source tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_env_existing_var() {
        // Use a known env var
        std::env::set_var("NIKA_TEST_VAR_8A", "test_value_8a");

        let store = RunContext::new();
        let mut spec = WithSpec::default();
        spec.insert(
            "my_var".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_VAR_8A").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("my_var"), Some(&json!("test_value_8a")));

        std::env::remove_var("NIKA_TEST_VAR_8A");
    }

    #[test]
    fn with_spec_env_missing_with_default() {
        let store = RunContext::new();
        let mut spec = WithSpec::default();
        spec.insert(
            "missing_env".to_string(),
            WithEntry::with_default(
                BindingPath::parse("$env.NIKA_NONEXISTENT_VAR_XYZ").unwrap(),
                json!("fallback_env"),
            ),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("missing_env"), Some(&json!("fallback_env")));
    }

    #[test]
    fn with_spec_env_missing_no_default_error() {
        let store = RunContext::new();
        let mut spec = WithSpec::default();
        spec.insert(
            "missing".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_NONEXISTENT_VAR_ABC").unwrap()),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Context source tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_context_file() {
        use crate::runtime::LoadedContext;
        let store = RunContext::new();
        let mut ctx = LoadedContext::new();
        ctx.files
            .insert("brand".to_string(), json!("Brand Guidelines v2"));
        store.set_context(ctx);

        let mut spec = WithSpec::default();
        spec.insert(
            "brand".to_string(),
            WithEntry::simple(BindingPath::parse("$context.files.brand").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("brand"), Some(&json!("Brand Guidelines v2")));
    }

    #[test]
    fn with_spec_context_session() {
        use crate::runtime::LoadedContext;
        let store = RunContext::new();
        let mut ctx = LoadedContext::new();
        ctx.session = Some(json!({"last_run": "2025-01-01"}));
        store.set_context(ctx);

        let mut spec = WithSpec::default();
        spec.insert(
            "session".to_string(),
            WithEntry::simple(BindingPath::parse("$context.session").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(
            bindings.get("session"),
            Some(&json!({"last_run": "2025-01-01"}))
        );
    }

    #[test]
    fn with_spec_context_missing_with_default() {
        let store = RunContext::new();
        // No context files loaded

        let mut spec = WithSpec::default();
        spec.insert(
            "brand".to_string(),
            WithEntry::with_default(
                BindingPath::parse("$context.files.brand").unwrap(),
                json!("no brand"),
            ),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("brand"), Some(&json!("no brand")));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Lazy bindings
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_lazy_does_not_fail_on_missing() {
        let store = RunContext::new();

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.data").unwrap());
        entry.lazy = true;
        spec.insert("lazy_val".to_string(), entry);

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_ok());
        let bindings = result.unwrap();
        assert!(bindings.is_lazy("lazy_val"));
        assert_eq!(bindings.get("lazy_val"), None);
    }

    #[test]
    fn with_spec_lazy_resolve_on_demand() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"data": "deferred"}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.data").unwrap());
        entry.lazy = true;
        spec.insert("lazy_val".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert!(bindings.is_lazy("lazy_val"));

        let resolved = bindings.get_resolved("lazy_val", &store).unwrap();
        assert_eq!(resolved, json!("deferred"));
    }

    #[test]
    fn with_spec_lazy_re_resolves() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"counter": 1}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.counter").unwrap());
        entry.lazy = true;
        spec.insert("counter".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        let v1 = bindings.get_resolved("counter", &store).unwrap();
        assert_eq!(v1, json!(1));

        // Update store
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"counter": 42}), Duration::from_secs(1)),
        );

        let v2 = bindings.get_resolved("counter", &store).unwrap();
        assert_eq!(v2, json!(42));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Transform pipeline tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_with_transform() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"name": "  Hello World  "}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.name").unwrap());
        entry.transform = Some(TransformExpr::parse("trim | upper").unwrap());
        spec.insert("name".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("name"), Some(&json!("HELLO WORLD")));
    }

    #[test]
    fn with_spec_transform_with_default_on_null() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"name": null}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry =
            WithEntry::with_default(BindingPath::parse("$step1.name").unwrap(), json!("DEFAULT"));
        entry.transform = Some(TransformExpr::parse("upper").unwrap());
        spec.insert("name".to_string(), entry);

        // Null goes through transform pipeline as-is (transform skipped for null),
        // then default kicks in
        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("name"), Some(&json!("DEFAULT")));
    }

    #[test]
    fn with_spec_transform_chain() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"items": [3, 1, 4, 1, 5, 9]}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.items").unwrap());
        entry.transform = Some(TransformExpr::parse("sort | unique | length").unwrap());
        spec.insert("unique_count".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        // [3,1,4,1,5,9] → sort → [1,1,3,4,5,9] → unique → [1,3,4,5,9] → length → 5
        assert_eq!(bindings.get("unique_count"), Some(&json!(5)));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: BindingType validation tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_type_string_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"name": "text"}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.name").unwrap());
        entry.binding_type = BindingType::String;
        spec.insert("name".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("name"), Some(&json!("text")));
    }

    #[test]
    fn with_spec_type_string_invalid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"count": 42}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.count").unwrap());
        entry.binding_type = BindingType::String;
        spec.insert("count".to_string(), entry);

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("NIKA-043")); // BindingTypeMismatch
    }

    #[test]
    fn with_spec_type_array_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"items": [1, 2, 3]}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.items").unwrap());
        entry.binding_type = BindingType::Array;
        spec.insert("items".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("items"), Some(&json!([1, 2, 3])));
    }

    #[test]
    fn with_spec_type_any_accepts_all() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"val": [1, "mixed"]}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.val").unwrap());
        entry.binding_type = BindingType::Any;
        spec.insert("val".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("val"), Some(&json!([1, "mixed"])));
    }

    #[test]
    fn with_spec_type_object_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"cfg": {"debug": true}}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.cfg").unwrap());
        entry.binding_type = BindingType::Object;
        spec.insert("cfg".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("cfg"), Some(&json!({"debug": true})));
    }

    #[test]
    fn with_spec_type_number_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"temp": 25.5}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.temp").unwrap());
        entry.binding_type = BindingType::Number;
        spec.insert("temp".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("temp"), Some(&json!(25.5)));
    }

    #[test]
    fn with_spec_type_integer_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"count": 42}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.count").unwrap());
        entry.binding_type = BindingType::Integer;
        spec.insert("count".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("count"), Some(&json!(42)));
    }

    #[test]
    fn with_spec_type_integer_rejects_float() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"val": 3.14}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.val").unwrap());
        entry.binding_type = BindingType::Integer;
        spec.insert("val".to_string(), entry);

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-043"));
    }

    #[test]
    fn with_spec_type_boolean_valid() {
        let store = RunContext::new();
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"flag": true}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.flag").unwrap());
        entry.binding_type = BindingType::Boolean;
        spec.insert("flag".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("flag"), Some(&json!(true)));
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Mixed source types
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_mixed_sources() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new();

        // Task output
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"result": "task_val"}), Duration::from_secs(1)),
        );

        // Inputs
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({"type": "string", "default": "AI"}),
        );
        store.set_inputs(inputs);

        // Context file
        {
            use crate::runtime::LoadedContext;
            let mut ctx = LoadedContext::new();
            ctx.files.insert("brand".to_string(), json!("Brand Text"));
            store.set_context(ctx);
        }

        // Env
        std::env::set_var("NIKA_TEST_MIXED_8A", "env_val");

        let mut spec = WithSpec::default();
        spec.insert(
            "from_task".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.result").unwrap()),
        );
        spec.insert(
            "from_input".to_string(),
            WithEntry::simple(BindingPath::parse("$inputs.topic").unwrap()),
        );
        spec.insert(
            "from_context".to_string(),
            WithEntry::simple(BindingPath::parse("$context.files.brand").unwrap()),
        );
        spec.insert(
            "from_env".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_MIXED_8A").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        assert_eq!(bindings.get("from_task"), Some(&json!("task_val")));
        assert_eq!(bindings.get("from_input"), Some(&json!("AI")));
        assert_eq!(bindings.get("from_context"), Some(&json!("Brand Text")));
        assert_eq!(bindings.get("from_env"), Some(&json!("env_val")));

        std::env::remove_var("NIKA_TEST_MIXED_8A");
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: LoopVar error
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_loop_var_errors() {
        let store = RunContext::new();

        let mut spec = WithSpec::default();
        spec.insert(
            "item".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::LoopVar(Arc::from("item")),
                segments: vec![],
            }),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("loop variable"));
    }

    // ═══════════════════════════════════════════════════════════════
    // navigate_segments tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn navigate_segments_empty() {
        let value = json!({"hello": "world"});
        let result = navigate_segments(&value, &[]).unwrap().cloned();
        assert_eq!(result, Some(json!({"hello": "world"})));
    }

    #[test]
    fn navigate_segments_field() {
        let value = json!({"name": "Nika"});
        let segments = vec![PathSegment::Field(Arc::from("name"))];
        let result = navigate_segments(&value, &segments).unwrap().cloned();
        assert_eq!(result, Some(json!("Nika")));
    }

    #[test]
    fn navigate_segments_deep_field() {
        let value = json!({"a": {"b": {"c": 42}}});
        let segments = vec![
            PathSegment::Field(Arc::from("a")),
            PathSegment::Field(Arc::from("b")),
            PathSegment::Field(Arc::from("c")),
        ];
        let result = navigate_segments(&value, &segments).unwrap().cloned();
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn navigate_segments_array_index() {
        let value = json!({"items": ["a", "b", "c"]});
        let segments = vec![
            PathSegment::Field(Arc::from("items")),
            PathSegment::Index(1),
        ];
        let result = navigate_segments(&value, &segments).unwrap().cloned();
        assert_eq!(result, Some(json!("b")));
    }

    #[test]
    fn navigate_segments_mixed() {
        let value = json!({"data": [{"name": "first"}, {"name": "second"}]});
        let segments = vec![
            PathSegment::Field(Arc::from("data")),
            PathSegment::Index(1),
            PathSegment::Field(Arc::from("name")),
        ];
        let result = navigate_segments(&value, &segments).unwrap().cloned();
        assert_eq!(result, Some(json!("second")));
    }

    #[test]
    fn navigate_segments_missing_field() {
        let value = json!({"a": 1});
        let segments = vec![PathSegment::Field(Arc::from("missing"))];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn navigate_segments_out_of_bounds() {
        let value = json!([1, 2, 3]);
        let segments = vec![PathSegment::Index(10)];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn navigate_segments_field_on_non_object() {
        let value = json!("string_value");
        let segments = vec![PathSegment::Field(Arc::from("field"))];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn navigate_segments_index_on_non_array() {
        let value = json!({"key": "val"});
        let segments = vec![PathSegment::Index(0)];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, None);
    }

    // ═══════════════════════════════════════════════════════════════
    // validate_binding_type tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_type_any_accepts_all() {
        validate_binding_type(&json!("str"), BindingType::Any, "a", "p").unwrap();
        validate_binding_type(&json!(42), BindingType::Any, "a", "p").unwrap();
        validate_binding_type(&json!(true), BindingType::Any, "a", "p").unwrap();
        validate_binding_type(&json!([]), BindingType::Any, "a", "p").unwrap();
        validate_binding_type(&json!({}), BindingType::Any, "a", "p").unwrap();
        validate_binding_type(&json!(null), BindingType::Any, "a", "p").unwrap();
    }

    #[test]
    fn validate_type_string_rejects_number() {
        let result = validate_binding_type(&json!(42), BindingType::String, "a", "p");
        assert!(result.is_err());
    }

    #[test]
    fn validate_type_number_accepts_int_and_float() {
        validate_binding_type(&json!(42), BindingType::Number, "a", "p").unwrap();
        validate_binding_type(&json!(3.14), BindingType::Number, "a", "p").unwrap();
    }

    #[test]
    fn validate_type_integer_rejects_float() {
        let result = validate_binding_type(&json!(3.14), BindingType::Integer, "a", "p");
        assert!(result.is_err());
    }

    #[test]
    fn validate_type_boolean_rejects_string() {
        let result = validate_binding_type(&json!("true"), BindingType::Boolean, "a", "p");
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // json_type_name tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn json_type_names() {
        assert_eq!(json_type_name(&json!(null)), "null");
        assert_eq!(json_type_name(&json!(true)), "boolean");
        assert_eq!(json_type_name(&json!(42)), "number");
        assert_eq!(json_type_name(&json!("str")), "string");
        assert_eq!(json_type_name(&json!([])), "array");
        assert_eq!(json_type_name(&json!({})), "object");
    }
}
