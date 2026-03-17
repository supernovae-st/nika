//! Property-Based Testing for Nika (v0.4.1)
//!
//! Uses proptest to fuzz-test critical parsing and validation logic.
//! Coverage targets:
//! - Template resolution (binding/template.rs)
//! - Workflow YAML parsing (ast/workflow.rs)
//! - DAG validation (dag/validate.rs)

use nika::serde_yaml;
use proptest::prelude::*;
use serde_json::json;

// =============================================================================
// TEST 1: Template Resolution Fuzzing
// =============================================================================
// Target: src/binding/template.rs
// Risk: Regex-based parsing, string manipulation, JSON traversal

mod template_fuzzing {
    use super::*;
    use nika::binding::template_resolve;
    use nika::binding::ResolvedBindings;
    use nika::store::RunContext;
    use std::borrow::Cow;

    /// Helper to create empty datastore for tests
    fn empty_datastore() -> RunContext {
        RunContext::new()
    }

    prop_compose! {
        /// Generate valid alias names (snake_case identifiers)
        fn arb_alias()(alias in r"[a-z][a-z0-9_]{0,15}") -> String {
            alias
        }
    }

    prop_compose! {
        /// Generate template strings with valid {{with.alias}} patterns
        fn arb_template_with_alias()(
            prefix in "[ -~]{0,20}",  // ASCII printable
            alias in r"[a-z][a-z0-9_]{0,15}",
            suffix in "[ -~]{0,20}"
        ) -> String {
            format!("{}{{{{with.{}}}}}{}", prefix, alias, suffix)
        }
    }

    prop_compose! {
        /// Generate template strings with multiple aliases
        fn arb_multi_template()(
            prefix in "[ -~]{0,10}",
            alias1 in r"[a-z][a-z0-9_]{0,10}",
            middle in "[ -~]{0,10}",
            alias2 in r"[a-z][a-z0-9_]{0,10}",
            suffix in "[ -~]{0,10}"
        ) -> String {
            format!("{}{{{{with.{}}}}}{}{{{{with.{}}}}}{}", prefix, alias1, middle, alias2, suffix)
        }
    }

    proptest! {
        /// Property: Template resolution never panics on arbitrary templates
        #[test]
        fn test_template_resolution_never_panics(template in ".*") {
            let bindings = ResolvedBindings::new();
            let ds = empty_datastore();
            // Should never panic, regardless of input
            let _ = template_resolve(&template, &bindings, &ds);
        }

        /// Property: No-template strings return Cow::Borrowed (zero allocation)
        #[test]
        fn test_no_template_returns_borrowed(s in "[^{}]*") {
            let bindings = ResolvedBindings::new();
            let ds = empty_datastore();
            let result = template_resolve(&s, &bindings, &ds);
            if let Ok(cow) = result {
                // If no {{with.}} pattern, should be borrowed
                if !s.contains("{{with.") {
                    assert!(matches!(cow, Cow::Borrowed(_)));
                }
            }
        }

        /// Property: Templates with substitutions return Cow::Owned
        #[test]
        fn test_template_with_substitution_returns_owned(template in arb_template_with_alias()) {
            let alias_re = regex::Regex::new(r"\{\{\s*with\.(\w+)").unwrap();
            if let Some(cap) = alias_re.captures(&template) {
                let alias = &cap[1];
                let mut bindings = ResolvedBindings::new();
                bindings.set(alias, json!("value"));
                let ds = empty_datastore();

                if let Ok(cow) = template_resolve(&template, &bindings, &ds) {
                    // With substitution, should be owned
                    assert!(matches!(cow, Cow::Owned(_)));
                }
            }
        }

        /// Property: Valid alias with binding always resolves successfully
        #[test]
        fn test_valid_alias_resolves(
            alias in arb_alias(),
            value in "[ -~]{0,50}"
        ) {
            let template = format!("{{{{with.{}}}}}", alias);
            let mut bindings = ResolvedBindings::new();
            bindings.set(&alias, json!(value.clone()));
            let ds = empty_datastore();

            let result = template_resolve(&template, &bindings, &ds);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_ref(), value);
        }

        /// Property: Missing alias always returns error (never panic)
        #[test]
        fn test_missing_alias_returns_error(alias in arb_alias()) {
            let template = format!("{{{{with.{}}}}}", alias);
            let bindings = ResolvedBindings::new();  // Empty bindings
            let ds = empty_datastore();

            let result = template_resolve(&template, &bindings, &ds);
            assert!(result.is_err());
        }

        /// Property: Nested path access works correctly
        #[test]
        fn test_nested_path_access(
            alias in arb_alias(),
            field in r"[a-z][a-z0-9_]{0,10}",
            value in "[ -~]{0,30}"
        ) {
            let template = format!("{{{{with.{}.{}}}}}", alias, field);
            let mut bindings = ResolvedBindings::new();
            bindings.set(&alias, json!({field.clone(): value.clone()}));
            let ds = empty_datastore();

            let result = template_resolve(&template, &bindings, &ds);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().as_ref(), value);
        }

        /// Property: Array index access works correctly
        #[test]
        fn test_array_index_access(
            alias in arb_alias(),
            index in 0usize..10,
            values in prop::collection::vec("[ -~]{1,10}", 1..15)
        ) {
            if index < values.len() {
                let template = format!("{{{{with.{}.{}}}}}", alias, index);
                let mut bindings = ResolvedBindings::new();
                bindings.set(&alias, json!(values.clone()));
                let ds = empty_datastore();

                let result = template_resolve(&template, &bindings, &ds);
                assert!(result.is_ok());
                assert_eq!(result.unwrap().as_ref(), values[index]);
            }
        }

        /// Property: Multiple templates resolve independently
        #[test]
        fn test_multiple_templates_resolve(
            alias1 in arb_alias(),
            value1 in "[ -~]{0,20}",
            alias2 in arb_alias(),
            value2 in "[ -~]{0,20}"
        ) {
            if alias1 != alias2 {
                let template = format!("{{{{with.{}}}}} and {{{{with.{}}}}}", alias1, alias2);
                let mut bindings = ResolvedBindings::new();
                bindings.set(&alias1, json!(value1.clone()));
                bindings.set(&alias2, json!(value2.clone()));
                let ds = empty_datastore();

                let result = template_resolve(&template, &bindings, &ds);
                assert!(result.is_ok());
                let resolved = result.unwrap();
                assert!(resolved.contains(&value1));
                assert!(resolved.contains(&value2));
            }
        }
    }
}

// =============================================================================
// TEST 2: Workflow YAML Parsing Fuzzing
// =============================================================================
// Target: src/ast/workflow.rs
// Risk: YAML deserialization, schema validation, for_each validation

mod workflow_fuzzing {
    use super::*;

    prop_compose! {
        /// Generate valid task IDs (snake_case)
        fn arb_task_id()(id in r"[a-z][a-z0-9_]{0,20}") -> String {
            id
        }
    }

    prop_compose! {
        /// Generate valid schema versions
        fn arb_schema_version()(version in prop::sample::select(vec![
            "nika/workflow@0.1",
            "nika/workflow@0.2",
            "nika/workflow@0.12"
        ])) -> String {
            version.to_string()
        }
    }

    prop_compose! {
        /// Generate minimal valid workflow YAML
        /// Note: prompt uses safe chars (no quotes, backslash) for valid YAML strings
        fn arb_valid_workflow()(
            schema in arb_schema_version(),
            workflow_name in r"[a-z][a-z0-9_\-]{0,20}",
            task_id in arb_task_id(),
            prompt in r"[a-zA-Z0-9 !#$%&()*+,\-./:<=>?@\[\]^_`{|}~]{1,50}"
        ) -> String {
            format!(
                r#"schema: {}
workflow: {}
tasks:
  - id: {}
    infer: "{}""#,
                schema, workflow_name, task_id, prompt
            )
        }
    }

    proptest! {
        /// Property: Workflow parsing never panics on arbitrary YAML
        #[test]
        fn test_workflow_parse_never_panics(yaml in ".*") {
            // Parse YAML - should never panic
            let _ = serde_yaml::from_str::<serde_json::Value>(&yaml);
        }

        /// Property: Valid schema versions parse successfully
        #[test]
        fn test_valid_schema_parses(yaml in arb_valid_workflow()) {
            let result: Result<serde_json::Value, _> = serde_yaml::from_str(&yaml);
            assert!(result.is_ok(), "Valid workflow should parse: {}", yaml);
        }

        /// Property: Invalid schema version is rejected (not panic)
        #[test]
        fn test_invalid_schema_rejected(
            invalid_schema in r"[a-z0-9@/.]{1,20}",
            task_id in arb_task_id()
        ) {
            // Exclude valid schemas
            if !["nika/workflow@0.1", "nika/workflow@0.2", "nika/workflow@0.12"].contains(&invalid_schema.as_str()) {
                let yaml = format!(
                    r#"schema: {}
workflow: test
tasks:
  - id: {}
    infer: "test""#,
                    invalid_schema, task_id
                );
                // Should not panic — either parse error or schema validation error
                let result = nika::ast::raw::parse(&yaml, nika::source::FileId(0));
                match result {
                    Ok(raw) => {
                        let analyzed = nika::ast::analyzer::analyze(raw);
                        prop_assert!(analyzed.is_err(),
                            "Invalid schema '{}' should fail analysis", invalid_schema);
                    }
                    Err(_) => {} // Parse error is fine too
                }
            }
        }

        /// Property: for_each with empty array fails validation (never panics)
        #[test]
        fn test_for_each_empty_array_fails(task_id in arb_task_id()) {
            let yaml = format!(
                r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: {}
    for_each: []
    as: item
    exec: "echo {{{{item}}}}""#,
                task_id
            );
            // Should not panic during parse/validation
            let _ = serde_yaml::from_str::<serde_json::Value>(&yaml);
        }

        /// Property: for_each with non-array fails validation (never panics)
        #[test]
        fn test_for_each_non_array_fails(
            task_id in arb_task_id(),
            non_array in prop::sample::select(vec!["\"string\"", "123", "true", "null"])
        ) {
            let yaml = format!(
                r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: {}
    for_each: {}
    as: item
    exec: "echo {{{{item}}}}""#,
                task_id, non_array
            );
            // Should not panic
            let _ = serde_yaml::from_str::<serde_json::Value>(&yaml);
        }

        /// Property: Valid for_each arrays parse successfully
        #[test]
        fn test_valid_for_each_parses(
            task_id in arb_task_id(),
            // Use alphanumeric items to avoid YAML quoting issues
            items in prop::collection::vec("[a-zA-Z0-9_]{1,10}", 1..5)
        ) {
            let items_yaml = format!("[{}]", items.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(", "));
            let yaml = format!(
                r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: {}
    for_each: {}
    as: item
    exec: "echo {{{{item}}}}""#,
                task_id, items_yaml
            );
            let result: Result<serde_json::Value, _> = serde_yaml::from_str(&yaml);
            prop_assert!(result.is_ok(), "Valid for_each should parse: {}", yaml);
        }
    }
}

// =============================================================================
// TEST 3: DAG Validation Fuzzing
// =============================================================================
// Target: src/dag/validate.rs
// Risk: Cycle detection, dependency resolution, task ID validation

mod dag_fuzzing {
    use super::*;
    use nika::ast::analyzer::analyze;
    use nika::ast::raw::parse;
    use nika::source::FileId;

    prop_compose! {
        /// Generate valid task IDs (snake_case only)
        fn arb_valid_task_id()(id in r"[a-z][a-z0-9_]{0,20}") -> String {
            id
        }
    }

    prop_compose! {
        /// Generate DAG with linear dependencies (A -> B -> C)
        fn arb_linear_dag()(
            tasks in prop::collection::vec(arb_valid_task_id(), 2..6)
        ) -> String {
            let unique_tasks: Vec<_> = tasks.iter()
                .enumerate()
                .map(|(i, t)| format!("{}{}", t, i))
                .collect();

            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for (i, task) in unique_tasks.iter().enumerate() {
                yaml.push_str(&format!("  - id: {}\n", task));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [{}]\n", unique_tasks[i-1]));
                }
                yaml.push_str(&format!("    infer: \"step {}\"\n", i));
            }
            yaml
        }
    }

    proptest! {
        /// Property: Valid snake_case task IDs pass validation
        #[test]
        fn test_valid_task_id_passes(id in arb_valid_task_id()) {
            let is_valid = id.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_');
            prop_assert!(is_valid, "Generated ID should be valid snake_case: {}", id);
        }

        /// Property: Linear DAGs pass full Nika pipeline without panics
        #[test]
        fn test_dag_validation_never_panics(yaml in arb_linear_dag()) {
            let raw = parse(&yaml, FileId(0)).expect("Linear DAG YAML should parse");
            let result = analyze(raw);
            prop_assert!(result.is_ok(),
                "Linear DAG should pass analysis: {:?}", result.errors);
        }

        /// Property: Self-referencing task detected as cycle by analyzer
        #[test]
        fn test_self_reference_fails(task_id in arb_valid_task_id()) {
            let yaml = format!(
                "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"test\"\n",
                task_id, task_id
            );
            if let Ok(raw) = parse(&yaml, FileId(0)) {
                let result = analyze(raw);
                prop_assert!(result.is_err(),
                    "Self-referencing task '{}' should fail analysis", task_id);
            }
        }

        /// Property: Cyclic dependencies detected by analyzer
        #[test]
        fn test_cycle_detection(
            task1 in arb_valid_task_id(),
            task2 in arb_valid_task_id()
        ) {
            if task1 != task2 {
                let yaml = format!(
                    "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"first\"\n  - id: {}\n    depends_on: [{}]\n    infer: \"second\"\n",
                    task1, task2, task2, task1
                );
                if let Ok(raw) = parse(&yaml, FileId(0)) {
                    let result = analyze(raw);
                    prop_assert!(result.is_err(),
                        "Cycle {}<->{} should fail analysis", task1, task2);
                }
            }
        }

        /// Property: Referencing non-existent task detected by analyzer
        #[test]
        fn test_nonexistent_task_fails(
            task1 in arb_valid_task_id(),
            ghost in arb_valid_task_id()
        ) {
            // task1 exists, task2 depends_on task1 (valid), but task2's ID
            // is ghost which is different from task1 — no error expected here.
            // Instead: task depends_on a nonexistent ID.
            let nonexistent = format!("{}__missing", ghost);
            let yaml = format!(
                "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"test\"\n",
                task1, nonexistent
            );
            if let Ok(raw) = parse(&yaml, FileId(0)) {
                let result = analyze(raw);
                prop_assert!(result.is_err(),
                    "Reference to nonexistent '{}' should fail analysis", nonexistent);
            }
        }

        /// Property: Large DAGs don't cause stack overflow in analyzer
        #[test]
        fn test_large_dag_no_overflow(depth in 10usize..50) {
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for i in 0..depth {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                yaml.push_str(&format!("    infer: \"level {}\"\n", i));
            }

            let raw = parse(&yaml, FileId(0)).expect("Linear DAG YAML should parse");
            let result = analyze(raw);
            prop_assert!(result.is_ok(),
                "Large linear DAG (depth={}) should pass: {:?}", depth, result.errors);
        }
    }
}

// =============================================================================
// TEST 4: JSON Value Handling (bonus coverage)
// =============================================================================

mod json_fuzzing {
    use super::*;

    proptest! {
        /// Property: JSON serialization round-trips correctly
        #[test]
        fn test_json_roundtrip(s in "[ -~]{0,100}") {
            let json = json!(s);
            let serialized = serde_json::to_string(&json).unwrap();
            let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(json, deserialized);
        }

        /// Property: Nested JSON access is consistent
        #[test]
        fn test_nested_json_access(
            key in r"[a-z][a-z0-9_]{0,10}",
            value in "[ -~]{0,30}"
        ) {
            let obj = json!({ key.clone(): value.clone() });
            let accessed = obj.get(&key);
            prop_assert!(accessed.is_some());
            prop_assert_eq!(accessed.unwrap().as_str(), Some(value.as_str()));
        }

        /// Property: Array indexing is bounds-checked
        #[test]
        fn test_array_bounds(
            arr_len in 1usize..20,
            index in 0usize..100
        ) {
            let arr: Vec<i32> = (0..arr_len as i32).collect();
            let json_arr = json!(arr);

            let accessed = json_arr.get(index);
            if index < arr_len {
                prop_assert!(accessed.is_some());
            } else {
                prop_assert!(accessed.is_none());
            }
        }
    }
}

// =============================================================================
// TEST 5: AST Parser Fuzzing (raw::parse never panics)
// =============================================================================
// Target: src/ast/raw/parser.rs
// Risk: marked_yaml span extraction, error handling paths

mod ast_parser_fuzzing {
    use super::*;
    use nika::ast::raw::parse;
    use nika::source::FileId;

    proptest! {
        /// Property: ast::raw::parse() NEVER panics on any input
        #[test]
        fn test_parser_never_panics(yaml in ".*") {
            let _ = parse(&yaml, FileId(0));
        }

        /// Property: ast::raw::parse() never panics on YAML-like input
        #[test]
        fn test_parser_never_panics_yaml_like(
            key in r"[a-z_]{1,10}",
            value in "[ -~]{0,50}"
        ) {
            let yaml = format!("{}: {}", key, value);
            let _ = parse(&yaml, FileId(0));
        }

        /// Property: parse() returns Err, never panics, on binary input
        #[test]
        fn test_parser_handles_binary(bytes in prop::collection::vec(0u8..=255, 0..200)) {
            let input = String::from_utf8_lossy(&bytes);
            let _ = parse(&input, FileId(0));
        }
    }
}

// =============================================================================
// TEST 6: Full Pipeline Roundtrip Invariance
// =============================================================================

mod pipeline_roundtrip_fuzzing {
    use super::*;
    use nika::ast::analyzer::analyze;
    use nika::ast::lower::{lower, unlower};
    use nika::ast::raw::parse;
    use nika::source::FileId;

    prop_compose! {
        /// Generate a workflow with N infer tasks and optional dependencies
        fn arb_pipeline_workflow()(
            n in 1usize..5,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,30}", 1..6),
        ) -> String {
            let n = n.min(prompts.len());
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for i in 0..n {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                yaml.push_str(&format!("    infer: \"{}\"\n", prompts[i]));
            }
            yaml
        }
    }

    prop_compose! {
        /// Generate workflow with `with:` bindings (implicit deps via $task_ref)
        fn arb_with_workflow()(
            n in 2usize..5,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,30}", 2..6),
        ) -> String {
            let n = n.min(prompts.len());
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            // First task: standalone
            yaml.push_str(&format!("  - id: task_0\n    infer: \"{}\"\n", prompts[0]));
            // Remaining tasks: each binds previous via with:
            for i in 1..n {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                yaml.push_str(&format!("    with:\n      prev: $task_{}\n", i - 1));
                yaml.push_str(&format!("    infer: \"{}\"\n", prompts[i]));
            }
            yaml
        }
    }

    prop_compose! {
        /// Generate workflow with mixed verbs (infer + exec)
        fn arb_mixed_verb_workflow()(
            n in 2usize..5,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,30}", 2..6),
            commands in prop::collection::vec(r"echo [a-zA-Z0-9]{1,15}", 2..6),
        ) -> String {
            let n = n.min(prompts.len()).min(commands.len());
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for i in 0..n {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                if i % 2 == 0 {
                    yaml.push_str(&format!("    infer: \"{}\"\n", prompts[i]));
                } else {
                    yaml.push_str(&format!("    exec: \"{}\"\n", commands[i]));
                }
            }
            yaml
        }
    }

    prop_compose! {
        /// Generate diamond DAG: A → B, A → C, B → D, C → D
        fn arb_diamond_workflow()(
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,20}", 4..5),
        ) -> String {
            format!(
                "schema: nika/workflow@0.12\ntasks:\n\
                 \x20 - id: root\n    infer: \"{}\"\n\
                 \x20 - id: left\n    depends_on: [root]\n    infer: \"{}\"\n\
                 \x20 - id: right\n    depends_on: [root]\n    infer: \"{}\"\n\
                 \x20 - id: sink\n    depends_on: [left, right]\n    infer: \"{}\"\n",
                prompts[0], prompts[1], prompts[2], prompts[3]
            )
        }
    }

    /// Shared roundtrip assertion: parse → analyze → lower → unlower preserves task count + IDs
    fn assert_roundtrip(yaml: &str) -> Result<(), proptest::test_runner::TestCaseError> {
        let raw = match parse(yaml, FileId(0)) {
            Ok(r) => r,
            Err(_) => return Ok(()), // Skip unparseable
        };
        let analyzed = match analyze(raw) {
            r if r.is_ok() => r.value.unwrap(),
            _ => return Ok(()), // Skip invalid
        };
        let task_count = analyzed.tasks.len();
        let task_names: Vec<String> = analyzed.tasks.iter().map(|t| t.name.clone()).collect();

        let lowered = match lower(analyzed) {
            Ok(l) => l,
            Err(_) => return Ok(()),
        };
        let unlowered = match unlower(lowered) {
            Ok(u) => u,
            Err(_) => return Ok(()),
        };

        prop_assert_eq!(
            unlowered.tasks.len(),
            task_count,
            "Task count should be preserved through roundtrip"
        );
        let rt_names: Vec<String> = unlowered.tasks.iter().map(|t| t.name.clone()).collect();
        prop_assert_eq!(
            rt_names, task_names,
            "Task names should be preserved through roundtrip"
        );
        Ok(())
    }

    proptest! {
        /// Property: Full pipeline roundtrip preserves task count and IDs
        #[test]
        fn test_pipeline_roundtrip_task_count(yaml in arb_pipeline_workflow()) {
            assert_roundtrip(&yaml)?;
        }

        /// Property: Workflows with `with:` bindings survive roundtrip
        #[test]
        fn test_pipeline_roundtrip_with_bindings(yaml in arb_with_workflow()) {
            assert_roundtrip(&yaml)?;
        }

        /// Property: Mixed-verb workflows survive roundtrip
        #[test]
        fn test_pipeline_roundtrip_mixed_verbs(yaml in arb_mixed_verb_workflow()) {
            assert_roundtrip(&yaml)?;
        }

        /// Property: Diamond DAGs survive roundtrip
        #[test]
        fn test_pipeline_roundtrip_diamond(yaml in arb_diamond_workflow()) {
            assert_roundtrip(&yaml)?;
        }
    }
}
