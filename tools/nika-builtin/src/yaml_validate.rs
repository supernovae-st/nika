// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:yaml_validate` — Batch YAML field presence checker.
//!
//! Globs YAML files, checks required dot-path fields exist, reports failures.
//! Replaces `validate-all-locales.py`.

use crate::{BuiltinTool, BuiltinError, __sealed};
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub struct YamlValidateTool;

impl __sealed::Sealed for YamlValidateTool {}

#[derive(Debug, Deserialize)]
struct YamlValidateParams {
    /// Array of YAML data objects to validate (pre-parsed)
    data: Vec<YamlEntry>,
    /// Required dot-path fields (e.g., ["locale.key", "locale.language"])
    required_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct YamlEntry {
    /// Label for this entry (e.g., file name or locale code)
    #[serde(default)]
    label: String,
    /// The YAML content as a JSON value (already parsed)
    content: Value,
}

impl BuiltinTool for YamlValidateTool {
    fn name(&self) -> &'static str {
        "yaml_validate"
    }

    fn description(&self) -> &'static str {
        "Validate an array of YAML/JSON objects: check that required dot-path fields exist in each"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["data", "required_fields"],
            "properties": {
                "data": {
                    "type": "array",
                    "description": "Array of {label, content} objects to validate",
                    "items": {
                        "type": "object",
                        "required": ["content"],
                        "properties": {
                            "label": { "type": "string", "description": "Label for this entry" },
                            "content": { "description": "Parsed YAML/JSON content" }
                        }
                    }
                },
                "required_fields": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Dot-path fields that must exist (e.g., 'locale.key')"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: YamlValidateParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:yaml_validate".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let total = params.data.len();
            // Consolidate failures per label so each failing entry appears once with
            // ALL its missing fields listed. This avoids duplicate label entries that
            // confuse callers trying to count failing entries from the array.
            let mut failure_map: std::collections::BTreeMap<String, Vec<String>> =
                std::collections::BTreeMap::new();

            for (i, entry) in params.data.iter().enumerate() {
                let label = if entry.label.is_empty() {
                    format!("entry[{}]", i)
                } else {
                    entry.label.clone()
                };

                for field in &params.required_fields {
                    if !has_dot_path(&entry.content, field) {
                        failure_map
                            .entry(label.clone())
                            .or_default()
                            .push(format!("missing field: {}", field));
                    }
                }
            }

            let failures: Vec<serde_json::Value> = failure_map
                .iter()
                .map(|(label, errors)| {
                    serde_json::json!({
                        "label": label,
                        "errors": errors,
                        // Kept for backwards compat: first error as a single string.
                        "error": errors.first().cloned().unwrap_or_default(),
                    })
                })
                .collect();

            let valid = total - failures.len();

            let result = serde_json::json!({
                "total": total,
                "valid": valid,
                "failures": failures,
            });

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:yaml_validate".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

/// Check if a dot-path field exists in a JSON value.
/// "locale.key" checks value["locale"]["key"] exists and is not null.
fn has_dot_path(value: &Value, path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for part in &parts {
        match current.get(*part) {
            Some(v) if !v.is_null() => current = v,
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn yaml_validate_all_pass() {
        let tool = YamlValidateTool;
        let args = json!({
            "data": [
                {"label": "en", "content": {"locale": {"key": "en", "language": "English"}, "text_direction": "ltr"}},
                {"label": "fr", "content": {"locale": {"key": "fr", "language": "French"}, "text_direction": "ltr"}}
            ],
            "required_fields": ["locale.key", "locale.language", "text_direction"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["valid"], 2);
        assert!(result["failures"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn yaml_validate_missing_field() {
        let tool = YamlValidateTool;
        let args = json!({
            "data": [
                {"label": "en", "content": {"locale": {"key": "en"}}},
                {"label": "fr", "content": {"locale": {"key": "fr", "language": "French"}}}
            ],
            "required_fields": ["locale.key", "locale.language"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["total"], 2);
        let failures = result["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["label"], "en");
        assert!(failures[0]["error"]
            .as_str()
            .unwrap()
            .contains("locale.language"));
    }

    #[tokio::test]
    async fn yaml_validate_empty_data() {
        let tool = YamlValidateTool;
        let args = json!({
            "data": [],
            "required_fields": ["key"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["total"], 0);
        assert_eq!(result["valid"], 0);
    }

    #[tokio::test]
    async fn yaml_validate_multi_field_failure_consolidated() {
        // Entry "en" is missing 2 fields — must produce 1 failure entry (not 2).
        // Callers computing `total - failures.len()` must get correct results.
        let tool = YamlValidateTool;
        let args = json!({
            "data": [
                {"label": "en", "content": {}},
                {"label": "fr", "content": {"locale": {"key": "fr", "language": "French"}}}
            ],
            "required_fields": ["locale.key", "locale.language"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["valid"], 1);
        let failures = result["failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1, "multi-field failure must be consolidated into 1 entry");
        assert_eq!(failures[0]["label"], "en");
        let errors = failures[0]["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 2, "both missing fields listed in errors array");
    }

    #[test]
    fn has_dot_path_basic() {
        let data = json!({"a": {"b": {"c": 1}}});
        assert!(has_dot_path(&data, "a.b.c"));
        assert!(has_dot_path(&data, "a.b"));
        assert!(has_dot_path(&data, "a"));
        assert!(!has_dot_path(&data, "a.b.c.d"));
        assert!(!has_dot_path(&data, "x"));
    }

    #[test]
    fn has_dot_path_null_value() {
        let data = json!({"a": null});
        assert!(!has_dot_path(&data, "a"));
    }
}
