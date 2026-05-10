// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model pricing catalog codegen — TOML schema, validation, Rust emission.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{opt_f64, rstr};
use crate::error::CodegenError;
use crate::schema::{PRICING_SCHEMA, assert_schema};

// ─── TOML schema ────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PricingFile {
    pub schema: String,
    pub rules: Vec<PricingEntry>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PricingEntry {
    pub provider: String,
    pub model_pattern: String,
    pub input_per_million: f64,
    pub output_per_million: f64,
    #[serde(default)]
    pub cache_write_per_million: Option<f64>,
    #[serde(default)]
    pub cache_read_per_million: Option<f64>,
    #[serde(default)]
    pub image_per_million: Option<f64>,
    #[serde(default)]
    pub reasoning_tokens_per_million: Option<f64>,
}

// ─── Public codegen entry ───────────────────────────────────────────────

pub fn parse_pricing_bytes(raw: &[u8], path: &Path) -> Result<Vec<PricingEntry>, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let file: PricingFile = toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
        path: path.to_path_buf(),
        source,
    })?;

    assert_schema(PRICING_SCHEMA, &file.schema, path)?;

    validate_pricing(&file.rules, path)?;
    Ok(file.rules)
}

pub fn codegen_pricing(toml_bytes: &[u8]) -> Result<String, CodegenError> {
    let synthetic = Path::new("<bytes>:model-pricing.toml");
    let entries = parse_pricing_bytes(toml_bytes, synthetic)?;
    Ok(generate_pricing_rs(&entries))
}

// ─── Validation ─────────────────────────────────────────────────────────

fn validate_pricing(rules: &[PricingEntry], path: &Path) -> Result<(), CodegenError> {
    let path_ctx = path.display().to_string();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (i, entry) in rules.iter().enumerate() {
        let ctx = format!(
            "{}: pricing rule #{} ({}/{})",
            path_ctx,
            i + 1,
            entry.provider,
            entry.model_pattern
        );

        if entry.provider.is_empty() {
            return Err(CodegenError::schema_validation(
                ctx,
                "provider is empty".to_string(),
            ));
        }
        if entry.model_pattern.is_empty() {
            return Err(CodegenError::schema_validation(
                ctx,
                "model_pattern is empty".to_string(),
            ));
        }

        let key = (
            entry.provider.to_ascii_lowercase(),
            entry.model_pattern.to_ascii_lowercase(),
        );
        if !seen.insert(key) {
            return Err(CodegenError::schema_validation(
                ctx,
                "duplicate (provider, model_pattern) pair".to_string(),
            ));
        }

        validate_rate(entry.input_per_million, "input_per_million", &ctx)?;
        validate_rate(entry.output_per_million, "output_per_million", &ctx)?;
        if let Some(v) = entry.cache_write_per_million {
            validate_rate(v, "cache_write_per_million", &ctx)?;
        }
        if let Some(v) = entry.cache_read_per_million {
            validate_rate(v, "cache_read_per_million", &ctx)?;
        }
        if let Some(v) = entry.image_per_million {
            validate_rate(v, "image_per_million", &ctx)?;
        }
        if let Some(v) = entry.reasoning_tokens_per_million {
            validate_rate(v, "reasoning_tokens_per_million", &ctx)?;
        }
    }

    // Ordering invariant: within each provider, more specific patterns must come first.
    let mut by_provider: std::collections::BTreeMap<String, Vec<&str>> =
        std::collections::BTreeMap::new();
    for entry in rules {
        by_provider
            .entry(entry.provider.clone())
            .or_default()
            .push(&entry.model_pattern);
    }
    for (provider, patterns) in &by_provider {
        for i in 0..patterns.len() {
            for j in (i + 1)..patterns.len() {
                if patterns[j].contains(patterns[i]) && patterns[j] != patterns[i] {
                    return Err(CodegenError::schema_validation(
                        path_ctx.clone(),
                        format!(
                            "pricing {provider}: pattern {:?} contains {:?} — \
                             longer/more-specific pattern must come first",
                            patterns[j], patterns[i],
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_rate(v: f64, field: &str, ctx: &str) -> Result<(), CodegenError> {
    if !v.is_finite() {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!("{field} must be finite (got {v})"),
        ));
    }
    if v < 0.0 {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!("{field} must be non-negative (got {v})"),
        ));
    }
    Ok(())
}

// ─── Rust source emission ───────────────────────────────────────────────

#[must_use]
pub(crate) fn generate_pricing_rs(entries: &[PricingEntry]) -> String {
    let mut out = String::with_capacity(8_192);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/model-pricing.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub(crate) static ALL_PRICING: &[crate::types::model::ModelPricing] = &["
    );

    for entry in entries {
        emit_pricing_entry(&mut out, entry);
    }

    let _ = writeln!(out, "];");
    out
}

fn emit_pricing_entry(out: &mut String, e: &PricingEntry) {
    let _ = writeln!(out, "    crate::types::model::ModelPricing {{");
    let _ = writeln!(out, "        provider: {},", rstr(&e.provider));
    let _ = writeln!(out, "        model_pattern: {},", rstr(&e.model_pattern));
    let _ = writeln!(out, "        input_per_million: {:?},", e.input_per_million);
    let _ = writeln!(
        out,
        "        output_per_million: {:?},",
        e.output_per_million
    );
    let _ = writeln!(
        out,
        "        cache_write_per_million: {},",
        opt_f64(e.cache_write_per_million)
    );
    let _ = writeln!(
        out,
        "        cache_read_per_million: {},",
        opt_f64(e.cache_read_per_million)
    );
    let _ = writeln!(
        out,
        "        image_per_million: {},",
        opt_f64(e.image_per_million)
    );
    let _ = writeln!(
        out,
        "        reasoning_tokens_per_million: {},",
        opt_f64(e.reasoning_tokens_per_million)
    );
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_entry() -> PricingEntry {
        PricingEntry {
            provider: "demo".to_string(),
            model_pattern: "demo-large".to_string(),
            input_per_million: 1.0,
            output_per_million: 2.0,
            cache_write_per_million: None,
            cache_read_per_million: None,
            image_per_million: None,
            reasoning_tokens_per_million: None,
        }
    }

    #[test]
    fn validate_rate_rejects_nan() {
        let e = validate_rate(f64::NAN, "x", "ctx").unwrap_err();
        let msg = format!("{e}");
        assert!(msg.contains("must be finite"), "got: {msg}");
    }

    #[test]
    fn validate_rate_rejects_negative() {
        let e = validate_rate(-1.0, "x", "ctx").unwrap_err();
        let msg = format!("{e}");
        assert!(msg.contains("non-negative"), "got: {msg}");
    }

    #[test]
    fn validate_rate_accepts_zero_and_positive() {
        validate_rate(0.0, "x", "ctx").unwrap();
        validate_rate(100.0, "x", "ctx").unwrap();
    }

    #[test]
    fn validate_pricing_rejects_duplicate_key() {
        let e1 = fixture_entry();
        let e2 = fixture_entry();
        let err = validate_pricing(&[e1, e2], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate"), "got: {msg}");
    }

    #[test]
    fn validate_pricing_specificity_ordering() {
        // "demo-large-v2" contains "demo-large" — the longer pattern MUST
        // come first. This sample violates: shorter first → reject.
        let e1 = PricingEntry {
            model_pattern: "demo-large".to_string(),
            ..fixture_entry()
        };
        let e2 = PricingEntry {
            model_pattern: "demo-large-v2".to_string(),
            ..fixture_entry()
        };
        let err = validate_pricing(&[e1, e2], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("more-specific pattern must come first"),
            "got: {msg}"
        );
    }

    #[test]
    fn validate_pricing_accepts_correct_ordering() {
        let e1 = PricingEntry {
            model_pattern: "demo-large-v2".to_string(),
            ..fixture_entry()
        };
        let e2 = PricingEntry {
            model_pattern: "demo-large".to_string(),
            ..fixture_entry()
        };
        validate_pricing(&[e1, e2], Path::new("/x")).unwrap();
    }

    #[test]
    fn validate_pricing_empty_provider_rejected() {
        let mut e = fixture_entry();
        e.provider.clear();
        let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("provider is empty"), "got: {msg}");
    }

    #[test]
    fn validate_pricing_empty_pattern_rejected() {
        let mut e = fixture_entry();
        e.model_pattern.clear();
        let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("model_pattern is empty"), "got: {msg}");
    }

    #[test]
    fn generate_emits_all_seven_axes() {
        let mut e = fixture_entry();
        e.cache_write_per_million = Some(0.5);
        e.cache_read_per_million = Some(0.1);
        e.image_per_million = Some(2.0);
        e.reasoning_tokens_per_million = Some(3.0);
        let s = generate_pricing_rs(&[e]);
        assert!(s.contains("input_per_million"));
        assert!(s.contains("output_per_million"));
        assert!(s.contains("cache_write_per_million: Some"));
        assert!(s.contains("cache_read_per_million: Some"));
        assert!(s.contains("image_per_million: Some"));
        assert!(s.contains("reasoning_tokens_per_million: Some"));
    }

    #[test]
    fn generate_emits_canonical_header_and_static() {
        let s = generate_pricing_rs(&[fixture_entry()]);
        assert!(s.contains("// GENERATED by build.rs from data/model-pricing.toml. DO NOT EDIT."));
        assert!(s.contains("pub(crate) static ALL_PRICING"));
        assert!(s.contains("crate::types::model::ModelPricing"));
    }

    #[test]
    fn idempotent_emission_pricing() {
        let e = fixture_entry();
        let a = generate_pricing_rs(&[fixture_entry()]);
        let b = generate_pricing_rs(&[e]);
        assert_eq!(a, b);
    }
}
