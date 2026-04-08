// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika test` command handler — golden file testing.
//!
//! Runs a workflow with the mock provider and optionally compares output
//! against a golden snapshot file for regression detection.

use std::path::Path;

use colored::Colorize;
use nika_engine::error::NikaError;

/// Normalize captured output for golden file comparison.
/// Strips non-deterministic fields (duration_ms) and sorts keys for stable ordering.
pub fn normalize_golden(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut normalized = serde_json::Map::new();
            for (key, val) in map {
                if key == "duration_ms" {
                    continue; // strip non-deterministic timing
                }
                normalized.insert(key.clone(), normalize_golden(val));
            }
            serde_json::Value::Object(normalized)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_golden).collect())
        }
        other => other.clone(),
    }
}

/// Compare two golden JSON values, returning a list of mismatches.
pub fn compare_golden(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    path: &str,
) -> Vec<String> {
    let mut diffs = Vec::new();
    match (actual, expected) {
        (serde_json::Value::Object(a), serde_json::Value::Object(e)) => {
            // Keys in expected but missing in actual
            for key in e.keys() {
                if !a.contains_key(key) {
                    diffs.push(format!("{path}.{key}: missing in actual output"));
                }
            }
            // Keys in actual but missing in expected
            for key in a.keys() {
                if !e.contains_key(key) {
                    diffs.push(format!("{path}.{key}: unexpected key in actual output"));
                }
            }
            // Recurse on shared keys
            for key in e.keys() {
                if let (Some(av), Some(ev)) = (a.get(key), e.get(key)) {
                    diffs.extend(compare_golden(av, ev, &format!("{path}.{key}")));
                }
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(e)) => {
            if a.len() != e.len() {
                diffs.push(format!(
                    "{path}: array length mismatch (actual={}, expected={})",
                    a.len(),
                    e.len()
                ));
            }
            for (i, (av, ev)) in a.iter().zip(e.iter()).enumerate() {
                diffs.extend(compare_golden(av, ev, &format!("{path}[{i}]")));
            }
        }
        (a, e) if a != e => {
            let actual_str = serde_json::to_string(a).unwrap_or_default();
            let expected_str = serde_json::to_string(e).unwrap_or_default();
            // Truncate long values for readability
            let trunc = |s: String| -> String {
                if s.len() > 120 {
                    format!("{}…", &s[..117])
                } else {
                    s
                }
            };
            diffs.push(format!(
                "{path}: value mismatch\n      actual:   {}\n      expected: {}",
                trunc(actual_str),
                trunc(expected_str)
            ));
        }
        _ => {} // equal
    }
    diffs
}

/// Run a workflow test with optional golden file comparison.
pub async fn test_workflow(
    file: &str,
    golden: Option<&str>,
    update_snapshot: bool,
    cli_inputs: &[String],
    quiet: bool,
    detail: nika_engine::display::DetailLevel,
) -> Result<(), NikaError> {
    let needs_capture = golden.is_some() || update_snapshot;

    // Create temp file for output capture when golden comparison is needed
    let capture_path = if needs_capture {
        let mut path = std::env::temp_dir();
        path.push(format!("nika-test-{}.json", std::process::id()));
        Some(path.to_string_lossy().to_string())
    } else {
        None
    };

    // Run workflow with mock provider (no API keys needed)
    let result = crate::run::run_workflow(
        file,
        Some("mock".to_string()),
        None,
        cli_inputs,
        None,
        false, // not interactive
        capture_path.as_deref(),
        None,
        None,
        true, // skip cost confirm
        quiet,
        detail,
        true, // no-live for test output
        "deny",
        false,
    )
    .await;

    match &result {
        Ok(()) => {
            if !quiet {
                eprintln!("  {} {}", "PASS".green().bold(), file);
            }
        }
        Err(e) => {
            if !quiet {
                eprintln!("  {} {} — {}", "FAIL".red().bold(), file, e);
            }
            return result;
        }
    }

    // Golden file comparison (if requested)
    if let Some(golden_path) = golden {
        // Read captured output
        let captured_json = if let Some(ref cp) = capture_path {
            let raw =
                tokio::fs::read_to_string(cp)
                    .await
                    .map_err(|e| NikaError::BuiltinToolError {
                        tool: "test".into(),
                        reason: format!("Failed to read captured output: {e}"),
                    })?;
            let val: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Invalid captured output JSON: {e}"),
                })?;
            normalize_golden(&val)
        } else {
            serde_json::Value::Object(serde_json::Map::new())
        };

        if update_snapshot {
            // Write normalized output to golden file
            let pretty = serde_json::to_string_pretty(&captured_json).unwrap_or_default();
            tokio::fs::write(golden_path, &pretty).await.map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Failed to write golden file '{}': {e}", golden_path),
                }
            })?;
            if !quiet {
                eprintln!(
                    "  {} golden file updated: {}",
                    "Snapshot:".cyan(),
                    golden_path
                );
            }
        } else if Path::new(golden_path).exists() {
            // Compare output to golden file
            let golden_content = tokio::fs::read_to_string(golden_path).await?;
            let golden_value: serde_json::Value =
                serde_json::from_str(&golden_content).map_err(|e| NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!("Invalid golden file JSON: {e}"),
                })?;
            let golden_normalized = normalize_golden(&golden_value);

            let diffs = compare_golden(&captured_json, &golden_normalized, "$");
            if diffs.is_empty() {
                if !quiet {
                    eprintln!("  {} golden file matches", "OK".green());
                }
            } else {
                eprintln!(
                    "  {} golden file mismatch ({} difference{}):",
                    "FAIL".red().bold(),
                    diffs.len(),
                    if diffs.len() > 1 { "s" } else { "" }
                );
                for diff in &diffs {
                    eprintln!("    {diff}");
                }
                return Err(NikaError::BuiltinToolError {
                    tool: "test".into(),
                    reason: format!(
                        "Golden file mismatch: {} difference(s). Run with --update-snapshot to update.",
                        diffs.len()
                    ),
                });
            }
        } else {
            return Err(NikaError::BuiltinToolError {
                tool: "test".into(),
                reason: format!(
                    "Golden file not found: {}. Run with --update-snapshot to create it.",
                    golden_path
                ),
            });
        }
    }

    // Clean up temp capture file
    if let Some(ref cp) = capture_path {
        let _ = tokio::fs::remove_file(cp).await;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_golden_strips_duration_ms() {
        let input = json!({
            "task1": {
                "output": "hello",
                "status": "Success",
                "duration_ms": 42
            }
        });
        let normalized = normalize_golden(&input);
        assert_eq!(
            normalized,
            json!({
                "task1": {
                    "output": "hello",
                    "status": "Success"
                }
            })
        );
    }

    #[test]
    fn normalize_golden_strips_nested_duration() {
        let input = json!({
            "outer": {
                "inner": { "duration_ms": 100, "value": 42 },
                "duration_ms": 200
            }
        });
        let normalized = normalize_golden(&input);
        assert!(!normalized.to_string().contains("duration_ms"));
        assert_eq!(normalized["outer"]["inner"]["value"], json!(42));
    }

    #[test]
    fn normalize_golden_preserves_arrays() {
        let input = json!([
            { "output": "a", "duration_ms": 1 },
            { "output": "b", "duration_ms": 2 }
        ]);
        let normalized = normalize_golden(&input);
        assert_eq!(normalized, json!([{ "output": "a" }, { "output": "b" }]));
    }

    #[test]
    fn compare_golden_identical_returns_empty() {
        let a = json!({"task1": {"output": "hello"}});
        let b = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&a, &b, "$");
        assert!(diffs.is_empty(), "Expected no diffs, got: {diffs:?}");
    }

    #[test]
    fn compare_golden_detects_value_mismatch() {
        let actual = json!({"task1": {"output": "hello"}});
        let expected = json!({"task1": {"output": "world"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].contains("task1"));
        assert!(diffs[0].contains("output"));
        assert!(diffs[0].contains("value mismatch"));
    }

    #[test]
    fn compare_golden_detects_missing_key() {
        let actual = json!({"task1": {}});
        let expected = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("missing")));
    }

    #[test]
    fn compare_golden_detects_unexpected_key() {
        let actual = json!({"task1": {"output": "hello", "extra": true}});
        let expected = json!({"task1": {"output": "hello"}});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("unexpected")));
    }

    #[test]
    fn compare_golden_detects_array_length_mismatch() {
        let actual = json!({"items": [1, 2, 3]});
        let expected = json!({"items": [1, 2]});
        let diffs = compare_golden(&actual, &expected, "$");
        assert!(!diffs.is_empty());
        assert!(diffs.iter().any(|d| d.contains("array length")));
    }
}
