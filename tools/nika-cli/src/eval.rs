// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika eval` — Workflow evaluation framework
//!
//! Runs a workflow multiple times with different inputs from a dataset,
//! validates task outputs against expected assertions, and reports results.
//!
//! Dataset format:
//! ```json
//! [
//!   {
//!     "inputs": { "topic": "quantum computing" },
//!     "expected": {
//!       "tasks": {
//!         "research": { "output_contains": "qubit" },
//!         "summarize": { "output_min_words": 50, "output_max_words": 500 }
//!       }
//!     }
//!   }
//! ]
//! ```

use colored::Colorize;
use nika_engine::error::NikaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// DATA TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// A single entry in the evaluation dataset.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalEntry {
    /// Workflow inputs for this test case.
    pub inputs: serde_json::Value,
    /// Expected outputs: per-task assertions.
    pub expected: EvalExpected,
}

/// Expected output assertions.
#[derive(Debug, Clone, Deserialize)]
pub struct EvalExpected {
    /// Per-task assertions keyed by task ID.
    pub tasks: HashMap<String, TaskAssertions>,
}

/// Assertions for a single task's output.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskAssertions {
    /// Output must contain this substring (case-insensitive). Must be non-empty.
    pub output_contains: Option<String>,
    /// Output must have at least this many words (ASCII whitespace split — not CJK-aware).
    pub output_min_words: Option<usize>,
    /// Output must have at most this many words (ASCII whitespace split — not CJK-aware).
    pub output_max_words: Option<usize>,
    /// Output must be valid JSON with matching top-level `type` and `required` keys.
    /// Subset validation only — does not check property types, min/max, patterns.
    /// For full JSON Schema validation, use `structured:` in the workflow itself.
    pub output_matches_schema: Option<serde_json::Value>,
}

/// Result of evaluating a single dataset entry.
#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    pub entry_index: usize,
    pub passed: bool,
    pub failures: Vec<String>,
    pub duration_ms: u64,
}

/// Summary of all evaluation results.
#[derive(Debug, Clone, Serialize)]
pub struct EvalSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<EvalResult>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ASSERTION LOGIC (pure, testable)
// ═══════════════════════════════════════════════════════════════════════════

/// Validate a task output against assertions. Returns list of failure messages.
pub fn validate_task_output(
    task_id: &str,
    output: &serde_json::Value,
    assertions: &TaskAssertions,
) -> Vec<String> {
    let mut failures = Vec::new();
    let output_str = match output {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };

    // output_contains (case-insensitive)
    if let Some(ref needle) = assertions.output_contains {
        if needle.is_empty() {
            failures.push(format!(
                "{}: output_contains is empty — vacuous assertion",
                task_id
            ));
        } else if !output_str.to_lowercase().contains(&needle.to_lowercase()) {
            failures.push(format!(
                "{}: output_contains '{}' — not found",
                task_id, needle
            ));
        }
    }

    // output_min_words (ASCII whitespace splitting — not suitable for CJK text)
    if let Some(min) = assertions.output_min_words {
        let word_count = output_str.split_whitespace().count();
        if word_count < min {
            failures.push(format!(
                "{}: output_min_words {} — got {}",
                task_id, min, word_count
            ));
        }
    }

    // output_max_words
    if let Some(max) = assertions.output_max_words {
        let word_count = output_str.split_whitespace().count();
        if word_count > max {
            failures.push(format!(
                "{}: output_max_words {} — got {}",
                task_id, max, word_count
            ));
        }
    }

    // output_matches_schema — full JSON Schema validation via the
    // `jsonschema` crate. Every standard keyword (type, required, properties,
    // minimum/maximum, pattern, enum, minLength, …) is honoured. Replaces
    // the pre-Track-2 hand-rolled subset that silently ignored everything
    // except `type` and `required`.
    if let Some(ref schema) = assertions.output_matches_schema {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&output_str);
        match parsed {
            Ok(val) => {
                match jsonschema::validator_for(schema) {
                    Ok(validator) => {
                        let errs: Vec<_> = validator.iter_errors(&val).collect();
                        for err in errs {
                            failures.push(format!(
                                "{}: output_matches_schema at '{}' — {}",
                                task_id, err.instance_path, err
                            ));
                        }
                    }
                    Err(compile_err) => {
                        failures.push(format!(
                            "{}: output_matches_schema compile error — {}",
                            task_id, compile_err
                        ));
                    }
                }
            }
            Err(_) => {
                failures.push(format!(
                    "{}: output_matches_schema — output is not valid JSON",
                    task_id
                ));
            }
        }
    }

    failures
}

/// Validate all task outputs for a single dataset entry.
pub fn validate_entry(
    captured: &HashMap<String, serde_json::Value>,
    expected: &EvalExpected,
) -> Vec<String> {
    let mut failures = Vec::new();

    for (task_id, assertions) in &expected.tasks {
        if let Some(task_data) = captured.get(task_id) {
            // Extract the "output" field from the captured task result
            let output = task_data.get("output").unwrap_or(task_data);
            failures.extend(validate_task_output(task_id, output, assertions));
        } else {
            failures.push(format!("{}: task not found in workflow output", task_id));
        }
    }

    failures
}

// ═══════════════════════════════════════════════════════════════════════════
// DATASET LOADING
// ═══════════════════════════════════════════════════════════════════════════

/// Load evaluation dataset from a JSON file.
pub fn load_dataset(path: &str) -> Result<Vec<EvalEntry>, NikaError> {
    let content = std::fs::read_to_string(path).map_err(|e| NikaError::BuiltinToolError {
        tool: "eval".into(),
        reason: format!("Failed to read dataset '{}': {}", path, e),
    })?;

    let entries: Vec<EvalEntry> =
        serde_json::from_str(&content).map_err(|e| NikaError::BuiltinToolError {
            tool: "eval".into(),
            reason: format!("Invalid dataset JSON in '{}': {}", path, e),
        })?;

    if entries.is_empty() {
        return Err(NikaError::BuiltinToolError {
            tool: "eval".into(),
            reason: "Dataset is empty".into(),
        });
    }

    Ok(entries)
}

// ═══════════════════════════════════════════════════════════════════════════
// FORMATTING
// ═══════════════════════════════════════════════════════════════════════════

/// Format eval results as human-readable text.
pub fn format_text(summary: &EvalSummary) -> String {
    let mut out = String::new();

    for r in &summary.results {
        if r.passed {
            out.push_str(&format!(
                "  {} entry #{} ({}ms)\n",
                "PASS".green(),
                r.entry_index,
                r.duration_ms
            ));
        } else {
            out.push_str(&format!(
                "  {} entry #{} ({}ms)\n",
                "FAIL".red().bold(),
                r.entry_index,
                r.duration_ms
            ));
            for f in &r.failures {
                out.push_str(&format!("    {f}\n"));
            }
        }
    }

    out.push_str(&format!(
        "\n  {} {}/{} passed, {} failed\n",
        "Eval:".cyan(),
        summary.passed,
        summary.total,
        summary.failed
    ));

    out
}

/// Format eval results as JSON.
pub fn format_json(summary: &EvalSummary) -> String {
    serde_json::to_string_pretty(summary).unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS (used by main.rs after running workflows)
// ═══════════════════════════════════════════════════════════════════════════

/// Convert inputs to CLI key=value format for `run_workflow`.
pub fn inputs_to_cli_args(inputs: &serde_json::Value) -> Vec<String> {
    if let Some(obj) = inputs.as_object() {
        obj.iter()
            .map(|(k, v)| {
                let val_str = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{k}={val_str}")
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Build summary from results and print it. Returns error if any failures.
pub fn finalize(results: Vec<EvalResult>, format: &str, quiet: bool) -> Result<(), NikaError> {
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();
    let total = results.len();

    let summary = EvalSummary {
        total,
        passed,
        failed,
        results,
    };

    match format {
        "json" => println!("{}", format_json(&summary)),
        _ => {
            if !quiet {
                eprint!("{}", format_text(&summary));
            }
        }
    }

    if failed > 0 {
        Err(NikaError::BuiltinToolError {
            tool: "eval".into(),
            reason: format!("{failed}/{total} entries failed"),
        })
    } else {
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI ENTRY POINT (moved from main.rs in Phase 15)
// ═══════════════════════════════════════════════════════════════════════════

/// Run evaluation pipeline: load dataset → run workflow per entry → validate → report.
#[allow(clippy::too_many_arguments)]
pub async fn eval_workflow(
    file: &str,
    dataset_path: &str,
    provider_override: Option<&str>,
    format: &str,
    fail_fast: bool,
    parallel: usize,
    skip_confirm: bool,
    quiet: bool,
    detail: nika_engine::display::DetailLevel,
) -> Result<(), NikaError> {
    let entries = load_dataset(dataset_path)?;
    let concurrency = parallel.max(1).min(entries.len());

    if !quiet {
        eprintln!(
            "  {} {} entries from {}{}",
            "Eval:".cyan(),
            entries.len(),
            dataset_path,
            if concurrency > 1 {
                format!(" (parallel: {concurrency})")
            } else {
                String::new()
            }
        );
    }

    // Default to mock provider for safety (no accidental API costs)
    let provider = provider_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| "mock".to_string());

    let mut results = Vec::with_capacity(entries.len());
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(concurrency));

    for (i, entry) in entries.iter().enumerate() {
        let _permit = semaphore.acquire().await.map_err(|_| NikaError::ValidationError {
            reason: "Eval semaphore closed unexpectedly".to_string(),
        })?;
        let start = std::time::Instant::now();
        let cli_inputs = inputs_to_cli_args(&entry.inputs);

        // Create temp file for output capture
        let capture_dir = std::env::temp_dir();
        let capture_path = capture_dir.join(format!("nika-eval-{}-{i}.json", std::process::id()));
        let capture_str = capture_path.to_string_lossy().to_string();

        // Run workflow with output capture
        let run_result = crate::run::run_workflow(
            file,
            Some(provider.clone()),
            None,
            &cli_inputs,
            None,
            false,
            Some(&capture_str),
            None,
            None,
            skip_confirm || provider == "mock",
            true, // quiet — suppress per-run output
            detail,
            true, // no-live
            "deny",
            false,
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match run_result {
            Ok(()) => {
                // Read captured output
                let captured: std::collections::HashMap<String, serde_json::Value> =
                    if let Ok(raw) = tokio::fs::read_to_string(&capture_path).await {
                        serde_json::from_str(&raw).unwrap_or_default()
                    } else {
                        std::collections::HashMap::new()
                    };
                let _ = tokio::fs::remove_file(&capture_path).await;

                let failures = validate_entry(&captured, &entry.expected);
                let passed = failures.is_empty();

                if !quiet && !passed {
                    eprintln!("  {} entry #{i}", "FAIL".red().bold());
                    for f in &failures {
                        eprintln!("    {f}");
                    }
                } else if !quiet {
                    eprintln!("  {} entry #{i} ({}ms)", "PASS".green(), duration_ms);
                }

                results.push(EvalResult {
                    entry_index: i,
                    passed,
                    failures,
                    duration_ms,
                });

                if !passed && fail_fast {
                    break;
                }
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&capture_path).await;

                if !quiet {
                    eprintln!("  {} entry #{i} — {e}", "FAIL".red().bold());
                }
                results.push(EvalResult {
                    entry_index: i,
                    passed: false,
                    failures: vec![format!("workflow execution failed: {e}")],
                    duration_ms,
                });
                if fail_fast {
                    break;
                }
            }
        }
    }

    finalize(results, format, quiet)
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── validate_task_output ────────────────────────────────────

    #[test]
    fn output_contains_passes_when_present() {
        let assertions = TaskAssertions {
            output_contains: Some("qubit".into()),
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("Quantum computing uses qubits for computation");
        let failures = validate_task_output("research", &output, &assertions);
        assert!(failures.is_empty(), "Should pass: {failures:?}");
    }

    #[test]
    fn output_contains_case_insensitive() {
        let assertions = TaskAssertions {
            output_contains: Some("QUBIT".into()),
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("A qubit is the basic unit");
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(failures.is_empty());
    }

    #[test]
    fn output_contains_fails_when_missing() {
        let assertions = TaskAssertions {
            output_contains: Some("blockchain".into()),
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("Quantum computing is revolutionary");
        let failures = validate_task_output("research", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("output_contains"));
        assert!(failures[0].contains("blockchain"));
    }

    #[test]
    fn output_contains_empty_needle_fails() {
        let assertions = TaskAssertions {
            output_contains: Some(String::new()),
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("any text");
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("vacuous"));
    }

    #[test]
    fn output_min_words_passes() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: Some(3),
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("one two three four five");
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(failures.is_empty());
    }

    #[test]
    fn output_min_words_fails() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: Some(10),
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("too short");
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("output_min_words"));
    }

    #[test]
    fn output_max_words_passes() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: Some(5),
            output_matches_schema: None,
        };
        let output = json!("just three words");
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(failures.is_empty());
    }

    #[test]
    fn output_max_words_fails() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: Some(2),
            output_matches_schema: None,
        };
        let output = json!("way too many words here");
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("output_max_words"));
    }

    #[test]
    fn output_matches_schema_object_passes() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "required": ["name", "age"]
            })),
        };
        let output = json!(r#"{"name": "Alice", "age": 30}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(failures.is_empty(), "Should pass: {failures:?}");
    }

    #[test]
    fn output_matches_schema_missing_required_field() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "required": ["name", "age", "email"]
            })),
        };
        let output = json!(r#"{"name": "Alice", "age": 30}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("email"));
    }

    #[test]
    fn output_matches_schema_wrong_type() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({"type": "array"})),
        };
        let output = json!(r#"{"key": "value"}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("type"));
    }

    #[test]
    fn output_matches_schema_not_json() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({"type": "object"})),
        };
        let output = json!("this is not JSON at all");
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("not valid JSON"));
    }

    // ── Full JSON Schema coverage (Track 2 — jsonschema crate) ──────────
    // Before: the hand-rolled validator honoured only `type` + `required`
    // and surfaced a WARNING for every other keyword. These tests lock the
    // new behaviour: properties/minimum/maximum/pattern/enum are all real
    // assertions that produce failures when violated.

    #[test]
    fn schema_rejects_minimum_violation() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "properties": {
                    "price": { "type": "number", "minimum": 0 }
                },
                "required": ["price"]
            })),
        };
        let output = json!(r#"{"price": -5}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(
            failures.iter().any(|f| f.contains("price") || f.contains("minimum") || f.contains("-5")),
            "minimum: 0 must reject -5: {failures:?}"
        );
    }

    #[test]
    fn schema_rejects_pattern_mismatch() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "properties": {
                    "zip": { "type": "string", "pattern": "^\\d{5}$" }
                },
                "required": ["zip"]
            })),
        };
        let output = json!(r#"{"zip": "abc"}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(
            !failures.is_empty(),
            "pattern ^\\d{{5}}$ must reject 'abc': {failures:?}"
        );
    }

    #[test]
    fn schema_rejects_enum_mismatch() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "properties": {
                    "color": { "type": "string", "enum": ["red", "blue"] }
                },
                "required": ["color"]
            })),
        };
        let output = json!(r#"{"color": "purple"}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(
            !failures.is_empty(),
            "enum [red, blue] must reject 'purple': {failures:?}"
        );
    }

    #[test]
    fn schema_accepts_fully_matching_output() {
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "properties": {
                    "price": { "type": "number", "minimum": 0, "maximum": 1000 },
                    "zip": { "type": "string", "pattern": "^\\d{5}$" },
                    "color": { "type": "string", "enum": ["red", "blue"] }
                },
                "required": ["price", "zip", "color"]
            })),
        };
        let output = json!(r#"{"price": 42, "zip": "12345", "color": "red"}"#);
        let failures = validate_task_output("ok", &output, &assertions);
        assert!(
            failures.is_empty(),
            "valid payload must pass: {failures:?}"
        );
    }

    #[test]
    fn schema_no_longer_warns_on_extra_keywords() {
        // Post-Track-2: keywords are honoured, not ignored. No "ignores
        // keywords" warning should ever appear again.
        let assertions = TaskAssertions {
            output_contains: None,
            output_min_words: None,
            output_max_words: None,
            output_matches_schema: Some(json!({
                "type": "object",
                "required": ["score"],
                "properties": { "score": { "type": "number", "minimum": 0 } }
            })),
        };
        let output = json!(r#"{"score": 42}"#);
        let failures = validate_task_output("t1", &output, &assertions);
        assert!(
            !failures.iter().any(|f| f.contains("ignores keywords")),
            "no more ignores-keywords warnings: {failures:?}"
        );
    }

    #[test]
    fn multiple_assertions_all_checked() {
        let assertions = TaskAssertions {
            output_contains: Some("quantum".into()),
            output_min_words: Some(5),
            output_max_words: Some(100),
            output_matches_schema: None,
        };
        let output = json!("Quantum computing uses qubits for parallel computation");
        let failures = validate_task_output("research", &output, &assertions);
        assert!(failures.is_empty());
    }

    #[test]
    fn multiple_assertions_multiple_failures() {
        let assertions = TaskAssertions {
            output_contains: Some("blockchain".into()),
            output_min_words: Some(100),
            output_max_words: None,
            output_matches_schema: None,
        };
        let output = json!("short text");
        let failures = validate_task_output("t1", &output, &assertions);
        assert_eq!(failures.len(), 2);
    }

    // ── validate_entry ──────────────────────────────────────────

    #[test]
    fn validate_entry_passes_when_all_tasks_match() {
        let mut captured = HashMap::new();
        captured.insert(
            "research".into(),
            json!({"output": "Quantum computing uses qubits", "status": "Success"}),
        );

        let expected = EvalExpected {
            tasks: {
                let mut m = HashMap::new();
                m.insert(
                    "research".into(),
                    TaskAssertions {
                        output_contains: Some("qubit".into()),
                        output_min_words: None,
                        output_max_words: None,
                        output_matches_schema: None,
                    },
                );
                m
            },
        };

        let failures = validate_entry(&captured, &expected);
        assert!(failures.is_empty());
    }

    #[test]
    fn validate_entry_fails_on_missing_task() {
        let captured = HashMap::new(); // empty — no tasks

        let expected = EvalExpected {
            tasks: {
                let mut m = HashMap::new();
                m.insert(
                    "research".into(),
                    TaskAssertions {
                        output_contains: Some("qubit".into()),
                        output_min_words: None,
                        output_max_words: None,
                        output_matches_schema: None,
                    },
                );
                m
            },
        };

        let failures = validate_entry(&captured, &expected);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("task not found"));
    }

    // ── load_dataset ────────────────────────────────────────────

    #[test]
    fn load_dataset_missing_file_errors() {
        let result = load_dataset("/nonexistent/path.json");
        assert!(result.is_err());
    }

    // ── format ──────────────────────────────────────────────────

    #[test]
    fn format_json_produces_valid_json() {
        let summary = EvalSummary {
            total: 2,
            passed: 1,
            failed: 1,
            results: vec![
                EvalResult {
                    entry_index: 0,
                    passed: true,
                    failures: vec![],
                    duration_ms: 100,
                },
                EvalResult {
                    entry_index: 1,
                    passed: false,
                    failures: vec!["something failed".into()],
                    duration_ms: 200,
                },
            ],
        };
        let json_str = format_json(&summary);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["total"], 2);
        assert_eq!(parsed["passed"], 1);
        assert_eq!(parsed["failed"], 1);
    }

    #[test]
    fn format_text_shows_pass_fail() {
        let summary = EvalSummary {
            total: 2,
            passed: 1,
            failed: 1,
            results: vec![
                EvalResult {
                    entry_index: 0,
                    passed: true,
                    failures: vec![],
                    duration_ms: 50,
                },
                EvalResult {
                    entry_index: 1,
                    passed: false,
                    failures: vec!["output_contains 'qubit' — not found".into()],
                    duration_ms: 75,
                },
            ],
        };
        let text = format_text(&summary);
        assert!(text.contains("1/2 passed"));
        assert!(text.contains("1 failed"));
    }
}
