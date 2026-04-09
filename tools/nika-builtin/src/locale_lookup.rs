// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:locale_lookup` — BCP-47 to NLLB/ISO code mapping.
//!
//! Two-level lookup: exact match → language prefix fallback → error.
//! Replaces `nllb-terms.py` and `nllb-translate.sh` (the lookup part).

use crate::{BuiltinTool, BuiltinError, __sealed};
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub struct LocaleLookupTool;

impl __sealed::Sealed for LocaleLookupTool {}

#[derive(Debug, Deserialize)]
struct LocaleLookupParams {
    /// BCP-47 locale to look up (e.g., "fr-CA")
    locale: String,
    /// JSON object mapping locale codes to target codes
    /// e.g., {"fr": "fra_Latn", "fr-CA": "fra_Latn", "en": "eng_Latn"}
    mapping: Value,
    /// Fallback strategy: "exact" (no fallback), "language_prefix" (default)
    #[serde(default = "default_strategy")]
    fallback_strategy: String,
}

fn default_strategy() -> String {
    "language_prefix".to_string()
}

impl BuiltinTool for LocaleLookupTool {
    fn name(&self) -> &'static str {
        "locale_lookup"
    }

    fn description(&self) -> &'static str {
        "Look up a BCP-47 locale code in a mapping table with optional language-prefix fallback"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["locale", "mapping"],
            "properties": {
                "locale": {
                    "type": "string",
                    "description": "BCP-47 locale to look up (e.g., 'fr-CA')"
                },
                "mapping": {
                    "type": "object",
                    "description": "Mapping of locale codes to target codes"
                },
                "fallback_strategy": {
                    "type": "string",
                    "enum": ["exact", "language_prefix"],
                    "description": "Fallback strategy (default: language_prefix)",
                    "default": "language_prefix"
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
            let params: LocaleLookupParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:locale_lookup".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let map = match &params.mapping {
                Value::Object(m) => m,
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Array(_) => {
                    return Err(BuiltinError::Other {
                        tool: "nika:locale_lookup".into(),
                        reason: "mapping must be a JSON object".into(),
                    });
                }
            };

            let locale = &params.locale;

            // Step 1: Exact match
            if let Some(code) = map.get(locale) {
                return Ok(serde_json::json!({
                    "locale": locale,
                    "code": code,
                    "matched_via": "exact"
                })
                .to_string());
            }

            // Step 2: Language prefix fallback (if enabled)
            if params.fallback_strategy == "language_prefix" {
                // Extract language from BCP-47: "fr-CA" → "fr", "zh-Hans-CN" → "zh"
                let lang = locale.split('-').next().unwrap_or(locale);
                if lang != locale {
                    if let Some(code) = map.get(lang) {
                        return Ok(serde_json::json!({
                            "locale": locale,
                            "code": code,
                            "matched_via": "language_prefix",
                            "matched_key": lang
                        })
                        .to_string());
                    }
                }
            }

            // No match found
            Err(BuiltinError::Other {
                tool: "nika:locale_lookup".into(),
                reason: format!(
                    "No mapping found for locale '{}' (strategy: {})",
                    locale, params.fallback_strategy
                ),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_mapping() -> Value {
        json!({
            "en": "eng_Latn",
            "fr": "fra_Latn",
            "fr-CA": "fra_Latn",
            "de": "deu_Latn",
            "ja": "jpn_Jpan",
            "zh-Hans": "zho_Hans"
        })
    }

    #[tokio::test]
    async fn locale_lookup_exact() {
        let tool = LocaleLookupTool;
        let args = json!({
            "locale": "fr-CA",
            "mapping": test_mapping()
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["locale"], "fr-CA");
        assert_eq!(result["code"], "fra_Latn");
        assert_eq!(result["matched_via"], "exact");
    }

    #[tokio::test]
    async fn locale_lookup_language_prefix_fallback() {
        let tool = LocaleLookupTool;
        let args = json!({
            "locale": "fr-BE",
            "mapping": test_mapping()
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["locale"], "fr-BE");
        assert_eq!(result["code"], "fra_Latn");
        assert_eq!(result["matched_via"], "language_prefix");
        assert_eq!(result["matched_key"], "fr");
    }

    #[tokio::test]
    async fn locale_lookup_exact_only_no_fallback() {
        let tool = LocaleLookupTool;
        let args = json!({
            "locale": "fr-BE",
            "mapping": test_mapping(),
            "fallback_strategy": "exact"
        });
        let result = tool.call(args.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn locale_lookup_not_found() {
        let tool = LocaleLookupTool;
        let args = json!({
            "locale": "xx-YY",
            "mapping": test_mapping()
        });
        let result = tool.call(args.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn locale_lookup_simple_locale() {
        let tool = LocaleLookupTool;
        let args = json!({
            "locale": "ja",
            "mapping": test_mapping()
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["code"], "jpn_Jpan");
        assert_eq!(result["matched_via"], "exact");
    }
}
