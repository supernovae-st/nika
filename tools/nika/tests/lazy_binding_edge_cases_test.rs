//! Lazy Binding Edge Cases Tests
//!
//! HIGH priority edge case tests for lazy binding resolution.
//! Tests error handling for problematic lazy binding patterns.
//!
//! See: docs/plans/tui-gap-remediation-v2.md

use nika::binding::{ResolvedBindings, UseEntry, WiringSpec};
use nika::store::DataStore;
use pretty_assertions::assert_eq;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: Missing Upstream Task
// ═══════════════════════════════════════════════════════════════════════════
//
// Scenario: A lazy binding references a task that doesn't exist in the workflow.
// Expected: Clear error message with the missing task ID when resolution is attempted.

#[test]
fn test_lazy_missing_upstream_task_defers_error() {
    // Arrange: Create a lazy binding to a non-existent task
    let store = DataStore::new();
    // Note: "nonexistent_task" is never added to the store

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "missing".to_string(),
        UseEntry::new_lazy("nonexistent_task.result"),
    );

    // Act: Create bindings - this should succeed (lazy defers resolution)
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);

    // Assert: Binding creation succeeds because lazy bindings defer resolution
    assert!(
        bindings.is_ok(),
        "Lazy binding creation should succeed even with missing upstream task"
    );

    let bindings = bindings.unwrap();
    assert!(
        bindings.is_lazy("missing"),
        "Binding should be marked as lazy (pending)"
    );
}

#[test]
fn test_lazy_missing_upstream_task_error_on_access() {
    // Arrange: Create a lazy binding to a non-existent task (no default)
    let store = DataStore::new();
    // "missing_task" never exists in store

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "data".to_string(),
        UseEntry::new_lazy("missing_task.output"),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Act: Attempt to resolve the lazy binding
    let result = bindings.get_resolved("data", &store);

    // Assert: Error should be clear about what's missing
    assert!(result.is_err(), "Resolution should fail for missing task");

    let error = result.unwrap_err();
    let error_msg = error.to_string();

    // Error should contain NIKA-052 (PathNotFound) and mention the path
    assert!(
        error_msg.contains("NIKA-052"),
        "Error should have code NIKA-052, got: {error_msg}"
    );
    assert!(
        error_msg.contains("missing_task.output"),
        "Error should mention the missing path, got: {error_msg}"
    );
}

#[test]
fn test_lazy_missing_task_with_default_uses_fallback() {
    // Arrange: Lazy binding to missing task WITH a default value
    let store = DataStore::new();
    // "optional_task" doesn't exist

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "optional".to_string(),
        UseEntry::lazy_with_default("optional_task.value", serde_json::json!("fallback_value")),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Act: Resolve the lazy binding
    let result = bindings.get_resolved("optional", &store);

    // Assert: Should use the default value instead of error
    assert!(
        result.is_ok(),
        "Resolution should succeed with default value"
    );
    assert_eq!(
        result.unwrap(),
        serde_json::json!("fallback_value"),
        "Should return the default value when task is missing"
    );
}

#[test]
fn test_lazy_missing_nested_path_clear_error() {
    // Arrange: Task exists but nested path doesn't
    let store = DataStore::new();
    store.insert(
        std::sync::Arc::from("task1"),
        nika::store::TaskResult::success(
            serde_json::json!({"data": {"existing": "value"}}),
            std::time::Duration::from_secs(1),
        ),
    );

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "missing_field".to_string(),
        UseEntry::new_lazy("task1.data.nonexistent.deeply.nested"),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Act: Attempt to resolve
    let result = bindings.get_resolved("missing_field", &store);

    // Assert: Error should indicate path not found
    assert!(result.is_err(), "Resolution should fail for missing path");

    let error_msg = result.unwrap_err().to_string();
    // Should indicate the path resolution failed
    assert!(
        error_msg.contains("NIKA-052") || error_msg.contains("not found"),
        "Error should indicate path not found, got: {error_msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Circular Dependency Detection (Lazy Bindings)
// ═══════════════════════════════════════════════════════════════════════════
//
// Scenario: Circular lazy bindings where resolution would cause infinite loop.
// Note: This is different from DAG cycle detection - these are data dependencies.
//
// Pattern: Task A uses lazy binding from Task B, Task B uses lazy binding from Task A.
// When both tasks complete, resolving either binding should work (no actual cycle).
// The "cycle" would only be problematic if we tried to resolve before any task runs.

#[test]
fn test_lazy_circular_pattern_no_deadlock_after_execution() {
    // Arrange: Two tasks with lazy cross-references (valid pattern)
    //
    // This pattern is VALID in lazy bindings:
    // - Task A references lazy(B.result)
    // - Task B references lazy(A.result)
    //
    // After both tasks complete, resolution works fine.
    // The "circularity" is broken because lazy doesn't resolve at parse time.

    let store = DataStore::new();

    // Simulate: both tasks completed
    store.insert(
        std::sync::Arc::from("task_a"),
        nika::store::TaskResult::success(
            serde_json::json!({"result": "a_output"}),
            std::time::Duration::from_secs(1),
        ),
    );
    store.insert(
        std::sync::Arc::from("task_b"),
        nika::store::TaskResult::success(
            serde_json::json!({"result": "b_output"}),
            std::time::Duration::from_secs(1),
        ),
    );

    // Task A's bindings (references B)
    let mut wiring_a = WiringSpec::default();
    wiring_a.insert("from_b".to_string(), UseEntry::new_lazy("task_b.result"));

    // Task B's bindings (references A)
    let mut wiring_b = WiringSpec::default();
    wiring_b.insert("from_a".to_string(), UseEntry::new_lazy("task_a.result"));

    // Act: Create bindings for both
    let bindings_a = ResolvedBindings::from_wiring_spec(Some(&wiring_a), &store).unwrap();
    let bindings_b = ResolvedBindings::from_wiring_spec(Some(&wiring_b), &store).unwrap();

    // Assert: Both can resolve (no infinite loop because data is already there)
    let result_a = bindings_a.get_resolved("from_b", &store);
    let result_b = bindings_b.get_resolved("from_a", &store);

    assert!(
        result_a.is_ok(),
        "Task A should resolve B's output: {:?}",
        result_a
    );
    assert!(
        result_b.is_ok(),
        "Task B should resolve A's output: {:?}",
        result_b
    );

    assert_eq!(result_a.unwrap(), serde_json::json!("b_output")); // A reads from B
    assert_eq!(result_b.unwrap(), serde_json::json!("a_output")); // B reads from A
}

#[test]
fn test_lazy_circular_pattern_partial_execution_error() {
    // Arrange: Two tasks with lazy cross-references
    // Only task_a has completed, task_b hasn't run yet
    //
    // Expected: task_a can't resolve its binding from task_b
    // because task_b hasn't produced output yet.

    let store = DataStore::new();

    // Only task_a completed
    store.insert(
        std::sync::Arc::from("task_a"),
        nika::store::TaskResult::success(
            serde_json::json!({"result": "a_done"}),
            std::time::Duration::from_secs(1),
        ),
    );
    // task_b NOT in store (hasn't run)

    // Task A's bindings (references B)
    let mut wiring_a = WiringSpec::default();
    wiring_a.insert("from_b".to_string(), UseEntry::new_lazy("task_b.result"));

    // Task B's bindings (references A)
    let mut wiring_b = WiringSpec::default();
    wiring_b.insert("from_a".to_string(), UseEntry::new_lazy("task_a.result"));

    // Act: Create bindings
    let bindings_a = ResolvedBindings::from_wiring_spec(Some(&wiring_a), &store).unwrap();
    let bindings_b = ResolvedBindings::from_wiring_spec(Some(&wiring_b), &store).unwrap();

    // Assert: A's binding fails (B not ready), B's binding succeeds (A is ready)
    let result_a = bindings_a.get_resolved("from_b", &store);
    let result_b = bindings_b.get_resolved("from_a", &store);

    assert!(
        result_a.is_err(),
        "Task A's binding should fail - task_b hasn't run"
    );
    assert!(
        result_b.is_ok(),
        "Task B's binding should succeed - task_a has run"
    );

    // Verify error message is helpful
    let error_msg = result_a.unwrap_err().to_string();
    assert!(
        error_msg.contains("task_b") || error_msg.contains("not found"),
        "Error should mention missing task_b, got: {error_msg}"
    );
}

#[test]
fn test_lazy_self_reference_fails_gracefully() {
    // Arrange: Task references itself via lazy binding (pathological case)
    //
    // This is a degenerate pattern that could cause issues.
    // Even if task1 completes, it can't reference its own output
    // in a binding that's evaluated before the task runs.

    let store = DataStore::new();
    // Task hasn't run yet (no output in store)

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "self_ref".to_string(),
        UseEntry::new_lazy("task1.previous_result"),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Act: Try to resolve self-reference before task runs
    let result = bindings.get_resolved("self_ref", &store);

    // Assert: Should fail with clear error (not infinite loop)
    assert!(
        result.is_err(),
        "Self-reference should fail when task hasn't run"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("NIKA-052") || error_msg.contains("not found"),
        "Error should indicate path not found, got: {error_msg}"
    );
}

#[test]
fn test_lazy_self_reference_with_default_uses_fallback() {
    // Arrange: Self-reference pattern but with a default value
    let store = DataStore::new();
    // No task output yet

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "previous".to_string(),
        UseEntry::lazy_with_default("task1.last_run", serde_json::json!({"first_run": true})),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Act: Resolve with default
    let result = bindings.get_resolved("previous", &store);

    // Assert: Should use default gracefully
    assert!(result.is_ok(), "Should succeed with default value");
    assert_eq!(
        result.unwrap(),
        serde_json::json!({"first_run": true}),
        "Should return the default value"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Additional Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_lazy_binding_task_id_extraction() {
    // Verify task_id() correctly extracts the task ID from various paths
    let simple = UseEntry::new_lazy("task1");
    assert_eq!(simple.task_id(), "task1");

    let with_field = UseEntry::new_lazy("task1.result");
    assert_eq!(with_field.task_id(), "task1");

    let deeply_nested = UseEntry::new_lazy("task1.data.nested.field");
    assert_eq!(deeply_nested.task_id(), "task1");

    let with_array = UseEntry::new_lazy("task1.items[0].name");
    assert_eq!(with_array.task_id(), "task1");
}

#[test]
fn test_lazy_binding_empty_path_segment() {
    // Edge case: path with leading dot (malformed)
    let entry = UseEntry::new_lazy(".invalid");

    // task_id() should return empty string for malformed path
    assert_eq!(entry.task_id(), "");
}

#[test]
fn test_lazy_binding_binding_not_found_error() {
    // Try to resolve an alias that was never declared
    let store = DataStore::new();
    let bindings = ResolvedBindings::new();

    let result = bindings.get_resolved("undeclared_alias", &store);

    assert!(result.is_err(), "Should fail for undeclared alias");

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("NIKA-042") || error_msg.contains("not found"),
        "Error should indicate binding not found, got: {error_msg}"
    );
}

#[test]
fn test_lazy_and_eager_mixed_validation() {
    // Arrange: Mix of eager (fails immediately) and lazy (defers) bindings
    // If eager binding fails, entire from_wiring_spec should fail
    // even if lazy bindings would succeed

    let store = DataStore::new();
    // No tasks in store

    let mut wiring = WiringSpec::default();
    wiring.insert(
        "eager_missing".to_string(),
        UseEntry::new("missing_eager.value"), // Eager - will fail
    );
    wiring.insert(
        "lazy_missing".to_string(),
        UseEntry::new_lazy("missing_lazy.value"), // Lazy - would defer
    );

    // Act: Create bindings
    let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);

    // Assert: Should fail due to eager binding
    assert!(
        result.is_err(),
        "Should fail because eager binding can't resolve"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("NIKA-052") || error_msg.contains("missing_eager"),
        "Error should be about the eager binding, got: {error_msg}"
    );
}

#[test]
fn test_lazy_binding_multiple_resolution_calls() {
    // Verify that lazy bindings can be resolved multiple times
    // (each call re-evaluates from datastore)

    let store = DataStore::new();

    store.insert(
        std::sync::Arc::from("counter"),
        nika::store::TaskResult::success(
            serde_json::json!({"value": 1}),
            std::time::Duration::from_secs(1),
        ),
    );

    let mut wiring = WiringSpec::default();
    wiring.insert("count".to_string(), UseEntry::new_lazy("counter.value"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // First resolution
    let result1 = bindings.get_resolved("count", &store).unwrap();
    assert_eq!(result1, serde_json::json!(1));

    // Update the datastore (simulating task re-run)
    store.insert(
        std::sync::Arc::from("counter"),
        nika::store::TaskResult::success(
            serde_json::json!({"value": 2}),
            std::time::Duration::from_secs(1),
        ),
    );

    // Second resolution should see new value
    let result2 = bindings.get_resolved("count", &store).unwrap();
    assert_eq!(result2, serde_json::json!(2));
}

#[test]
fn test_lazy_binding_preserves_pending_state() {
    // Verify is_lazy() returns true even after attempted resolution

    let store = DataStore::new();

    let mut wiring = WiringSpec::default();
    wiring.insert("pending".to_string(), UseEntry::new_lazy("future.result"));

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    // Initially lazy
    assert!(bindings.is_lazy("pending"));

    // Try to resolve (will fail - task doesn't exist)
    let _ = bindings.get_resolved("pending", &store);

    // Should still be marked as lazy (Pending state)
    // Note: get_resolved doesn't mutate bindings, it returns resolved value
    assert!(
        bindings.is_lazy("pending"),
        "Binding should remain lazy after failed resolution"
    );
}
