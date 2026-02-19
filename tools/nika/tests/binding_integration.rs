//! Integration tests for binding workflow
//!
//! Tests the full pipeline: YAML → WiringSpec → ResolvedBindings → template resolution

use nika::binding::{
    parse_use_entry, template_resolve, validate_task_id, ResolvedBindings, WiringSpec,
};
use nika::store::{DataStore, TaskResult};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// Full Workflow Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn full_workflow_simple_path() {
    // 1. Parse use: entry
    let entry = parse_use_entry("weather.summary").unwrap();
    assert_eq!(entry.path, "weather.summary");
    assert!(entry.default.is_none());

    // 2. Create wiring
    let mut wiring = WiringSpec::default();
    wiring.insert("forecast".to_string(), entry);

    // 3. Populate datastore with task output
    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(
            json!({"summary": "Sunny", "temp": 25}),
            Duration::from_secs(1),
        ),
    );

    // 4. Resolve bindings
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(bindings.get("forecast"), Some(&json!("Sunny")));

    // 5. Template resolution
    let template = "Weather: {{use.forecast}}";
    let result = template_resolve(template, &bindings).unwrap();
    assert_eq!(result, "Weather: Sunny");
}

#[test]
fn full_workflow_with_default() {
    // 1. Parse with default
    let entry = parse_use_entry(r#"weather.rating ?? 5"#).unwrap();
    assert_eq!(entry.path, "weather.rating");
    assert_eq!(entry.default, Some(json!(5)));

    // 2. Wiring
    let mut wiring = WiringSpec::default();
    wiring.insert("rating".to_string(), entry);

    // 3. Datastore WITHOUT rating field
    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
    );

    // 4. Resolve - should use default
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(bindings.get("rating"), Some(&json!(5)));

    // 5. Template
    let result = template_resolve("Rating: {{use.rating}}/5", &bindings).unwrap();
    assert_eq!(result, "Rating: 5/5");
}

#[test]
fn full_workflow_nested_path() {
    // Deep nesting: flights.cheapest.price
    let entry = parse_use_entry("flights.cheapest.price").unwrap();

    let mut wiring = WiringSpec::default();
    wiring.insert("price".to_string(), entry);

    let store = DataStore::new();
    store.insert(
        Arc::from("flights"),
        TaskResult::success(
            json!({"cheapest": {"price": 89, "airline": "Ryanair"}}),
            Duration::from_secs(1),
        ),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(bindings.get("price"), Some(&json!(89)));

    let result = template_resolve("Price: ${{use.price}}", &bindings).unwrap();
    assert_eq!(result, "Price: $89");
}

#[test]
fn full_workflow_multiple_aliases() {
    let mut wiring = WiringSpec::default();
    wiring.insert("city".to_string(), parse_use_entry("weather.city").unwrap());
    wiring.insert(
        "temp".to_string(),
        parse_use_entry("weather.temp ?? 20").unwrap(),
    );
    wiring.insert(
        "price".to_string(),
        parse_use_entry("flights.cheapest.price").unwrap(),
    );

    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(json!({"city": "Paris", "temp": 25}), Duration::from_secs(1)),
    );
    store.insert(
        Arc::from("flights"),
        TaskResult::success(json!({"cheapest": {"price": 89}}), Duration::from_secs(1)),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    let template = "Travel to {{use.city}}: {{use.temp}}C, ${{use.price}}";
    let result = template_resolve(template, &bindings).unwrap();
    assert_eq!(result, "Travel to Paris: 25C, $89");
}

#[test]
fn full_workflow_string_default() {
    let entry = parse_use_entry(r#"user.name ?? "Anonymous""#).unwrap();
    assert_eq!(entry.default, Some(json!("Anonymous")));

    let mut wiring = WiringSpec::default();
    wiring.insert("name".to_string(), entry);

    // No user task in store
    let store = DataStore::new();

    // Should error without default
    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    // Actually, this should use the default because we have one
    // But wait - the task doesn't exist, so it should error even with default?
    // Let me check the resolve logic...
    // Actually with default, if task not found, it should use default
    assert!(bindings.is_ok());
    assert_eq!(bindings.unwrap().get("name"), Some(&json!("Anonymous")));
}

#[test]
fn full_workflow_object_default() {
    let entry = parse_use_entry(r#"settings ?? {"debug": false}"#).unwrap();
    assert_eq!(entry.default, Some(json!({"debug": false})));

    let mut wiring = WiringSpec::default();
    wiring.insert("config".to_string(), entry);

    let store = DataStore::new();

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(bindings.get("config"), Some(&json!({"debug": false})));
}

// ═══════════════════════════════════════════════════════════════
// Error Propagation Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn error_task_not_found_no_default() {
    let entry = parse_use_entry("missing.data").unwrap();

    let mut wiring = WiringSpec::default();
    wiring.insert("x".to_string(), entry);

    let store = DataStore::new();

    let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("NIKA-052") || err.contains("not found"));
}

#[test]
fn error_path_not_found_no_default() {
    let entry = parse_use_entry("weather.nonexistent").unwrap();

    let mut wiring = WiringSpec::default();
    wiring.insert("x".to_string(), entry);

    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
    );

    let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    assert!(result.is_err());
}

#[test]
fn error_null_value_no_default() {
    let entry = parse_use_entry("weather.temp").unwrap();

    let mut wiring = WiringSpec::default();
    wiring.insert("temp".to_string(), entry);

    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(json!({"temp": null}), Duration::from_secs(1)),
    );

    let result = ResolvedBindings::from_wiring_spec(Some(&wiring), &store);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NIKA-072"));
}

#[test]
fn error_template_unknown_alias() {
    let bindings = ResolvedBindings::new();

    let result = template_resolve("Hello {{use.unknown}}", &bindings);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    // Template errors for unknown aliases include the alias name
    assert!(err_msg.contains("unknown") || err_msg.contains("not resolved"));
}

// ═══════════════════════════════════════════════════════════════
// Task ID Validation Integration
// ═══════════════════════════════════════════════════════════════

#[test]
fn task_id_validation_in_workflow() {
    // Valid task IDs work
    assert!(validate_task_id("weather").is_ok());
    assert!(validate_task_id("get_data").is_ok());
    assert!(validate_task_id("task123").is_ok());

    // Invalid task IDs fail with NIKA-055
    let err = validate_task_id("fetch-api").unwrap_err();
    assert!(err.to_string().contains("NIKA-055"));

    let err = validate_task_id("myTask").unwrap_err();
    assert!(err.to_string().contains("NIKA-055"));

    let err = validate_task_id("weather.api").unwrap_err();
    assert!(err.to_string().contains("NIKA-055"));
}

// ═══════════════════════════════════════════════════════════════
// YAML Deserialization Integration
// ═══════════════════════════════════════════════════════════════

#[test]
fn yaml_to_bindings_full_workflow() {
    // Simulate YAML parsing
    let yaml = r#"
forecast: weather.summary
temp: weather.temp ?? 20
name: 'user.name ?? "Guest"'
"#;

    let wiring: WiringSpec = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(wiring.len(), 3);

    // Verify parsed entries
    let forecast = wiring.get("forecast").unwrap();
    assert_eq!(forecast.path, "weather.summary");
    assert!(forecast.default.is_none());

    let temp = wiring.get("temp").unwrap();
    assert_eq!(temp.path, "weather.temp");
    assert_eq!(temp.default, Some(json!(20)));

    let name = wiring.get("name").unwrap();
    assert_eq!(name.path, "user.name");
    assert_eq!(name.default, Some(json!("Guest")));

    // Now resolve against datastore
    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(
            json!({"summary": "Rainy", "temp": 15}),
            Duration::from_secs(1),
        ),
    );
    store.insert(
        Arc::from("user"),
        TaskResult::success(json!({"name": "Alice"}), Duration::from_secs(1)),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();

    assert_eq!(bindings.get("forecast"), Some(&json!("Rainy")));
    assert_eq!(bindings.get("temp"), Some(&json!(15)));
    assert_eq!(bindings.get("name"), Some(&json!("Alice")));
}

// ═══════════════════════════════════════════════════════════════
// Edge Cases
// ═══════════════════════════════════════════════════════════════

#[test]
fn edge_case_empty_template() {
    let bindings = ResolvedBindings::new();
    let result = template_resolve("", &bindings).unwrap();
    assert_eq!(result, "");
}

#[test]
fn edge_case_no_templates() {
    let bindings = ResolvedBindings::new();
    let result = template_resolve("Hello world!", &bindings).unwrap();
    assert_eq!(result, "Hello world!");
}

#[test]
fn edge_case_entire_task_output() {
    let entry = parse_use_entry("weather").unwrap();
    assert_eq!(entry.path, "weather");

    let mut wiring = WiringSpec::default();
    wiring.insert("data".to_string(), entry);

    let store = DataStore::new();
    store.insert(
        Arc::from("weather"),
        TaskResult::success(
            json!({"summary": "Sunny", "temp": 25}),
            Duration::from_secs(1),
        ),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(
        bindings.get("data"),
        Some(&json!({"summary": "Sunny", "temp": 25}))
    );
}

#[test]
fn edge_case_array_index() {
    let entry = parse_use_entry("results.items[0].name").unwrap();

    let mut wiring = WiringSpec::default();
    wiring.insert("first".to_string(), entry);

    let store = DataStore::new();
    store.insert(
        Arc::from("results"),
        TaskResult::success(
            json!({"items": [{"name": "Alpha"}, {"name": "Beta"}]}),
            Duration::from_secs(1),
        ),
    );

    let bindings = ResolvedBindings::from_wiring_spec(Some(&wiring), &store).unwrap();
    assert_eq!(bindings.get("first"), Some(&json!("Alpha")));
}

#[test]
fn edge_case_default_with_special_chars() {
    // Default containing ?? inside quotes
    let entry = parse_use_entry(r#"x ?? "What?? Really??""#).unwrap();
    assert_eq!(entry.default, Some(json!("What?? Really??")));
}
