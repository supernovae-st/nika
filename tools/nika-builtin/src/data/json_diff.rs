// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:json_diff — Compare source and target JSON for translation QA.
//!
//! Returns:
//! - `identical_values`: keys where source == target (suspicious untranslated strings)
//! - `modified_values`: keys where source != target (translated)
//! - `added_keys`: keys only in target
//! - `removed_keys`: keys only in source
//! - `length_ratios`: per-key string length comparison
//! - `suspicious`: keys flagged for review (untranslated or extreme ratio)

use crate::{BuiltinTool, BuiltinError, __sealed};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;

pub struct JsonDiffTool;

impl __sealed::Sealed for JsonDiffTool {}

#[derive(Debug, Deserialize)]
struct JsonDiffParams {
    /// Source JSON (original language)
    source: Value,
    /// Target JSON (translated)
    target: Value,
}

impl BuiltinTool for JsonDiffTool {
    fn name(&self) -> &'static str {
        "json_diff"
    }

    fn description(&self) -> &'static str {
        "Compare source and target JSON objects for translation QA. Reports identical/modified values, added/removed keys, length ratios, and suspicious entries."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
          "type": "object",
          "required": ["source", "target"],
          "properties": {
            "source": {
              "description": "Source JSON object (original language)"
            },
            "target": {
              "description": "Target JSON object (translated)"
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
            let params: JsonDiffParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:json_diff".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let source = flatten_json(&params.source, "");
            let target = flatten_json(&params.target, "");

            let source_keys: BTreeSet<&String> = source.keys().collect();
            let target_keys: BTreeSet<&String> = target.keys().collect();

            let mut identical_values = Vec::new();
            let mut modified_values = Vec::new();
            let mut length_ratios = serde_json::Map::new();
            let mut suspicious = Vec::new();

            // Compare shared keys
            for key in source_keys.intersection(&target_keys) {
                let sv = &source[*key];
                let tv = &target[*key];

                if sv == tv {
                    identical_values.push(Value::String((*key).clone()));
                    // Identical string values are suspicious (untranslated)
                    if sv.is_string() {
                        suspicious.push(Value::String((*key).clone()));
                    }
                } else {
                    modified_values.push(Value::String((*key).clone()));

                    // Compute length ratio for string values
                    if let (Some(ss), Some(ts)) = (sv.as_str(), tv.as_str()) {
                        let src_len = ss.chars().count();
                        let tgt_len = ts.chars().count();
                        let ratio = if src_len > 0 {
                            tgt_len as f64 / src_len as f64
                        } else {
                            0.0
                        };
                        length_ratios.insert(
                            (*key).clone(),
                            json!({
                              "source_len": src_len,
                              "target_len": tgt_len,
                              "ratio": (ratio * 100.0).round() / 100.0
                            }),
                        );
                        // Flag extreme ratios
                        if ratio > 3.0 || (ratio < 0.3 && ratio > 0.0) {
                            suspicious.push(Value::String((*key).clone()));
                        }
                    }
                }
            }

            // Keys only in source (removed during translation)
            let removed_keys: Vec<Value> = source_keys
                .difference(&target_keys)
                .map(|k| Value::String((*k).clone()))
                .collect();

            // Keys only in target (added during translation)
            let added_keys: Vec<Value> = target_keys
                .difference(&source_keys)
                .map(|k| Value::String((*k).clone()))
                .collect();

            // Added/removed keys are suspicious
            for k in &removed_keys {
                suspicious.push(k.clone());
            }
            for k in &added_keys {
                suspicious.push(k.clone());
            }

            let result = json!({
              "identical_values": identical_values,
              "modified_values": modified_values,
              "added_keys": added_keys,
              "removed_keys": removed_keys,
              "length_ratios": Value::Object(length_ratios),
              "suspicious": suspicious,
            });

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:json_diff".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

/// Flatten a nested JSON object into dot-path → value pairs.
fn flatten_json(value: &Value, prefix: &str) -> std::collections::BTreeMap<String, Value> {
    let mut result = std::collections::BTreeMap::new();
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                match v {
                    Value::Object(_) => {
                        result.extend(flatten_json(v, &key));
                    }
                    Value::Null
                    | Value::Bool(_)
                    | Value::Number(_)
                    | Value::String(_)
                    | Value::Array(_) => {
                        result.insert(key, v.clone());
                    }
                }
            }
        }
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Array(_) => {
            if !prefix.is_empty() {
                result.insert(prefix.to_string(), value.clone());
            }
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn json_diff_basic_translation() {
        let tool = JsonDiffTool;
        let result = tool
      .call(
        serde_json::to_string(&json!({
          "source": {"hero_title": "Welcome to Nika", "cta": "Get Started", "nav.settings": "Settings"},
          "target": {"hero_title": "Bienvenue sur Nika", "cta": "Commencer", "nav.settings": "Settings"}
        }))
        .unwrap(),
      )
      .await
      .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        // "nav.settings" is identical — suspicious
        assert!(parsed["identical_values"]
            .as_array()
            .unwrap()
            .contains(&json!("nav.settings")));
        assert!(parsed["suspicious"]
            .as_array()
            .unwrap()
            .contains(&json!("nav.settings")));

        // hero_title and cta are modified
        let modified = parsed["modified_values"].as_array().unwrap();
        assert!(modified.contains(&json!("cta")));
        assert!(modified.contains(&json!("hero_title")));

        // length_ratios should exist for modified string values
        assert!(parsed["length_ratios"]["hero_title"].is_object());
    }

    #[tokio::test]
    async fn json_diff_added_removed_keys() {
        let tool = JsonDiffTool;
        let result = tool
            .call(
                serde_json::to_string(&json!({
                  "source": {"a": "1", "b": "2"},
                  "target": {"b": "deux", "c": "3"}
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert!(parsed["removed_keys"]
            .as_array()
            .unwrap()
            .contains(&json!("a")));
        assert!(parsed["added_keys"]
            .as_array()
            .unwrap()
            .contains(&json!("c")));
    }

    #[tokio::test]
    async fn json_diff_nested_objects() {
        let tool = JsonDiffTool;
        let result = tool
            .call(
                serde_json::to_string(&json!({
                  "source": {"nav": {"home": "Home", "about": "About"}},
                  "target": {"nav": {"home": "Accueil", "about": "About"}}
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        // nav.about is identical (untranslated) — suspicious
        assert!(parsed["identical_values"]
            .as_array()
            .unwrap()
            .contains(&json!("nav.about")));
        assert!(parsed["modified_values"]
            .as_array()
            .unwrap()
            .contains(&json!("nav.home")));
    }

    #[tokio::test]
    async fn json_diff_extreme_ratio_flagged() {
        let tool = JsonDiffTool;
        let result = tool
            .call(
                serde_json::to_string(&json!({
                  "source": {"k": "Hi"},
                  "target": {"k": "This is an extremely verbose and unnecessarily long translation"}
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        // Ratio > 3.0 should be flagged
        assert!(parsed["suspicious"]
            .as_array()
            .unwrap()
            .contains(&json!("k")));
        let ratio = parsed["length_ratios"]["k"]["ratio"].as_f64().unwrap();
        assert!(ratio > 3.0, "Ratio should be > 3.0, got {ratio}");
    }

    #[tokio::test]
    async fn json_diff_empty_objects() {
        let tool = JsonDiffTool;
        let result = tool
            .call(
                serde_json::to_string(&json!({
                  "source": {},
                  "target": {}
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["identical_values"], json!([]));
        assert_eq!(parsed["modified_values"], json!([]));
        assert_eq!(parsed["added_keys"], json!([]));
        assert_eq!(parsed["removed_keys"], json!([]));
    }

    #[test]
    fn flatten_json_simple() {
        let input = json!({"a": 1, "b": {"c": 2, "d": 3}});
        let flat = flatten_json(&input, "");
        assert_eq!(flat["a"], json!(1));
        assert_eq!(flat["b.c"], json!(2));
        assert_eq!(flat["b.d"], json!(3));
        assert_eq!(flat.len(), 3);
    }
}
