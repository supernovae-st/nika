// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI input parsing helpers.
//!
//! Handles `-i key=value` flags, `--input-file` loading (JSON/YAML/stdin),
//! type-coercing strings into serde_json::Value, and a minimal template
//! resolver for workflow-level `provider:`/`model:` field interpolation in
//! dry-run paths.

use nika_engine::error::NikaError;

/// Parse a CLI input value, inferring type from the string form.
///
/// Recognized forms:
/// - `true`/`false` → boolean
/// - `null` → JSON null
/// - Integer literal (`42`, `-7`, `0`) → number
/// - Decimal literal without exponent (`1.5`) → number
/// - JSON object/array literal (`{...}`, `[...]`) → parsed JSON
/// - Otherwise → string (including scientific notation like `1e3`)
pub fn parse_input_value(s: &str) -> serde_json::Value {
    use serde_json::Value;
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                return serde_json::json!(n);
            }
            // Only coerce to float for plain decimal notation (not scientific like "1e3")
            if !s.contains('e') && !s.contains('E') {
                if let Ok(n) = s.parse::<f64>() {
                    return serde_json::json!(n);
                }
            }
            if s.starts_with('{') || s.starts_with('[') {
                if let Ok(v) = serde_json::from_str::<Value>(s) {
                    return v;
                }
            }
            Value::String(s.to_string())
        }
    }
}

/// Parse `-i key=value` CLI flags into an ordered map.
pub fn parse_cli_inputs(raw: &[String]) -> Result<Vec<(String, serde_json::Value)>, NikaError> {
    let mut result = Vec::new();
    for item in raw {
        let (key, value) = item.split_once('=').ok_or_else(|| {
            // Detect common mistake: passing a bare value without a key
            let hint = if item.starts_with("http") {
                format!(
                    "Got '{item}' but -i expects KEY=VALUE format.\n\n  Example: nika run workflow.nika.yaml -i url={item}\n\n  Multiple inputs: -i url={item} -i lang=en\n  From file:       --input-file inputs.json",
                    item = item
                )
            } else {
                format!(
                    "Got '{item}' but -i expects KEY=VALUE format.\n\n  Example: nika run workflow.nika.yaml -i name={item}\n\n  Check your workflow's `inputs:` block for the expected key names.",
                    item = item
                )
            };
            NikaError::ValidationError { reason: hint }
        })?;
        result.push((key.to_string(), parse_input_value(value)));
    }
    Ok(result)
}

/// Load inputs from a JSON/YAML file (or stdin with "-").
pub async fn load_input_file(path: &str) -> Result<Vec<(String, serde_json::Value)>, NikaError> {
    let content = if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|_| NikaError::ParseError {
                details: "Failed to read from stdin".to_string(),
            })?;
        buf
    } else {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| NikaError::ParseError {
                details: format!("Failed to read input file '{}': {}", path, e),
            })?
    };

    // Auto-detect format: .json → JSON first, everything else → YAML first
    let value: serde_json::Value = if path.ends_with(".json") {
        serde_json::from_str(&content).map_err(|e| NikaError::ParseError {
            details: format!("Invalid JSON in '{}': {}", path, e),
        })?
    } else if path == "-" {
        // stdin: try JSON first, then YAML
        serde_json::from_str(&content).or_else(|_| {
            nika_engine::serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
                details: format!("Invalid JSON/YAML on stdin: {}", e),
            })
        })?
    } else {
        // .yaml, .yml, or anything else → YAML
        nika_engine::serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
            details: format!("Invalid YAML in '{}': {}", path, e),
        })?
    };

    // Must be a mapping at top level
    let map = value
        .as_object()
        .ok_or_else(|| NikaError::ValidationError {
            reason: format!(
                "Input file '{}' must be a JSON/YAML mapping (got {})",
                path,
                match &value {
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Null => "null",
                    _ => "unknown",
                }
            ),
        })?;

    Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// Minimal `{{inputs.key}}` template resolver for dry-run display paths.
///
/// This is **not** the full template engine — it only handles the
/// `{{inputs.xxx}}` form with exact-key matches, intended to render a
/// representative preview of resolved provider/model strings before a
/// workflow executes. The full engine (`nika_engine::binding::template`)
/// handles everything else during the real run.
pub fn simple_input_resolve<'a, I>(template: &str, inputs: I) -> String
where
    I: IntoIterator<Item = (&'a String, &'a serde_json::Value)>,
{
    let mut result = template.to_string();
    for (key, value) in inputs {
        let pattern = format!("{{{{inputs.{}}}}}", key);
        if result.contains(&pattern) {
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&pattern, &replacement);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // parse_input_value
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn input_value_string() {
        assert_eq!(parse_input_value("hello"), json!("hello"));
    }

    #[test]
    fn input_value_integer() {
        assert_eq!(parse_input_value("5"), json!(5));
        assert_eq!(parse_input_value("-42"), json!(-42));
        assert_eq!(parse_input_value("0"), json!(0));
    }

    #[test]
    fn input_value_float() {
        assert_eq!(parse_input_value("1.5"), json!(1.5));
        assert_eq!(parse_input_value("-0.5"), json!(-0.5));
    }

    #[test]
    fn input_value_bool() {
        assert_eq!(parse_input_value("true"), json!(true));
        assert_eq!(parse_input_value("false"), json!(false));
    }

    #[test]
    fn input_value_null() {
        assert_eq!(parse_input_value("null"), json!(null));
    }

    #[test]
    fn input_value_json_object() {
        assert_eq!(
            parse_input_value(r#"{"a":1,"b":"x"}"#),
            json!({"a": 1, "b": "x"})
        );
    }

    #[test]
    fn input_value_json_array() {
        assert_eq!(parse_input_value(r#"["x","y"]"#), json!(["x", "y"]));
    }

    #[test]
    fn input_value_broken_json_fallback_string() {
        assert_eq!(parse_input_value("{broken"), json!("{broken"));
        assert_eq!(parse_input_value("[not json"), json!("[not json"));
    }

    #[test]
    fn input_value_string_with_digits() {
        assert_eq!(parse_input_value("5 apples"), json!("5 apples"));
        assert_eq!(parse_input_value("v2.0"), json!("v2.0"));
    }

    #[test]
    fn input_value_scientific_notation_stays_string() {
        // Scientific notation is deliberately NOT coerced to float —
        // ambiguity with identifiers like "1e3_rev_a" would surprise users.
        assert_eq!(parse_input_value("1e3"), json!("1e3"));
        assert_eq!(parse_input_value("2E10"), json!("2E10"));
    }

    // ═══════════════════════════════════════════════════════════════
    // parse_cli_inputs
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn cli_inputs_valid() {
        let raw = vec!["locale=fr-FR".to_string(), "count=5".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0], ("locale".to_string(), json!("fr-FR")));
        assert_eq!(result[1], ("count".to_string(), json!(5)));
    }

    #[test]
    fn cli_inputs_missing_equals() {
        let raw = vec!["no-equals".to_string()];
        let result = parse_cli_inputs(&raw);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no-equals"),
            "Error should mention the input: {err}"
        );
    }

    #[test]
    fn cli_inputs_value_with_equals() {
        // key=val=ue should split on FIRST = only
        let raw = vec!["url=https://example.com?a=1".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0].1, json!("https://example.com?a=1"));
    }

    #[test]
    fn cli_inputs_empty_value() {
        let raw = vec!["key=".to_string()];
        let result = parse_cli_inputs(&raw).unwrap();
        assert_eq!(result[0].1, json!(""));
    }

    #[test]
    fn cli_inputs_http_url_without_equals_gives_url_hint() {
        // When the user drops a bare URL, the error should hint at `-i url=<url>`
        let raw = vec!["https://example.com/data".to_string()];
        let result = parse_cli_inputs(&raw);
        let err = result.unwrap_err().to_string();
        assert!(err.contains("-i url="), "Error should hint at url= form: {err}");
    }

    // ═══════════════════════════════════════════════════════════════
    // simple_input_resolve
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn simple_input_resolve_replaces_template() {
        let mut inputs = std::collections::HashMap::new();
        inputs.insert("model".to_string(), json!("gpt-4o"));
        assert_eq!(simple_input_resolve("{{inputs.model}}", &inputs), "gpt-4o");
    }

    #[test]
    fn simple_input_resolve_no_template() {
        let inputs: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        assert_eq!(simple_input_resolve("openai", &inputs), "openai");
    }

    #[test]
    fn simple_input_resolve_unresolved_stays() {
        let inputs: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        assert_eq!(
            simple_input_resolve("{{inputs.missing}}", &inputs),
            "{{inputs.missing}}"
        );
    }

    #[test]
    fn simple_input_resolve_numeric_value() {
        let mut inputs = std::collections::HashMap::new();
        inputs.insert("count".to_string(), json!(42));
        assert_eq!(simple_input_resolve("{{inputs.count}}", &inputs), "42");
    }

    // ═══════════════════════════════════════════════════════════════
    // load_input_file (pure file I/O — tests use tempfile)
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn load_input_file_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inputs.json");
        std::fs::write(&path, r#"{"topic":"AI","count":3}"#).unwrap();
        let result = load_input_file(path.to_str().unwrap()).await.unwrap();
        assert_eq!(result.len(), 2);
        // BTreeMap ordering via serde_json::Map is insertion-order since preserve_order
        let map: std::collections::HashMap<_, _> = result.into_iter().collect();
        assert_eq!(map["topic"], json!("AI"));
        assert_eq!(map["count"], json!(3));
    }

    #[tokio::test]
    async fn load_input_file_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inputs.yaml");
        std::fs::write(&path, "topic: AI\ncount: 3\n").unwrap();
        let result = load_input_file(path.to_str().unwrap()).await.unwrap();
        let map: std::collections::HashMap<_, _> = result.into_iter().collect();
        assert_eq!(map["topic"], json!("AI"));
        assert_eq!(map["count"], json!(3));
    }

    #[tokio::test]
    async fn load_input_file_not_a_mapping_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inputs.json");
        std::fs::write(&path, r#"["not", "a", "mapping"]"#).unwrap();
        let err = load_input_file(path.to_str().unwrap()).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("mapping") && msg.contains("array"),
            "Error should mention mapping vs array: {msg}"
        );
    }
}
