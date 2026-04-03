//! `nika:json_verify` — Translation structural validator.
//!
//! Compares a source JSON and a translated JSON for:
//! - Missing keys (in source but not translation)
//! - Extra keys (in translation but not source)
//! - Broken placeholders ({name}, {count}, etc.)
//! - Broken HTML tags
//!
//! Replaces `verify-translation.py`.

use super::BuiltinTool;
use crate::error::NikaError;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

static PLACEHOLDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{[a-zA-Z_][a-zA-Z0-9_]*\}").expect("valid regex"));

static HTML_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"</?[a-zA-Z][a-zA-Z0-9]*(?:\s[^>]*)?\s*/?>").expect("valid regex"));

pub struct JsonVerifyTool;

#[derive(Debug, Deserialize)]
struct JsonVerifyParams {
    /// Source JSON (reference/original)
    source: Value,
    /// Translated JSON to verify
    translation: Value,
}

impl BuiltinTool for JsonVerifyTool {
    fn name(&self) -> &'static str {
        "json_verify"
    }

    fn description(&self) -> &'static str {
        "Verify translated JSON structure against source: missing keys, extra keys, broken placeholders, broken HTML tags"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["source", "translation"],
            "properties": {
                "source": {
                    "description": "Source JSON (reference/original language)"
                },
                "translation": {
                    "description": "Translated JSON to verify against source"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: JsonVerifyParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:json_verify".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let result = verify(&params.source, &params.translation);

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:json_verify".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

/// Flatten a nested JSON object into dot-separated key paths.
fn flatten_keys(obj: &Value, prefix: &str) -> BTreeMap<String, Value> {
    let mut items = BTreeMap::new();
    if let Value::Object(map) = obj {
        for (k, v) in map {
            let path = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            if v.is_object() {
                items.extend(flatten_keys(v, &path));
            } else {
                items.insert(path, v.clone());
            }
        }
    }
    items
}

/// Extract {placeholder} tokens from a string value.
fn extract_placeholders(val: &Value) -> BTreeSet<String> {
    match val {
        Value::String(s) => PLACEHOLDER_RE
            .find_iter(s)
            .map(|m| m.as_str().to_string())
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Extract HTML tags from a string value.
fn extract_html_tags(val: &Value) -> Vec<String> {
    match val {
        Value::String(s) => {
            let mut tags: Vec<String> = HTML_TAG_RE
                .find_iter(s)
                .map(|m| m.as_str().to_string())
                .collect();
            tags.sort();
            tags
        }
        _ => Vec::new(),
    }
}

/// Compare source and translation JSON structures.
fn verify(source: &Value, translation: &Value) -> Value {
    let source_flat = flatten_keys(source, "");
    let trans_flat = flatten_keys(translation, "");

    let source_keys: BTreeSet<&String> = source_flat.keys().collect();
    let trans_keys: BTreeSet<&String> = trans_flat.keys().collect();

    let missing_keys: Vec<&String> = source_keys.difference(&trans_keys).copied().collect();
    let extra_keys: Vec<&String> = trans_keys.difference(&source_keys).copied().collect();

    let mut broken_placeholders = Vec::new();
    let mut broken_html = Vec::new();

    for key in source_keys.intersection(&trans_keys) {
        let src_val = &source_flat[*key];
        let trs_val = &trans_flat[*key];

        // Check placeholders
        let src_ph = extract_placeholders(src_val);
        let trs_ph = extract_placeholders(trs_val);
        if !src_ph.is_empty() && src_ph != trs_ph {
            let missing: Vec<&String> = src_ph.difference(&trs_ph).collect();
            if !missing.is_empty() {
                broken_placeholders.push(serde_json::json!({
                    "key": key,
                    "missing": missing,
                }));
            }
        }

        // Check HTML tags
        let src_tags = extract_html_tags(src_val);
        let trs_tags = extract_html_tags(trs_val);
        if !src_tags.is_empty() && src_tags != trs_tags {
            broken_html.push(serde_json::json!({
                "key": key,
                "source_tags": src_tags,
                "translation_tags": trs_tags,
            }));
        }
    }

    let passed =
        missing_keys.is_empty() && broken_placeholders.is_empty() && broken_html.is_empty();

    serde_json::json!({
        "pass": passed,
        "total_keys": source_keys.len(),
        "missing_keys": missing_keys,
        "extra_keys": extra_keys,
        "broken_placeholders": broken_placeholders,
        "broken_html": broken_html,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn json_verify_identical() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"greeting": "Hello {name}", "bye": "Goodbye"},
            "translation": {"greeting": "Bonjour {name}", "bye": "Au revoir"}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], true);
        assert_eq!(result["total_keys"], 2);
        assert!(result["missing_keys"].as_array().unwrap().is_empty());
        assert!(result["extra_keys"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn json_verify_missing_key() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"a": "hello", "b": "world"},
            "translation": {"a": "bonjour"}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], false);
        assert_eq!(result["missing_keys"], json!(["b"]));
    }

    #[tokio::test]
    async fn json_verify_extra_key() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"a": "hello"},
            "translation": {"a": "bonjour", "extra": "oops"}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        // extra keys don't cause failure (pass still true if no missing/broken)
        assert_eq!(result["pass"], true);
        assert_eq!(result["extra_keys"], json!(["extra"]));
    }

    #[tokio::test]
    async fn json_verify_broken_placeholder() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"greeting": "Hello {name}, you have {count} items"},
            "translation": {"greeting": "Bonjour, vous avez {count} articles"}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], false);
        let broken = &result["broken_placeholders"].as_array().unwrap()[0];
        assert_eq!(broken["key"], "greeting");
        assert!(broken["missing"].as_array().unwrap().contains(&json!("{name}")));
    }

    #[tokio::test]
    async fn json_verify_broken_html() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"msg": "Click <a href='#'>here</a> to continue"},
            "translation": {"msg": "Cliquez ici pour continuer"}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], false);
        assert!(!result["broken_html"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn json_verify_nested_keys() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {"ui": {"button": {"save": "Save", "cancel": "Cancel"}}},
            "translation": {"ui": {"button": {"save": "Enregistrer"}}}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], false);
        assert_eq!(result["missing_keys"], json!(["ui.button.cancel"]));
    }

    #[tokio::test]
    async fn json_verify_empty_objects() {
        let tool = JsonVerifyTool;
        let args = json!({
            "source": {},
            "translation": {}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["pass"], true);
        assert_eq!(result["total_keys"], 0);
    }

    #[test]
    fn flatten_nested() {
        let data = json!({"a": {"b": {"c": 1}}, "d": 2});
        let flat = flatten_keys(&data, "");
        assert_eq!(flat.len(), 2);
        assert_eq!(flat["a.b.c"], json!(1));
        assert_eq!(flat["d"], json!(2));
    }

    #[test]
    fn extract_placeholders_basic() {
        let val = json!("Hello {name}, you have {count} items");
        let ph = extract_placeholders(&val);
        assert!(ph.contains("{name}"));
        assert!(ph.contains("{count}"));
        assert_eq!(ph.len(), 2);
    }

    #[test]
    fn extract_html_tags_basic() {
        let val = json!("Click <a href='#'>here</a> or <br /> break");
        let tags = extract_html_tags(&val);
        assert_eq!(tags.len(), 3); // <a href='#'>, </a>, <br />
    }
}
