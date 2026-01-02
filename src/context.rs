//! Task execution context (v0.1)
//!
//! TaskContext holds resolved values from `use:` blocks for template resolution.
//! Eliminates intermediate storage - values are resolved once and used inline.

use serde_json::Value;
use std::collections::HashMap;

use crate::datastore::DataStore;
use crate::error::NikaError;
use crate::jsonpath;
use crate::use_block::{UseAdvanced, UseBlock, UseEntry};

/// Task execution context with resolved inputs
#[derive(Debug, Clone, Default)]
pub struct TaskContext {
    /// Resolved alias → value mappings from use: block
    resolved: HashMap<String, Value>,
}

impl TaskContext {
    /// Create empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Build context from use: block by resolving paths from datastore
    ///
    /// Returns empty context if use_block is None.
    pub fn from_use_block(
        use_block: Option<&UseBlock>,
        datastore: &DataStore,
    ) -> Result<Self, NikaError> {
        let Some(block) = use_block else {
            return Ok(Self::new());
        };

        let mut ctx = Self::new();

        for (key, entry) in block {
            match entry {
                // Form 1: alias: task.path
                UseEntry::Path(path) => {
                    let value = datastore.resolve_path(path).ok_or_else(|| {
                        NikaError::Template(format!(
                            "NIKA-052: Path '{}' not found for alias '{}'",
                            path, key
                        ))
                    })?;
                    ctx.set(key, value);
                }

                // Form 2: task.path: [field1, field2]
                UseEntry::Batch(fields) => {
                    // The key IS the path in batch form
                    let base_value = datastore.resolve_path(key).ok_or_else(|| {
                        NikaError::Template(format!("NIKA-052: Path '{}' not found", key))
                    })?;

                    for field in fields {
                        let field_value = base_value.get(field).cloned().ok_or_else(|| {
                            NikaError::Template(format!(
                                "NIKA-052: Field '{}' not found in '{}'",
                                field, key
                            ))
                        })?;
                        ctx.set(field, field_value);
                    }
                }

                // Form 3: alias: { from: task, path: x.y, default: v }
                UseEntry::Advanced(UseAdvanced { from, path, default }) => {
                    // Get the output from the source task
                    let base_value = datastore.get_output(from);

                    // Apply JSONPath if path is specified
                    let value: Option<Value> = match (&base_value, path) {
                        (Some(v), Some(p)) => {
                            // Use JSONPath parser for $.a.b.c or a.b.c syntax
                            jsonpath::resolve(v, p)?
                        }
                        (Some(v), None) => Some(v.clone()),
                        (None, _) => None,
                    };

                    let display_path = match path {
                        Some(p) => format!("{}.{}", from, p),
                        None => from.clone(),
                    };

                    match (value, default) {
                        // Value found → use it
                        (Some(v), _) => {
                            // Check for null in strict mode (unless default provided)
                            if v.is_null() {
                                if let Some(def) = default {
                                    ctx.set(key, def.clone());
                                } else {
                                    return Err(NikaError::NullValue {
                                        path: display_path,
                                        alias: key.clone(),
                                    });
                                }
                            } else {
                                ctx.set(key, v);
                            }
                        }
                        // Not found but default exists → use default
                        (None, Some(def)) => {
                            ctx.set(key, def.clone());
                        }
                        // Not found and no default → error
                        (None, None) => {
                            return Err(NikaError::PathNotFound { path: display_path });
                        }
                    }
                }
            }
        }

        Ok(ctx)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::TaskResult;
    use crate::use_block::UseAdvanced;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn set_and_get() {
        let mut ctx = TaskContext::new();
        ctx.set("forecast", json!("Sunny"));

        assert_eq!(ctx.get("forecast"), Some(&json!("Sunny")));
        assert_eq!(ctx.get("unknown"), None);
    }

    #[test]
    fn is_empty() {
        let mut ctx = TaskContext::new();
        assert!(ctx.is_empty());

        ctx.set("key", json!("value"));
        assert!(!ctx.is_empty());
    }

    #[test]
    fn from_use_block_none() {
        let store = DataStore::new();
        let ctx = TaskContext::from_use_block(None, &store).unwrap();
        assert!(ctx.is_empty());
    }

    #[test]
    fn from_use_block_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
        );

        let mut block = UseBlock::new();
        block.insert("forecast".to_string(), UseEntry::Path("weather.summary".to_string()));

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("forecast"), Some(&json!("Sunny")));
    }

    #[test]
    fn from_use_block_batch() {
        let store = DataStore::new();
        store.insert(
            Arc::from("flight"),
            TaskResult::success(
                json!({"departure": "10:30", "gate": "A12"}),
                Duration::from_secs(1),
            ),
        );

        let mut block = UseBlock::new();
        block.insert(
            "flight".to_string(),
            UseEntry::Batch(vec!["departure".to_string(), "gate".to_string()]),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("departure"), Some(&json!("10:30")));
        assert_eq!(ctx.get("gate"), Some(&json!("A12")));
    }

    #[test]
    fn from_use_block_path_not_found() {
        let store = DataStore::new();

        let mut block = UseBlock::new();
        block.insert("x".to_string(), UseEntry::Path("missing.path".to_string()));

        let result = TaskContext::from_use_block(Some(&block), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: Advanced form tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn from_use_block_advanced_simple() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": "Sunny", "temp": 25}), Duration::from_secs(1)),
        );

        let mut block = UseBlock::new();
        block.insert(
            "data".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: None,
                default: None,
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("data"), Some(&json!({"summary": "Sunny", "temp": 25})));
    }

    #[test]
    fn from_use_block_advanced_with_path() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"data": {"summary": "Rainy"}}), Duration::from_secs(1)),
        );

        let mut block = UseBlock::new();
        block.insert(
            "forecast".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("data.summary".to_string()),
                default: None,
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("forecast"), Some(&json!("Rainy")));
    }

    #[test]
    fn from_use_block_advanced_default_on_missing() {
        let store = DataStore::new();
        // No weather task in store

        let mut block = UseBlock::new();
        block.insert(
            "forecast".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("summary".to_string()),
                default: Some(json!("Unknown")),
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("forecast"), Some(&json!("Unknown")));
    }

    #[test]
    fn from_use_block_advanced_default_on_null() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": null}), Duration::from_secs(1)),
        );

        let mut block = UseBlock::new();
        block.insert(
            "forecast".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("summary".to_string()),
                default: Some(json!("N/A")),
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("forecast"), Some(&json!("N/A")));
    }

    #[test]
    fn from_use_block_advanced_null_strict_error() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": null}), Duration::from_secs(1)),
        );

        let mut block = UseBlock::new();
        block.insert(
            "forecast".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("summary".to_string()),
                default: None, // No default → strict mode
            }),
        );

        let result = TaskContext::from_use_block(Some(&block), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    #[test]
    fn from_use_block_advanced_missing_no_default() {
        let store = DataStore::new();
        // No weather task

        let mut block = UseBlock::new();
        block.insert(
            "forecast".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: None,
                default: None,
            }),
        );

        let result = TaskContext::from_use_block(Some(&block), &store);
        assert!(result.is_err());
        // Should be PathNotFound error
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NIKA-052") || err_msg.contains("not found"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: JSONPath tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn from_use_block_advanced_jsonpath_dollar_syntax() {
        let store = DataStore::new();
        store.insert(
            Arc::from("flight"),
            TaskResult::success(
                json!({"price": {"currency": "EUR", "amount": 100}}),
                Duration::from_secs(1),
            ),
        );

        let mut block = UseBlock::new();
        block.insert(
            "currency".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "flight".to_string(),
                path: Some("$.price.currency".to_string()), // JSONPath with $
                default: None,
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("currency"), Some(&json!("EUR")));
    }

    #[test]
    fn from_use_block_advanced_jsonpath_array_index() {
        let store = DataStore::new();
        store.insert(
            Arc::from("data"),
            TaskResult::success(
                json!({"items": [{"name": "first"}, {"name": "second"}]}),
                Duration::from_secs(1),
            ),
        );

        let mut block = UseBlock::new();
        block.insert(
            "first_item".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "data".to_string(),
                path: Some("$.items[0].name".to_string()),
                default: None,
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("first_item"), Some(&json!("first")));
    }

    #[test]
    fn from_use_block_advanced_jsonpath_simple_dot() {
        let store = DataStore::new();
        store.insert(
            Arc::from("weather"),
            TaskResult::success(
                json!({"data": {"temp": 25}}),
                Duration::from_secs(1),
            ),
        );

        let mut block = UseBlock::new();
        block.insert(
            "temp".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("data.temp".to_string()), // Simple dot without $
                default: None,
            }),
        );

        let ctx = TaskContext::from_use_block(Some(&block), &store).unwrap();
        assert_eq!(ctx.get("temp"), Some(&json!(25)));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: to_value() for event logging
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn to_value_serializes_resolved_inputs() {
        let mut ctx = TaskContext::new();
        ctx.set("weather", json!("sunny"));
        ctx.set("temp", json!(25));
        ctx.set("nested", json!({"key": "value"}));

        let value = ctx.to_value();

        assert!(value.is_object());
        assert_eq!(value["weather"], "sunny");
        assert_eq!(value["temp"], 25);
        assert_eq!(value["nested"]["key"], "value");
    }

    #[test]
    fn to_value_empty_context() {
        let ctx = TaskContext::new();
        let value = ctx.to_value();

        assert!(value.is_object());
        assert!(value.as_object().unwrap().is_empty());
    }
}
