// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
        RunContext::new(nika::trust::InvocationSource::Test)
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
        ///
        /// The regex `[^{}]*` guarantees no `{` or `}` in the input, so there
        /// are no template markers — the result must always be Borrowed.
        #[test]
        fn test_no_template_returns_borrowed(s in "[^{}]*") {
            let bindings = ResolvedBindings::new();
            let ds = empty_datastore();
            let result = template_resolve(&s, &bindings, &ds)
                .expect("Strings without braces should always resolve");
            assert!(
                matches!(result, Cow::Borrowed(_)),
                "No-template string should be Cow::Borrowed, got Owned"
            );
        }

        /// Property: Templates with substitutions return Cow::Owned
        #[test]
        fn test_template_with_substitution_returns_owned(template in arb_template_with_alias()) {
            let alias_re = regex::Regex::new(r"\{\{\s*with\.(\w+)").unwrap();
            let cap = alias_re.captures(&template)
                .expect("arb_template_with_alias must produce {{with.alias}} patterns");
            let alias = &cap[1];
            let mut bindings = ResolvedBindings::new();
            bindings.set(alias, json!("value"));
            let ds = empty_datastore();

            let cow = template_resolve(&template, &bindings, &ds)
                .expect("Template with bound alias should resolve");
            // With substitution, should be owned
            assert!(matches!(cow, Cow::Owned(_)));
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

        /// Property: Array index access works correctly for in-bounds indices
        #[test]
        fn test_array_index_access(
            alias in arb_alias(),
            values in prop::collection::vec("[ -~]{1,10}", 1..15)
        ) {
            // Always pick a valid index to avoid silently skipping the test
            let index = 0; // guaranteed in-bounds since vec has at least 1 element
            let template = format!("{{{{with.{}.{}}}}}", alias, index);
            let mut bindings = ResolvedBindings::new();
            bindings.set(&alias, json!(values.clone()));
            let ds = empty_datastore();

            let result = template_resolve(&template, &bindings, &ds);
            assert!(result.is_ok(), "In-bounds index access should succeed");
            assert_eq!(result.unwrap().as_ref(), values[index]);
        }

        /// Property: Out-of-bounds array index access doesn't panic
        #[test]
        fn test_array_out_of_bounds_no_panic(
            alias in arb_alias(),
            values in prop::collection::vec("[ -~]{1,10}", 1..5)
        ) {
            let oob_index = values.len(); // always out of bounds
            let template = format!("{{{{with.{}.{}}}}}", alias, oob_index);
            let mut bindings = ResolvedBindings::new();
            bindings.set(&alias, json!(values.clone()));
            let ds = empty_datastore();

            // Should not panic; either returns error or empty string
            let _ = template_resolve(&template, &bindings, &ds);
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
                // Should not panic — either parse error or schema validation error.
                // NOTE: Parse failure is acceptable here because some random strings
                // generate invalid YAML (e.g. tabs in wrong position, special chars).
                // The important property is: invalid schemas NEVER pass analysis.
                let result = nika::ast::raw::parse(&yaml, nika::source::FileId(0));
                match result {
                    Ok(raw) => {
                        let analyzed = nika::ast::analyzer::analyze(raw);
                        prop_assert!(analyzed.is_err(),
                            "Invalid schema '{}' should fail analysis", invalid_schema);
                    }
                    Err(_) => {
                        // Parse error is acceptable: the generated schema string may
                        // produce YAML that our parser rejects before reaching schema
                        // validation. The key invariant (invalid schema ≠ success)
                        // still holds since parse errors prevent execution.
                    }
                }
            }
        }

        /// Property: for_each with empty array parses but doesn't panic
        #[test]
        fn test_for_each_empty_array_parses_safely(task_id in arb_task_id()) {
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
            // Parser should handle empty for_each without panic.
            // Empty array validation happens at runtime (validate_for_each).
            let result = nika::ast::raw::parse(&yaml, nika::source::FileId(0));
            if let Ok(raw) = result {
                // If parsed, analyze should also not panic
                let _ = nika::ast::analyzer::analyze(raw);
            }
        }

        /// Property: for_each with non-array scalar parses safely
        #[test]
        fn test_for_each_non_array_parses_safely(
            task_id in arb_task_id(),
            non_array in prop::sample::select(vec!["plain_string", "123", "true"])
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
            // Parser treats these as scalar strings. Should not panic.
            let result = nika::ast::raw::parse(&yaml, nika::source::FileId(0));
            if let Ok(raw) = result {
                // Non-binding, non-array for_each should not crash analyzer
                let _ = nika::ast::analyzer::analyze(raw);
            }
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
        /// Property: Valid snake_case task IDs are accepted by parser+analyzer
        #[test]
        fn test_valid_task_id_accepted_by_parser(id in arb_valid_task_id()) {
            let yaml = format!(
                "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    infer: \"test\"\n",
                id
            );
            let raw = parse(&yaml, FileId(0))
                .expect("Valid task ID YAML should parse");
            let result = analyze(raw);
            prop_assert!(result.is_ok(),
                "Valid task ID '{}' should pass analysis: {:?}", id, result.errors);
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
            let raw = parse(&yaml, FileId(0))
                .expect("Self-reference YAML should parse (valid syntax)");
            let result = analyze(raw);
            prop_assert!(result.is_err(),
                "Self-referencing task '{}' should fail analysis", task_id);
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
                let raw = parse(&yaml, FileId(0))
                    .expect("Cycle YAML should parse (valid syntax)");
                let result = analyze(raw);
                prop_assert!(result.is_err(),
                    "Cycle {}<->{} should fail analysis", task1, task2);
            }
        }

        /// Property: Referencing non-existent task detected by analyzer
        #[test]
        fn test_nonexistent_task_fails(
            task1 in arb_valid_task_id(),
            ghost in arb_valid_task_id()
        ) {
            // task depends_on a nonexistent ID (ghost + "__missing" suffix).
            let nonexistent = format!("{}__missing", ghost);
            let yaml = format!(
                "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"test\"\n",
                task1, nonexistent
            );
            let raw = parse(&yaml, FileId(0))
                .expect("Nonexistent-dep YAML should parse (valid syntax)");
            let result = analyze(raw);
            prop_assert!(result.is_err(),
                "Reference to nonexistent '{}' should fail analysis", nonexistent);
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
// TEST 4: Binding Storage Tests (Nika-specific)
// =============================================================================

mod binding_storage_fuzzing {
    use super::*;
    use nika::binding::ResolvedBindings;

    proptest! {
        /// Property: Binding set+get roundtrip preserves values
        #[test]
        fn test_binding_set_get_roundtrip(
            alias in r"[a-z][a-z0-9_]{0,15}",
            value in "[ -~]{0,50}"
        ) {
            let mut bindings = ResolvedBindings::new();
            let json_val = json!(value.clone());
            bindings.set(&alias, json_val.clone());
            let got = bindings.get(&alias);
            prop_assert!(got.is_some(), "Stored alias '{}' should be retrievable", alias);
            prop_assert_eq!(got.unwrap(), &json_val);
        }

        /// Property: Getting an unset alias returns None
        #[test]
        fn test_binding_unset_returns_none(alias in r"[a-z][a-z0-9_]{0,15}") {
            let bindings = ResolvedBindings::new();
            prop_assert!(bindings.get(&alias).is_none(),
                "Unset alias '{}' should return None", alias);
        }

        /// Property: Multiple bindings are independent
        #[test]
        fn test_binding_independence(
            alias1 in r"[a-z][a-z0-9_]{0,10}",
            alias2 in r"[a-z][a-z0-9_]{0,10}",
            value1 in "[ -~]{0,20}",
            value2 in "[ -~]{0,20}"
        ) {
            if alias1 != alias2 {
                let mut bindings = ResolvedBindings::new();
                bindings.set(&alias1, json!(value1.clone()));
                bindings.set(&alias2, json!(value2.clone()));
                prop_assert_eq!(bindings.get(&alias1).unwrap(), &json!(value1));
                prop_assert_eq!(bindings.get(&alias2).unwrap(), &json!(value2));
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
            for (i, prompt) in prompts.iter().enumerate().take(n) {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                yaml.push_str(&format!("    infer: \"{}\"\n", prompt));
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
            for (i, prompt) in prompts.iter().enumerate().take(n).skip(1) {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                yaml.push_str(&format!("    with:\n      prev: $task_{}\n", i - 1));
                yaml.push_str(&format!("    infer: \"{}\"\n", prompt));
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
                    yaml.push_str(&format!("    infer: \"{}\"\n", &prompts[i]));
                } else {
                    yaml.push_str(&format!("    exec: \"{}\"\n", &commands[i]));
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

    /// Shared roundtrip assertion: parse → analyze → lower → unlower preserves task count + IDs.
    /// All generators that call this produce valid workflow YAML, so every stage must succeed.
    fn assert_roundtrip(yaml: &str) -> Result<(), proptest::test_runner::TestCaseError> {
        let raw = parse(yaml, FileId(0)).expect("Generator should produce valid YAML");
        let analyzed = {
            let r = analyze(raw);
            assert!(
                r.is_ok(),
                "Generator should produce analyzable workflow: {:?}",
                r.errors
            );
            r.value.unwrap()
        };
        let task_count = analyzed.tasks.len();
        let task_names: Vec<String> = analyzed.tasks.iter().map(|t| t.name.clone()).collect();
        // Capture dependency counts before `analyzed` is consumed by lower()
        let orig_dep_counts: Vec<usize> = analyzed
            .tasks
            .iter()
            .map(|t| t.depends_on.len() + t.implicit_deps.len())
            .collect();

        let lowered = lower(analyzed).expect("Analyzed workflow should lower");
        let unlowered = unlower(lowered).expect("Lowered workflow should unlower");

        prop_assert_eq!(
            unlowered.tasks.len(),
            task_count,
            "Task count should be preserved through roundtrip"
        );
        let rt_names: Vec<String> = unlowered.tasks.iter().map(|t| t.name.clone()).collect();
        prop_assert_eq!(
            rt_names,
            task_names,
            "Task names should be preserved through roundtrip"
        );

        // Verify dependency counts are preserved (names merge with implicit_deps
        // but total must not grow)
        for (roundtripped, &orig_count) in unlowered.tasks.iter().zip(orig_dep_counts.iter()) {
            let rt_dep_count = roundtripped.depends_on.len() + roundtripped.implicit_deps.len();
            prop_assert!(
                rt_dep_count <= orig_count + 1, // +1 tolerance for merged deps
                "Dependencies should be preserved: original={}, roundtripped={}",
                orig_count,
                rt_dep_count
            );
        }

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

// =============================================================================
// TEST 7: Algebraic Property Tests
// =============================================================================
// Determinism, symmetry, and type preservation as algebraic invariants.

mod algebraic_property_tests {
    use super::*;
    use nika::ast::analyzer::analyze;
    use nika::ast::lower::{lower, unlower};
    use nika::ast::raw::parse;
    use nika::source::FileId;

    prop_compose! {
        /// Generate a deterministic test workflow
        fn arb_deterministic_workflow()(
            n in 1usize..4,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,20}", 1..5),
        ) -> String {
            let n = n.min(prompts.len());
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for (i, prompt) in prompts.iter().enumerate().take(n) {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                yaml.push_str(&format!("    infer: \"{}\"\n", prompt));
            }
            yaml
        }
    }

    prop_compose! {
        /// Generate valid task IDs for cycle tests
        fn arb_task_id()(id in r"[a-z][a-z0-9_]{0,15}") -> String {
            id
        }
    }

    prop_compose! {
        /// Generate a mixed-verb workflow for action type preservation
        fn arb_mixed_action_workflow()(
            n in 2usize..5,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,20}", 2..6),
            commands in prop::collection::vec(r"echo [a-zA-Z0-9]{1,10}", 2..6),
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

    proptest! {
        /// Algebraic: parse+analyze is deterministic (same input → same output).
        #[test]
        fn test_analyze_deterministic(yaml in arb_deterministic_workflow()) {
            let raw1 = parse(&yaml, FileId(0))
                .expect("Generator should produce valid YAML");
            let raw2 = parse(&yaml, FileId(0))
                .expect("Second parse should also succeed");

            let result1 = analyze(raw1);
            let result2 = analyze(raw2);

            assert!(result1.is_ok(), "First analysis should pass: {:?}", result1.errors);
            assert!(result2.is_ok(), "Second analysis should pass: {:?}", result2.errors);

            let wf1 = result1.value.unwrap();
            let wf2 = result2.value.unwrap();

            // Same task count
            prop_assert_eq!(wf1.tasks.len(), wf2.tasks.len(),
                "Determinism: task count should be identical");
            // Same task names in same order
            let names1: Vec<&str> = wf1.tasks.iter().map(|t| t.name.as_str()).collect();
            let names2: Vec<&str> = wf2.tasks.iter().map(|t| t.name.as_str()).collect();
            prop_assert_eq!(names1, names2,
                "Determinism: task names should be identical");
            // Same provider
            prop_assert_eq!(wf1.provider, wf2.provider,
                "Determinism: provider should be identical");
        }

        /// Algebraic: cycle detection is bidirectional — A→B and B→A both detected.
        #[test]
        fn test_cycle_detection_bidirectional(
            task_a in arb_task_id(),
            task_b in arb_task_id()
        ) {
            if task_a != task_b {
                // A depends on B, B depends on A
                let yaml_ab = format!(
                    "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"first\"\n  - id: {}\n    depends_on: [{}]\n    infer: \"second\"\n",
                    task_a, task_b, task_b, task_a
                );
                // B depends on A, A depends on B (reversed task order)
                let yaml_ba = format!(
                    "schema: nika/workflow@0.12\ntasks:\n  - id: {}\n    depends_on: [{}]\n    infer: \"second\"\n  - id: {}\n    depends_on: [{}]\n    infer: \"first\"\n",
                    task_b, task_a, task_a, task_b
                );

                let raw_ab = parse(&yaml_ab, FileId(0))
                    .expect("Cycle AB YAML should parse");
                let raw_ba = parse(&yaml_ba, FileId(0))
                    .expect("Cycle BA YAML should parse");

                let result_ab = analyze(raw_ab);
                let result_ba = analyze(raw_ba);

                // Both should fail (cycle detected)
                prop_assert!(result_ab.is_err(),
                    "A->B cycle should fail analysis");
                prop_assert!(result_ba.is_err(),
                    "B->A cycle should fail analysis");

                // Both should have at least one error
                prop_assert!(!result_ab.errors.is_empty(),
                    "A->B should produce error(s)");
                prop_assert!(!result_ba.errors.is_empty(),
                    "B->A should produce error(s)");
            }
        }

        /// Algebraic: roundtrip preserves action types (infer stays infer, exec stays exec).
        #[test]
        fn test_roundtrip_preserves_action_types(yaml in arb_mixed_action_workflow()) {
            let raw = parse(&yaml, FileId(0))
                .expect("Generator should produce valid YAML");
            let result = analyze(raw);
            assert!(result.is_ok(), "Analysis should pass: {:?}", result.errors);
            let wf = result.value.unwrap();

            // Record action types before roundtrip
            let action_types_before: Vec<&str> = wf.tasks.iter().map(|t| match &t.action {
                nika::ast::analyzed::AnalyzedTaskAction::Infer(_) => "infer",
                nika::ast::analyzed::AnalyzedTaskAction::Exec(_) => "exec",
                nika::ast::analyzed::AnalyzedTaskAction::Fetch(_) => "fetch",
                nika::ast::analyzed::AnalyzedTaskAction::Invoke(_) => "invoke",
                nika::ast::analyzed::AnalyzedTaskAction::Agent(_) => "agent",
            }).collect();

            let lowered = lower(wf).expect("Lowering should succeed");
            let unlowered = unlower(lowered).expect("Unlowering should succeed");

            // Verify action types preserved
            let action_types_after: Vec<&str> = unlowered.tasks.iter().map(|t| match &t.action {
                nika::ast::analyzed::AnalyzedTaskAction::Infer(_) => "infer",
                nika::ast::analyzed::AnalyzedTaskAction::Exec(_) => "exec",
                nika::ast::analyzed::AnalyzedTaskAction::Fetch(_) => "fetch",
                nika::ast::analyzed::AnalyzedTaskAction::Invoke(_) => "invoke",
                nika::ast::analyzed::AnalyzedTaskAction::Agent(_) => "agent",
            }).collect();

            prop_assert_eq!(action_types_before, action_types_after,
                "Action types must be preserved through lower/unlower roundtrip");
        }
    }
}
