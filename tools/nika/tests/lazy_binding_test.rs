//! Lazy Binding Tests (MVP 8 Phase 5)
//!
//! TDD: RED phase - these tests should FAIL until implementation is complete.
//!
//! Lazy bindings defer resolution until first access. This allows:
//! - Referencing outputs from tasks that haven't executed yet
//! - Breaking circular dependency detection false positives
//! - Supporting dynamic workflow patterns

use nika::binding::{ResolvedBindings, UseEntry, WiringSpec};
use nika::store::{DataStore, TaskResult};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// YAML Parsing Tests - Extended UseEntry syntax
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_use_entry_lazy_flag_default_false() {
    // Standard string syntax should have lazy=false by default
    let entry = UseEntry::new("task1.result");
    assert!(!entry.is_lazy());
}

#[test]
fn test_use_entry_lazy_flag_explicit() {
    // Extended syntax should allow lazy: true
    let entry = UseEntry::new_lazy("task1.result");
    assert!(entry.is_lazy());
}

#[test]
fn test_use_entry_lazy_with_default() {
    // Lazy binding with default value
    let entry = UseEntry::lazy_with_default("task1.result", json!("fallback"));
    assert!(entry.is_lazy());
    assert_eq!(entry.default, Some(json!("fallback")));
}

#[test]
fn test_yaml_parse_lazy_extended_syntax() {
    // Extended YAML syntax for lazy bindings
    // use:
    //   eager: task1.result
    //   lazy_one:
    //     path: task2.result
    //     lazy: true
    let yaml = r#"
eager: task1.result
lazy_one:
  path: task2.result
  lazy: true
"#;

    let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();

    // eager should be non-lazy
    let eager = wiring.get("eager").unwrap();
    assert!(!eager.is_lazy());

    // lazy_one should be lazy
    let lazy_one = wiring.get("lazy_one").unwrap();
    assert!(lazy_one.is_lazy());
    assert_eq!(lazy_one.path, "task2.result");
}

#[test]
fn test_yaml_parse_lazy_with_default() {
    let yaml = r#"
optional:
  path: missing.result
  lazy: true
  default: "fallback"
"#;

    let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();
    let optional = wiring.get("optional").unwrap();

    assert!(optional.is_lazy());
    assert_eq!(optional.path, "missing.result");
    assert_eq!(optional.default, Some(json!("fallback")));
}

// ═══════════════════════════════════════════════════════════════════════════
// Resolution Behavior Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lazy_binding_not_resolved_initially() {
    // Lazy bindings should NOT fail during from_wiring_spec
    // even if the source task output doesn't exist yet
    let store = DataStore::new();
    // Note: "future" task NOT in store

    let mut wiring = WiringSpec::default();
    wiring.insert("lazy_val".to_string(), UseEntry::new_lazy("future.result"));

    // This should succeed because lazy bindings defer resolution
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    assert!(bindings.is_ok());

    let bindings = bindings.unwrap();
    assert!(bindings.is_lazy("lazy_val"));
}

#[test]
fn test_lazy_binding_resolved_on_access() {
    let store = DataStore::new();
    // Initially empty - task1 hasn't run yet

    let mut wiring = WiringSpec::default();
    // Note: path "task1.result" expects task1 output to have a "result" field
    wiring.insert("lazy_val".to_string(), UseEntry::new_lazy("task1.result"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Initially lazy
    assert!(bindings.is_lazy("lazy_val"));

    // Now add task output (simulating task1 completing)
    // Output has a "result" field that the path will resolve
    store.insert(
        Arc::from("task1"),
        TaskResult::success(json!({"result": "hello"}), Duration::from_secs(1)),
    );

    // Resolve on access - should now work
    let value = bindings.get_resolved("lazy_val", &store).unwrap();
    assert_eq!(value, json!("hello"));
}

#[test]
fn test_lazy_binding_with_path() {
    let store = DataStore::new();

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "nested".to_string(),
        UseEntry::new_lazy("task1.data.value"),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Add task output
    store.insert(
        Arc::from("task1"),
        TaskResult::success(json!({"data": {"value": 42}}), Duration::from_secs(1)),
    );

    // Resolve nested path on access
    let value = bindings.get_resolved("nested", &store).unwrap();
    assert_eq!(value, json!(42));
}

#[test]
fn test_lazy_binding_with_default_on_missing() {
    let store = DataStore::new();
    // Task never runs

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "optional".to_string(),
        UseEntry::lazy_with_default("missing.result", json!("fallback")),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Resolve with default (task doesn't exist)
    let value = bindings.get_resolved("optional", &store).unwrap();
    assert_eq!(value, json!("fallback"));
}

#[test]
fn test_lazy_binding_with_default_on_null() {
    let store = DataStore::new();
    store.insert(
        Arc::from("task1"),
        TaskResult::success(json!(null), Duration::from_secs(1)),
    );

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "nullable".to_string(),
        UseEntry::lazy_with_default("task1", json!("default")),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Null value should use default
    let value = bindings.get_resolved("nullable", &store).unwrap();
    assert_eq!(value, json!("default"));
}

#[test]
fn test_lazy_binding_error_on_missing_no_default() {
    let store = DataStore::new();
    // Task never runs, no default

    let mut wiring = WiringSpec::default();
    wiring.insert("strict".to_string(), UseEntry::new_lazy("missing.result"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Should error on access when no default and task missing
    let result = bindings.get_resolved("strict", &store);
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// Mixed Eager and Lazy Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_mixed_eager_and_lazy_bindings() {
    let store = DataStore::new();

    // Add output for eager binding source
    store.insert(
        Arc::from("task1"),
        TaskResult::success(json!("eager_value"), Duration::from_secs(1)),
    );
    // task2 not in store (lazy binding target)

    let mut wiring = WiringSpec::default();
    wiring.insert("eager".to_string(), UseEntry::new("task1"));
    wiring.insert("lazy".to_string(), UseEntry::new_lazy("task2.result"));

    // Should succeed - eager resolved, lazy deferred
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Eager is resolved
    assert!(!bindings.is_lazy("eager"));
    assert_eq!(bindings.get("eager"), Some(&json!("eager_value")));

    // Lazy is pending
    assert!(bindings.is_lazy("lazy"));

    // Now add task2 output
    store.insert(
        Arc::from("task2"),
        TaskResult::success(json!({"result": "lazy_value"}), Duration::from_secs(1)),
    );

    // Lazy can now be resolved
    let value = bindings.get_resolved("lazy", &store).unwrap();
    assert_eq!(value, json!("lazy_value"));
}

#[test]
fn test_eager_binding_fails_on_missing() {
    let store = DataStore::new();
    // No tasks in store

    let mut wiring = WiringSpec::default();
    wiring.insert("eager".to_string(), UseEntry::new("missing.value"));

    // Eager binding should fail immediately
    let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// is_lazy() API Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_is_lazy_returns_false_for_resolved() {
    let store = DataStore::new();
    store.insert(
        Arc::from("task1"),
        TaskResult::success(json!("value"), Duration::from_secs(1)),
    );

    let mut wiring = WiringSpec::default();
    wiring.insert("resolved".to_string(), UseEntry::new("task1"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    assert!(!bindings.is_lazy("resolved"));
    assert!(!bindings.is_lazy("nonexistent")); // Non-existent is also not lazy
}

#[test]
fn test_is_lazy_returns_true_for_pending() {
    let store = DataStore::new();
    // No tasks

    let mut wiring = WiringSpec::default();
    wiring.insert("pending".to_string(), UseEntry::new_lazy("future.result"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    assert!(bindings.is_lazy("pending"));
}
