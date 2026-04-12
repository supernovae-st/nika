// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Resolved Bindings — re-exported from nika-core (S23-A3).
//!
//! The resolver moved to `nika_core::binding::resolve` to break the 3,752 LOC
//! engine monolith. This shim keeps the `crate::binding::resolve` path alive
//! for intra-engine callers and hosts the engine-side test module (which
//! exercises the full integration surface via `RunContext`).

pub use nika_core::binding::resolve::{LazyBinding, ResolvedBindings};

// Private helpers re-exported only for the engine-side test module
// (integration surface against `RunContext`). Not part of the public API.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use nika_core::binding::resolve::{
    json_type_name, navigate_segments, resolve_binding_path, resolve_binding_path_traced,
    resolve_entry, resolve_with_entry, resolve_with_entry_traced, split_path,
    validate_binding_type,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{
        BindingEntry, BindingSpec, BindingSource, BindingType, PathSegment, WithEntry, WithSpec,
    };
    use crate::binding::types::BindingPath;
    use crate::store::{RunContext, TaskResult};
    use nika_core::binding::transform::TransformExpr;
    use nika_core::binding::BindingResolveError;
    use nika_kernel_mock::MockBindingStore;
    use serde_json::{json, Value};
    use serial_test::serial;
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
    fn from_binding_spec_none() {
        let store = MockBindingStore::new();
        let bindings = ResolvedBindings::from_binding_spec(None, &store).unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn from_with_spec_none() {
        let store = MockBindingStore::new();
        let bindings = ResolvedBindings::from_with_spec(None, &store).unwrap();
        assert!(bindings.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // String-path resolution tests (BindingEntry / BindingSpec)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_simple_path() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"summary": "Sunny"}));

        let mut spec = BindingSpec::default();
        spec.insert("forecast".to_string(), BindingEntry::new("weather.summary"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("Sunny")));
    }

    #[test]
    fn resolve_entire_task_output() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"summary": "Sunny", "temp": 25}));

        let mut spec = BindingSpec::default();
        spec.insert("data".to_string(), BindingEntry::new("weather"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(
            bindings.get("data"),
            Some(&json!({"summary": "Sunny", "temp": 25}))
        );
    }

    #[test]
    fn resolve_nested_path() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"data": {"temp": {"celsius": 25}}}));

        let mut spec = BindingSpec::default();
        spec.insert(
            "temp".to_string(),
            BindingEntry::new("weather.data.temp.celsius"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("temp"), Some(&json!(25)));
    }

    #[test]
    fn resolve_with_default_on_missing() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert(
            "forecast".to_string(),
            BindingEntry::with_default("weather.summary", json!("Unknown")),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("Unknown")));
    }

    #[test]
    fn resolve_with_default_on_null() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"summary": null}));

        let mut spec = BindingSpec::default();
        spec.insert(
            "forecast".to_string(),
            BindingEntry::with_default("weather.summary", json!("N/A")),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("forecast"), Some(&json!("N/A")));
    }

    #[test]
    fn resolve_with_default_object() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert(
            "cfg".to_string(),
            BindingEntry::with_default("settings", json!({"debug": false})),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("cfg"), Some(&json!({"debug": false})));
    }

    #[test]
    fn resolve_with_default_array() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert(
            "tags".to_string(),
            BindingEntry::with_default("meta.tags", json!(["default"])),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("tags"), Some(&json!(["default"])));
    }

    // ═══════════════════════════════════════════════════════════════
    // Error cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_path_not_found_error() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert("x".to_string(), BindingEntry::new("missing.path"));

        let result = ResolvedBindings::from_binding_spec(Some(&spec), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052"));
    }

    #[test]
    fn resolve_null_strict_error() {
        let store = MockBindingStore::new()
            .with_output("weather", json!({"summary": null}));

        let mut spec = BindingSpec::default();
        spec.insert("forecast".to_string(), BindingEntry::new("weather.summary"));

        let result = ResolvedBindings::from_binding_spec(Some(&spec), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    // ═══════════════════════════════════════════════════════════════
    // JSONPath tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn resolve_jsonpath_array_index() {
        let store = MockBindingStore::new()
            .with_output("data", json!({"items": [{"name": "first"}, {"name": "second"}]}));

        let mut spec = BindingSpec::default();
        spec.insert("first".to_string(), BindingEntry::new("data.items[0].name"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
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
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "result"}));

        let mut spec = BindingSpec::default();
        spec.insert(
            "lazy_bind".to_string(),
            BindingEntry::lazy_with_default("task.value", json!("default")),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        // get() should NOT resolve lazy bindings
        assert_eq!(bindings.get("lazy_bind"), None);
    }

    // ═══════════════════════════════════════════════════════════════
    // ResolvedBindings::get_resolved() tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_resolved_eager_binding() {
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "result"}));

        let mut spec = BindingSpec::default();
        spec.insert("eager".to_string(), BindingEntry::new("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let result = bindings.get_resolved("eager", &store).unwrap();
        assert_eq!(result, json!("result"));
    }

    #[test]
    fn get_resolved_lazy_binding() {
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "lazy_result"}));

        let mut spec = BindingSpec::default();
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let result = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result, json!("lazy_result"));
    }

    #[test]
    fn get_resolved_nonexistent_binding() {
        let store = MockBindingStore::new();
        let bindings = ResolvedBindings::new();
        let result = bindings.get_resolved("missing", &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-042")); // BindingNotFound
    }

    #[test]
    fn get_resolved_lazy_with_default() {
        let store = MockBindingStore::new();
        // No task in store - should use default

        let mut spec = BindingSpec::default();
        spec.insert(
            "lazy_default".to_string(),
            BindingEntry::lazy_with_default("missing.path", json!("fallback")),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let result = bindings.get_resolved("lazy_default", &store).unwrap();
        assert_eq!(result, json!("fallback"));
    }

    #[test]
    fn get_resolved_re_resolves_on_each_call() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"counter": 1}), Duration::from_secs(1)),
        );

        let mut spec = BindingSpec::default();
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.counter"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

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
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "test"}));

        let mut spec = BindingSpec::default();
        spec.insert("eager".to_string(), BindingEntry::new("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert!(!bindings.is_lazy("eager"));
    }

    #[test]
    fn is_lazy_for_lazy_binding() {
        let store = MockBindingStore::new();
        let mut spec = BindingSpec::default();
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert!(bindings.is_lazy("lazy"));
    }

    #[test]
    fn is_lazy_for_nonexistent_binding() {
        let bindings = ResolvedBindings::new();
        assert!(!bindings.is_lazy("missing"));
    }

    #[test]
    fn is_lazy_after_resolution() {
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "result"}));

        let mut spec = BindingSpec::default();
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
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
        let store = MockBindingStore::new()
            .with_output("task", json!({"value": "result"}));

        let mut spec = BindingSpec::default();
        spec.insert("eager".to_string(), BindingEntry::new("task.value"));
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.value"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

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
        bindings.insert_raw("lazy",
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
        bindings.insert_raw("lazy_new",
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
    // Error handling in from_binding_spec()
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn from_binding_spec_eager_missing_path() {
        let store = MockBindingStore::new();
        let mut spec = BindingSpec::default();
        spec.insert("x".to_string(), BindingEntry::new("nonexistent.path"));

        let result = ResolvedBindings::from_binding_spec(Some(&spec), &store);
        assert!(result.is_err());
    }

    #[test]
    fn from_binding_spec_lazy_does_not_fail_on_missing() {
        let store = MockBindingStore::new();
        let mut spec = BindingSpec::default();
        spec.insert("x".to_string(), BindingEntry::new_lazy("nonexistent.path"));

        // Lazy bindings don't fail during from_binding_spec - they fail on get_resolved()
        let result = ResolvedBindings::from_binding_spec(Some(&spec), &store);
        assert!(result.is_ok());
    }

    #[test]
    fn from_binding_spec_preserves_all_entries() {
        let store = MockBindingStore::new()
            .with_output("task1", json!({"a": 1}))
            .with_output("task2", json!({"b": 2}));

        let mut spec = BindingSpec::default();
        spec.insert("binding1".to_string(), BindingEntry::new("task1.a"));
        spec.insert("binding2".to_string(), BindingEntry::new_lazy("task2.b"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Both bindings should exist
        assert_eq!(bindings.get("binding1"), Some(&json!(1)));
        assert!(bindings.is_lazy("binding2"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Mixed eager and lazy bindings
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn mixed_eager_and_lazy_workflow() {
        let store = MockBindingStore::new()
            .with_output("quick", json!({"result": "fast"}))
            .with_output("slow", json!({"result": "slow_value"}));

        let mut spec = BindingSpec::default();
        spec.insert("quick_bind".to_string(), BindingEntry::new("quick.result"));
        spec.insert(
            "slow_bind".to_string(),
            BindingEntry::new_lazy("slow.result"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

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
        let store = MockBindingStore::new()
            .with_input("topic", json!({"type": "string", "default": "AI trends 2025"}));

        let mut spec = BindingSpec::default();
        spec.insert("topic_val".to_string(), BindingEntry::new("inputs.topic"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("topic_val"), Some(&json!("AI trends 2025")));
    }

    #[test]
    fn resolve_inputs_nested_field() {
        let store = MockBindingStore::new()
            .with_input("config", json!({
                "type": "object",
                "default": {
                    "theme": "dark",
                    "version": 2,
                    "nested": {
                        "deep": "value"
                    }
                }
            }));

        let mut spec = BindingSpec::default();
        spec.insert(
            "theme".to_string(),
            BindingEntry::new("inputs.config.theme"),
        );
        spec.insert(
            "deep".to_string(),
            BindingEntry::new("inputs.config.nested.deep"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("theme"), Some(&json!("dark")));
        assert_eq!(bindings.get("deep"), Some(&json!("value")));
    }

    #[test]
    fn resolve_inputs_with_default_on_missing() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert(
            "fallback".to_string(),
            BindingEntry::with_default("inputs.missing", json!("default_value")),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("fallback"), Some(&json!("default_value")));
    }

    #[test]
    fn resolve_inputs_missing_no_default() {
        let store = MockBindingStore::new();

        let mut spec = BindingSpec::default();
        spec.insert("missing".to_string(), BindingEntry::new("inputs.missing"));

        let result = ResolvedBindings::from_binding_spec(Some(&spec), &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-052")); // PathNotFound
    }

    #[test]
    fn resolve_inputs_lazy_binding() {
        let store = MockBindingStore::new()
            .with_input("lazy_input", json!({"type": "string", "default": "lazy_value"}));

        let mut spec = BindingSpec::default();
        spec.insert(
            "lazy_alias".to_string(),
            BindingEntry::new_lazy("inputs.lazy_input"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        assert!(bindings.is_lazy("lazy_alias"));
        assert_eq!(bindings.get("lazy_alias"), None);

        let resolved = bindings.get_resolved("lazy_alias", &store).unwrap();
        assert_eq!(resolved, json!("lazy_value"));
    }

    #[test]
    fn resolve_inputs_mixed_with_task_outputs() {
        let store = MockBindingStore::new()
            .with_input("topic", json!({"type": "string", "default": "AI"}))
            .with_output("step1", json!({"result": "generated"}));

        let mut spec = BindingSpec::default();
        spec.insert("from_input".to_string(), BindingEntry::new("inputs.topic"));
        spec.insert("from_task".to_string(), BindingEntry::new("step1.result"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        assert_eq!(bindings.get("from_input"), Some(&json!("AI")));
        assert_eq!(bindings.get("from_task"), Some(&json!("generated")));
    }

    #[test]
    fn resolve_inputs_array_value() {
        let store = MockBindingStore::new()
            .with_input("items", json!({"type": "array", "default": ["a", "b", "c"]}));

        let mut spec = BindingSpec::default();
        spec.insert("all_items".to_string(), BindingEntry::new("inputs.items"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("all_items"), Some(&json!(["a", "b", "c"])));
    }

    // ═══════════════════════════════════════════════════════════════
    // from_with_spec tests (with: block)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_task_simple() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"title": "Hello"}));

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"a": 1, "b": 2}));

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"data": {"items": [{"name": "first"}]}}));

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
        let store = MockBindingStore::new();
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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"data": null}));

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
        let store = MockBindingStore::new();

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"data": null}));

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
        let store = MockBindingStore::new()
            .with_input("topic", json!({"type": "string", "default": "AI trends"}));

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
        let store = MockBindingStore::new()
            .with_input("config", json!({
                "type": "object",
                "default": {"theme": "dark", "nested": {"deep": "val"}}
            }));

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
        let store = MockBindingStore::new();
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
    #[serial]
    fn with_spec_env_existing_var() {
        // Use a known env var
        std::env::set_var("NIKA_TEST_VAR_8A", "test_value_8a");

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
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
        let store = MockBindingStore::new();
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
        let store = MockBindingStore::new();
        let mut spec = WithSpec::default();
        spec.insert(
            "missing".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_NONEXISTENT_VAR_ABC").unwrap()),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err());
    }

    #[test]
    fn with_spec_env_missing_transform_default_fires() {
        // $env.MISSING | default("fallback") should resolve to "fallback"
        use nika_core::binding::transform::TransformExpr;

        let store = MockBindingStore::new();
        let mut entry = WithEntry::simple(
            BindingPath::parse("$env.NIKA_NONEXISTENT_TRANSFORM_DEFAULT_TEST").unwrap(),
        );
        entry.transform = Some(TransformExpr::parse("default(\"fallback\")").unwrap());

        let mut spec = WithSpec::default();
        spec.insert("val".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("val"), Some(&json!("fallback")));
    }

    #[test]
    fn with_spec_env_missing_transform_default_then_upper() {
        // $env.MISSING | default("fallback") | upper → "FALLBACK"
        use nika_core::binding::transform::TransformExpr;

        let store = MockBindingStore::new();
        let mut entry =
            WithEntry::simple(BindingPath::parse("$env.NIKA_NONEXISTENT_CHAIN_TEST").unwrap());
        entry.transform = Some(TransformExpr::parse("default(\"hello\") | upper").unwrap());

        let mut spec = WithSpec::default();
        spec.insert("val".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("val"), Some(&json!("HELLO")));
    }

    #[test]
    fn with_spec_env_missing_transform_no_default_still_errors() {
        // $env.MISSING | upper should still fail (no default in chain)
        use nika_core::binding::transform::TransformExpr;

        let store = MockBindingStore::new();
        let mut entry =
            WithEntry::simple(BindingPath::parse("$env.NIKA_NONEXISTENT_UPPER_TEST").unwrap());
        entry.transform = Some(TransformExpr::parse("upper").unwrap());

        let mut spec = WithSpec::default();
        spec.insert("val".to_string(), entry);

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(
            result.is_err(),
            "Missing env with non-default transform should error"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: Context source tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_context_file() {
        let store = MockBindingStore::new()
            .with_context("brand", json!("Brand Guidelines v2"));

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
        let store = MockBindingStore::new()
            .with_session(json!({"last_run": "2025-01-01"}));

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
        let store = MockBindingStore::new();
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
        let store = MockBindingStore::new();

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"data": "deferred"}));

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
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"name": "  Hello World  "}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.name").unwrap());
        entry.transform = Some(TransformExpr::parse("trim | upper").unwrap());
        spec.insert("name".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("name"), Some(&json!("HELLO WORLD")));
    }

    #[test]
    fn with_spec_transform_with_default_on_null() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"name": null}));

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
    fn to_value_redacted_masks_env_sourced_bindings() {
        let mut bindings = ResolvedBindings::new();
        bindings.insert_raw("token",
            LazyBinding::Resolved(json!("super-secret-123")),
        );
        bindings.mark_env_sourced("token");
        bindings.insert_raw("name",
            LazyBinding::Resolved(json!("safe-value")),
        );

        let redacted = bindings.to_value_redacted();
        assert_eq!(redacted["token"], json!("[REDACTED:$env]"));
        assert_eq!(redacted["name"], json!("safe-value"));
    }

    #[test]
    fn to_value_redacted_also_catches_api_key_patterns() {
        let mut bindings = ResolvedBindings::new();
        // Not env-sourced but contains API key pattern
        bindings.insert_raw("header",
            LazyBinding::Resolved(json!("Bearer sk-proj-abcdefghij1234567890")),
        );

        let redacted = bindings.to_value_redacted();
        let val = redacted["header"].as_str().unwrap();
        assert!(
            val.contains("[REDACTED]"),
            "API key pattern not masked: {}",
            val
        );
        assert!(!val.contains("sk-proj-"), "API key leaked: {}", val);
    }

    #[test]
    fn to_value_redacted_recurses_into_nested_objects() {
        let mut bindings = ResolvedBindings::new();
        bindings.insert_raw("config",
            LazyBinding::Resolved(json!({
                "provider": "openai",
                "auth": {
                    "key": "sk-proj-abcdefghij1234567890",
                    "endpoint": "https://api.openai.com"
                },
                "db": "postgres://admin:s3cret@db.example.com:5432/prod"
            })),
        );

        let redacted = bindings.to_value_redacted();
        let config = &redacted["config"];
        // Nested object string should be redacted
        assert!(
            config["auth"]["key"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]"),
            "Nested API key not redacted: {}",
            config["auth"]["key"]
        );
        // Safe string should be preserved
        assert_eq!(config["provider"], json!("openai"));
        // DB URI should be redacted
        assert!(
            config["db"].as_str().unwrap().contains("[REDACTED]"),
            "DB URI not redacted: {}",
            config["db"]
        );
    }

    #[test]
    fn to_value_redacted_recurses_into_arrays() {
        let mut bindings = ResolvedBindings::new();
        bindings.insert_raw("keys",
            LazyBinding::Resolved(json!([
                "safe-value",
                "sk-proj-abcdefghij1234567890",
                {"token": "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"}
            ])),
        );

        let redacted = bindings.to_value_redacted();
        let keys = redacted["keys"].as_array().unwrap();
        assert_eq!(keys[0], json!("safe-value"));
        assert!(keys[1].as_str().unwrap().contains("[REDACTED]"));
        assert!(keys[2]["token"].as_str().unwrap().contains("[REDACTED]"));
    }

    #[test]
    fn with_spec_traced_transform_with_default_on_null() {
        // CQ-11: traced and untraced must behave identically for null+transform+default
        let store = MockBindingStore::new()
            .with_output("step1", json!({"name": null}));

        let mut spec = WithSpec::default();
        let mut entry =
            WithEntry::with_default(BindingPath::parse("$step1.name").unwrap(), json!("DEFAULT"));
        entry.transform = Some(TransformExpr::parse("upper").unwrap());
        spec.insert("name".to_string(), entry);

        let task_id: Arc<str> = Arc::from("test_traced");
        let (bindings, _events) =
            ResolvedBindings::from_with_spec_traced(Some(&spec), &store, &task_id).unwrap();

        // Must match untraced behavior: null → transform fails → default kicks in
        assert_eq!(bindings.get("name"), Some(&json!("DEFAULT")));
    }

    #[test]
    fn with_spec_transform_chain() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"items": [3, 1, 4, 1, 5, 9]}));

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"name": "text"}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.name").unwrap());
        entry.binding_type = BindingType::String;
        spec.insert("name".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("name"), Some(&json!("text")));
    }

    #[test]
    fn with_spec_type_string_invalid() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"count": 42}));

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"items": [1, 2, 3]}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.items").unwrap());
        entry.binding_type = BindingType::Array;
        spec.insert("items".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("items"), Some(&json!([1, 2, 3])));
    }

    #[test]
    fn with_spec_type_any_accepts_all() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"val": [1, "mixed"]}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.val").unwrap());
        entry.binding_type = BindingType::Any;
        spec.insert("val".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("val"), Some(&json!([1, "mixed"])));
    }

    #[test]
    fn with_spec_type_object_valid() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"cfg": {"debug": true}}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.cfg").unwrap());
        entry.binding_type = BindingType::Object;
        spec.insert("cfg".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("cfg"), Some(&json!({"debug": true})));
    }

    #[test]
    fn with_spec_type_number_valid() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"temp": 25.5}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.temp").unwrap());
        entry.binding_type = BindingType::Number;
        spec.insert("temp".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("temp"), Some(&json!(25.5)));
    }

    #[test]
    fn with_spec_type_integer_valid() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"count": 42}));

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.count").unwrap());
        entry.binding_type = BindingType::Integer;
        spec.insert("count".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("count"), Some(&json!(42)));
    }

    #[test]
    fn with_spec_type_integer_rejects_float() {
        let store = MockBindingStore::new()
            .with_output("step1", json!({"val": 3.12}));

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
        let store = MockBindingStore::new()
            .with_output("step1", json!({"flag": true}));

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
    #[serial]
    fn with_spec_mixed_sources() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

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
            use crate::store::LoadedContext;
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
    // BUG-001: $env should allow secret-pattern vars (KEY, TOKEN, etc.)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn env_binding_allows_secret_pattern_vars() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Set env vars with secret patterns (KEY, TOKEN, AUTH)
        std::env::set_var("NIKA_TEST_ELEVENLABS_API_KEY", "sk-test-123");
        std::env::set_var("NIKA_TEST_MY_SECRET_TOKEN", "tok-456");
        std::env::set_var("NIKA_TEST_CUSTOM_AUTH", "auth-789");

        let mut spec = WithSpec::default();
        spec.insert(
            "api_key".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_ELEVENLABS_API_KEY").unwrap()),
        );
        spec.insert(
            "token".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_MY_SECRET_TOKEN").unwrap()),
        );
        spec.insert(
            "auth".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_CUSTOM_AUTH").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // All secret-pattern vars should now be accessible (BUG-001 fix)
        assert_eq!(bindings.get("api_key"), Some(&json!("sk-test-123")));
        assert_eq!(bindings.get("token"), Some(&json!("tok-456")));
        assert_eq!(bindings.get("auth"), Some(&json!("auth-789")));

        std::env::remove_var("NIKA_TEST_ELEVENLABS_API_KEY");
        std::env::remove_var("NIKA_TEST_MY_SECRET_TOKEN");
        std::env::remove_var("NIKA_TEST_CUSTOM_AUTH");
    }

    // SEC-1: $env blocklist — restricted vars return None (as if unset)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn env_binding_blocks_restricted_vars() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Set restricted env vars to known values
        std::env::set_var("SSH_AUTH_SOCK", "/tmp/test-agent.sock");
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "super-secret");

        let mut spec = WithSpec::default();
        // SSH_AUTH_SOCK — process/system internal
        let mut entry_ssh = WithEntry::simple(BindingPath::parse("$env.SSH_AUTH_SOCK").unwrap());
        entry_ssh.transform = Some(TransformExpr::parse("default(\"blocked\")").unwrap());
        spec.insert("ssh".to_string(), entry_ssh);
        // NIKA_VAULT_PASSPHRASE — nika internal
        let mut entry_vault =
            WithEntry::simple(BindingPath::parse("$env.NIKA_VAULT_PASSPHRASE").unwrap());
        entry_vault.transform = Some(TransformExpr::parse("default(\"blocked\")").unwrap());
        spec.insert("vault".to_string(), entry_vault);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // Both should resolve to the default, not the actual value
        assert_eq!(bindings.get("ssh"), Some(&json!("blocked")));
        assert_eq!(bindings.get("vault"), Some(&json!("blocked")));

        std::env::remove_var("SSH_AUTH_SOCK");
        std::env::remove_var("NIKA_VAULT_PASSPHRASE");
    }

    #[test]
    fn env_binding_blocks_case_sensitive() {
        // Blocklist uses uppercase comparison
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        std::env::set_var("LD_PRELOAD", "/tmp/evil.so");

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$env.LD_PRELOAD").unwrap());
        entry.transform = Some(TransformExpr::parse("default(\"safe\")").unwrap());
        spec.insert("ld".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("ld"), Some(&json!("safe")));

        std::env::remove_var("LD_PRELOAD");
    }

    #[test]
    fn env_binding_returns_error_for_missing_var() {
        let store = MockBindingStore::new();

        let mut spec = WithSpec::default();
        spec.insert(
            "missing".to_string(),
            WithEntry::simple(
                BindingPath::parse("$env.NIKA_TEST_SURELY_NONEXISTENT_VAR_12345").unwrap(),
            ),
        );

        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(
            result.is_err(),
            "Missing env var without ?? fallback should error"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // WithSpec: LoopVar error
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_spec_loop_var_errors() {
        let store = MockBindingStore::new();

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
        let result = navigate_segments(&value, &[]).unwrap();
        assert_eq!(result, Some(json!({"hello": "world"})));
    }

    #[test]
    fn navigate_segments_field() {
        let value = json!({"name": "Nika"});
        let segments = vec![PathSegment::Field(Arc::from("name"))];
        let result = navigate_segments(&value, &segments).unwrap();
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
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, Some(json!(42)));
    }

    #[test]
    fn navigate_segments_array_index() {
        let value = json!({"items": ["a", "b", "c"]});
        let segments = vec![
            PathSegment::Field(Arc::from("items")),
            PathSegment::Index(1),
        ];
        let result = navigate_segments(&value, &segments).unwrap();
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
        let result = navigate_segments(&value, &segments).unwrap();
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

    #[test]
    fn navigate_segments_json_string_auto_parse() {
        // Exec output is stored as Value::String containing JSON
        let value = json!(r#"{"name":"Nika","version":"0.30"}"#);
        let segments = vec![PathSegment::Field(Arc::from("name"))];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, Some(json!("Nika")));
    }

    #[test]
    fn navigate_segments_json_string_deep_access() {
        let value = json!(r#"{"a":{"b":{"c":"deep"}}}"#);
        let segments = vec![
            PathSegment::Field(Arc::from("a")),
            PathSegment::Field(Arc::from("b")),
            PathSegment::Field(Arc::from("c")),
        ];
        let result = navigate_segments(&value, &segments).unwrap();
        assert_eq!(result, Some(json!("deep")));
    }

    #[test]
    fn navigate_segments_plain_string_returns_none() {
        // Non-JSON string should NOT be parsed
        let value = json!("hello world");
        let segments = vec![PathSegment::Field(Arc::from("name"))];
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
        validate_binding_type(&json!(3.12), BindingType::Number, "a", "p").unwrap();
    }

    #[test]
    fn validate_type_integer_rejects_float() {
        let result = validate_binding_type(&json!(3.12), BindingType::Integer, "a", "p");
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

    // ========== source_task_id tracking ==========

    #[test]
    fn source_task_id_set_with_source() {
        let mut bindings = ResolvedBindings::new();
        bindings.set_with_source("img", json!("output text"), "gen_img");

        assert_eq!(bindings.source_task_id("img"), Some("gen_img"));
        assert_eq!(bindings.source_task_id("nonexistent"), None);

        // The value should still be accessible
        assert_eq!(bindings.get("img"), Some(&json!("output text")));
    }

    #[test]
    fn source_task_id_plain_set_has_no_tracking() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("img", json!("output text"));

        // Plain set() does not track source task ID
        assert_eq!(bindings.source_task_id("img"), None);
    }

    #[test]
    fn source_task_id_from_binding_spec() {
        use crate::binding::BindingEntry;

        let mut spec = BindingSpec::default();
        spec.insert("forecast".to_string(), BindingEntry::new("weather.summary"));

        let store = MockBindingStore::new()
            .with_output("weather", json!({"summary": "Sunny"}));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // "forecast" binding came from $weather -> source task ID is "weather"
        assert_eq!(bindings.source_task_id("forecast"), Some("weather"));
    }

    // =========================================================================
    // Media Tool Results → Binding Spec → Template Resolution
    // =========================================================================
    //
    // These tests verify the end-to-end path:
    //   invoke result → RunContext → BindingSpec → ResolvedBindings → get_resolved
    //
    // Three scenarios:
    //   1. Invoke output (JSON) fields accessible via binding paths
    //   2. Media refs (side-channel) accessible via media[N].field paths
    //   3. Chained bindings: gen.media[0].hash + thumb.hash both accessible

    /// Helper: populate a RunContext with a "gen" task that has media refs
    /// and a "thumb" task that has thumbnail JSON output.
    fn store_with_media_chain() -> RunContext {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task "gen": produces an image with media refs in the side-channel
        let gen_media = vec![crate::media::MediaRef {
            hash: "blake3:abc123def456".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 1048576,
            path: std::path::PathBuf::from("/tmp/cas/ab/c123def456"),
            extension: "png".to_string(),
            created_by: "gen".to_string(),
            metadata: {
                let mut m = serde_json::Map::new();
                m.insert("width".to_string(), json!(1024));
                m.insert("height".to_string(), json!(768));
                m
            },
        }];
        store.insert(
            Arc::from("gen"),
            TaskResult::success(json!({"prompt": "a sunset photo"}), Duration::from_secs(3))
                .with_media(gen_media),
        );

        // Task "thumb": invoke result stored as JSON-string output
        store.insert(
            Arc::from("thumb"),
            TaskResult::success_str(
                r#"{"hash":"blake3:thumb_999","mime_type":"image/png","size_bytes":2048,"metadata":{"width":256,"height":192}}"#,
                Duration::from_millis(100),
            ),
        );

        store
    }

    // ----- Scenario 1: Invoke output fields via binding spec -----

    #[test]
    fn binding_spec_resolves_invoke_output_hash() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { thumb_hash: $thumb.hash }
        spec.insert("thumb_hash".to_string(), BindingEntry::new("thumb.hash"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:thumb_999"));
    }

    #[test]
    fn binding_spec_resolves_invoke_output_nested_metadata() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { thumb_width: $thumb.metadata.width }
        spec.insert(
            "thumb_width".to_string(),
            BindingEntry::new("thumb.metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_width", &store).unwrap();
        assert_eq!(value, json!(256));
    }

    #[test]
    fn binding_spec_resolves_invoke_output_mime_type() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        spec.insert(
            "thumb_mime".to_string(),
            BindingEntry::new("thumb.mime_type"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_mime", &store).unwrap();
        assert_eq!(value, json!("image/png"));
    }

    // ----- Scenario 2: Media refs via binding spec -----

    #[test]
    fn binding_spec_resolves_media_ref_hash() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { gen_hash: $gen.media[0].hash }
        spec.insert(
            "gen_hash".to_string(),
            BindingEntry::new("gen.media[0].hash"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:abc123def456"));
    }

    #[test]
    fn binding_spec_resolves_media_ref_enriched_width() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { gen_width: $gen.media[0].metadata.width }
        spec.insert(
            "gen_width".to_string(),
            BindingEntry::new("gen.media[0].metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_width", &store).unwrap();
        assert_eq!(value, json!(1024));
    }

    #[test]
    fn binding_spec_resolves_media_ref_mime_type() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        spec.insert(
            "gen_mime".to_string(),
            BindingEntry::new("gen.media[0].mime_type"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_mime", &store).unwrap();
        assert_eq!(value, json!("image/png"));
    }

    #[test]
    fn binding_spec_resolves_media_full_array() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { all_media: $gen.media }
        spec.insert("all_media".to_string(), BindingEntry::new("gen.media"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("all_media", &store).unwrap();
        let arr = value.as_array().expect("media should be an array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["hash"], "blake3:abc123def456");
    }

    // ----- Scenario 3: Chained bindings -----

    #[test]
    fn binding_spec_chained_gen_media_and_thumb_output() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();

        // Bind both gen.media[0].hash and thumb.hash in one spec
        spec.insert(
            "source_hash".to_string(),
            BindingEntry::new("gen.media[0].hash"),
        );
        spec.insert("thumb_hash".to_string(), BindingEntry::new("thumb.hash"));
        spec.insert(
            "thumb_width".to_string(),
            BindingEntry::new("thumb.metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Verify all three bindings resolve correctly
        assert_eq!(
            bindings.get_resolved("source_hash", &store).unwrap(),
            json!("blake3:abc123def456"),
            "gen.media[0].hash should resolve via media side-channel"
        );
        assert_eq!(
            bindings.get_resolved("thumb_hash", &store).unwrap(),
            json!("blake3:thumb_999"),
            "thumb.hash should resolve via JSON-string auto-parse"
        );
        assert_eq!(
            bindings.get_resolved("thumb_width", &store).unwrap(),
            json!(256),
            "thumb.metadata.width should resolve via nested JSON-string traversal"
        );
    }

    #[test]
    fn binding_spec_lazy_media_ref_resolves_on_access() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // Lazy binding: resolution deferred until get_resolved
        spec.insert(
            "lazy_hash".to_string(),
            BindingEntry {
                path: "gen.media[0].hash".to_string(),
                default: None,
                lazy: true,
            },
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Should be pending initially
        assert!(bindings.is_lazy("lazy_hash"));

        // But still resolves correctly via get_resolved
        let value = bindings.get_resolved("lazy_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:abc123def456"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Failed task binding warning tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn binding_to_failed_task_returns_null_error() {
        // A failed task has Value::Null output — binding without default should
        // return NullValue error (and the warning is emitted before the error).
        let store = MockBindingStore::new()
            .with_output("broken", Value::Null)
            .with_failed("broken");

        let entry = BindingEntry::new("broken");
        let result = resolve_entry(&entry, "data", &store);
        assert!(result.is_err());
        assert!(
            matches!(&result.unwrap_err(), BindingResolveError::NullValue { path, .. } if path == "broken")
        );
    }

    #[test]
    fn binding_to_failed_task_uses_default() {
        // With a default, binding to failed task resolves to the default value
        let store = MockBindingStore::new()
            .with_output("broken", Value::Null)
            .with_failed("broken");

        let entry = BindingEntry {
            path: "broken".to_string(),
            default: Some(json!("fallback")),
            lazy: false,
        };
        let result = resolve_entry(&entry, "data", &store);
        assert_eq!(result.unwrap(), json!("fallback"));
    }

    #[test]
    fn binding_to_failed_task_typed_path() {
        let store = MockBindingStore::new()
            .with_output("broken", Value::Null)
            .with_failed("broken");

        // Typed path: resolve_with_entry via from_with_spec
        let entry = WithEntry {
            source: BindingPath {
                source: BindingSource::Task(Arc::from("broken")),
                segments: vec![],
            },
            binding_type: BindingType::Any,
            default: Some(json!("fallback")),
            lazy: false,
            transform: None,
        };

        let mut spec = WithSpec::default();
        spec.insert("data".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        // Should resolve — either the failed task output or the default
        assert!(bindings.get("data").is_some());
    }

    // ═══════════════════════════════════════════════════════════════
    // Record-aware binding tests (S22 P1-2 from rust-pro review)
    //
    // resolve_binding_path has a special path when a task has a compression
    // Record. These tests use MockBindingStore's with_record builder to
    // exercise the trait-level get_record() delegation.
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn record_aware_empty_segments_returns_summary() {
        // $task (no segments) → extract summary string from opaque JSON
        let record = json!({
            "summary": "Compressed findings",
            "key_findings": ["finding A", "finding B"],
            "confidence": 0.92,
            "tokens_original": 5000u64,
            "tokens_compressed": 200u64,
            "compression_ratio": 0.04
        });
        let store = MockBindingStore::new()
            .with_output("research", json!({"raw": "original output"}))
            .with_record("research", record);

        let path = BindingPath {
            source: BindingSource::Task(Arc::from("research")),
            segments: vec![],
        };
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("summary".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("summary"), Some(&json!("Compressed findings")));
    }

    #[test]
    fn record_aware_field_access_navigates_opaque_json() {
        // $task.confidence → navigate the opaque record JSON
        let record = json!({
            "summary": "Summary text",
            "key_findings": ["A", "B"],
            "confidence": 0.85,
            "tokens_original": 1000u64,
            "tokens_compressed": 100u64,
            "compression_ratio": 0.1
        });
        let store = MockBindingStore::new()
            .with_output("research", json!({"data": "raw"}))
            .with_record("research", record);

        let path = BindingPath::parse("$research.confidence").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("conf".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("conf"), Some(&json!(0.85)));
    }

    #[test]
    fn record_aware_raw_bypasses_record() {
        // $task.raw → bypass the Record, return raw task output
        let record = json!({
            "summary": "Short summary",
            "key_findings": [],
            "confidence": 1.0,
            "tokens_original": 100u64,
            "tokens_compressed": 10u64,
            "compression_ratio": 0.1
        });
        let raw_output = json!({"full": "raw unfiltered data", "tokens": 5000});
        let store = MockBindingStore::new()
            .with_output("research", raw_output.clone())
            .with_record("research", record);

        let path = BindingPath::parse("$research.raw").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("raw_data".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        // .raw returns the full task output, not a navigated field
        assert_eq!(bindings.get("raw_data"), Some(&raw_output));
    }

    #[test]
    fn redact_secrets_in_binding_default() {
        use crate::util::redact_secrets;

        let secret = "sk-ant-api03-real-secret-key-here-1234567890";
        let redacted = redact_secrets(secret);
        assert!(
            redacted.contains("[REDACTED]"),
            "secret should be redacted: {redacted}"
        );
        assert!(
            !redacted.contains("real-secret"),
            "raw secret must not appear: {redacted}"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // $vault.SERVICE.FIELD binding tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    #[serial]
    fn vault_binding_parses_correctly() {
        // Parsing $vault.stripe.secret should produce BindingSource::Vault
        let bp = BindingPath::parse("$vault.stripe.secret").unwrap();
        assert_eq!(
            bp.source,
            BindingSource::Vault {
                service: Arc::from("stripe"),
                field: Arc::from("secret"),
            }
        );
        assert!(bp.segments.is_empty());
    }

    #[test]
    #[serial]
    fn vault_binding_resolves_value() {
        // Set up a vault with credentials in a temp dir
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut fields = std::collections::BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live_test123".to_string());
        fields.insert("secret".to_string(), "whsec_test456".to_string());
        vault.set_credential("stripe", fields, None, None).unwrap();

        // Attach vault to RunContext
        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        // Build WithSpec with $vault.stripe.secret
        let path = BindingPath::parse("$vault.stripe.secret").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("stripe_secret".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("stripe_secret"), Some(&json!("whsec_test456")));
    }

    #[test]
    #[serial]
    fn vault_binding_not_found_errors() {
        // Vault has no "nonexistent" service
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.nonexistent.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("missing".to_string(), entry);

        // Should error because value is None with no default
        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err(), "Should error on missing vault credential");
    }

    #[test]
    #[serial]
    fn vault_binding_with_default() {
        // When vault credential is missing, the ?? default should apply
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.missing_service.api_key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: Some(json!("fallback-key")),
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("key"), Some(&json!("fallback-key")));
    }

    #[test]
    #[serial]
    fn vault_binding_is_redacted_in_traces() {
        // Vault-sourced bindings must be redacted in to_value_redacted()
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());
        vault.set("anthropic", "sk-ant-secret").unwrap();

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.anthropic.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("api_key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // The redacted version should not contain the secret
        let redacted = bindings.to_value_redacted();
        let redacted_map = redacted.as_object().unwrap();
        let val = redacted_map.get("api_key").unwrap();
        assert_eq!(
            val,
            &json!("[REDACTED:$env]"),
            "Vault-sourced binding should be redacted"
        );
    }

    #[test]
    #[serial]
    fn vault_binding_simple_key_via_key_field() {
        // A simple Key("sk-ant-test") is accessible as $vault.anthropic.key
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());
        vault.set("anthropic", "sk-ant-test-12345").unwrap();

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.anthropic.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("api_key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("api_key"), Some(&json!("sk-ant-test-12345")));
    }

    #[test]
    fn vault_binding_no_vault_configured() {
        // When no vault is set, $vault bindings should get None → fall through to default
        let store = MockBindingStore::new();

        let path = BindingPath::parse("$vault.stripe.secret").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: Some(json!("no-vault-fallback")),
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("secret".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(
            bindings.get("secret"),
            Some(&json!("no-vault-fallback")),
            "When no vault configured, should fall through to default"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // BUG-038: WithEntry missing source with | default() transform
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_entry_missing_env_with_pipe_default() {
        // $env.NONEXISTENT | default("fallback") should return "fallback"
        let store = MockBindingStore::new();

        let entry = WithEntry {
            source: BindingPath {
                source: BindingSource::Env(Arc::from("__NIKA_TEST_NONEXISTENT_038__")),
                segments: vec![],
            },
            binding_type: crate::binding::types::BindingType::Any,
            default: None,
            lazy: false,
            transform: Some(
                nika_core::binding::TransformExpr::parse("default(\"fallback\")").unwrap(),
            ),
        };

        let result = resolve_with_entry(&entry, "data", &store);
        assert_eq!(result.unwrap(), json!("fallback"));
    }

    #[test]
    fn with_entry_missing_env_no_default_errors() {
        // $env.NONEXISTENT without default should error
        let store = MockBindingStore::new();

        let entry = WithEntry {
            source: BindingPath {
                source: BindingSource::Env(Arc::from("__NIKA_TEST_NONEXISTENT_038__")),
                segments: vec![],
            },
            binding_type: crate::binding::types::BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };

        let result = resolve_with_entry(&entry, "data", &store);
        assert!(result.is_err());
    }
}
