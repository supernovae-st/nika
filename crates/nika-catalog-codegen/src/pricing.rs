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
    pub meta: PricingMeta,
    pub rules: Vec<PricingEntry>,
}

/// Snapshot provenance — the machine-readable identity of the vendored
/// pricing catalog (schema `@1.1`; until then the date/sha lived in TOML
/// comments and nothing could answer « how old is your pricing? »).
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PricingMeta {
    /// Upstream source URL (e.g. `https://models.dev/api.json`).
    pub source: String,
    /// Snapshot date, ISO `YYYY-MM-DD` (the day the refresh ran).
    pub as_of: String,
    /// First 16 hex chars of the upstream payload's sha256.
    pub source_sha256_16: String,
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

pub fn parse_pricing_bytes(raw: &[u8], path: &Path) -> Result<PricingFile, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let file: PricingFile = toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
        path: path.to_path_buf(),
        source,
    })?;

    assert_schema(PRICING_SCHEMA, &file.schema, path)?;

    validate_meta(&file.meta, path)?;
    validate_pricing(&file.rules, path)?;
    Ok(file)
}

pub fn codegen_pricing(toml_bytes: &[u8]) -> Result<String, CodegenError> {
    let synthetic = Path::new("<bytes>:model-pricing.toml");
    let file = parse_pricing_bytes(toml_bytes, synthetic)?;
    Ok(generate_pricing_rs(&file.meta, &file.rules))
}

// ─── Validation ─────────────────────────────────────────────────────────

/// Snapshot provenance must be machine-usable: a real URL, a real ISO
/// date, a real sha prefix — a malformed meta is a refresh-script bug
/// and must fail the BUILD, not surface as garbage in `nika doctor`.
fn validate_meta(meta: &PricingMeta, path: &Path) -> Result<(), CodegenError> {
    let ctx = format!("{}: [meta]", path.display());
    if !(meta.source.starts_with("https://") || meta.source.starts_with("http://")) {
        return Err(CodegenError::schema_validation(
            ctx,
            format!("meta.source must be a URL (got {:?})", meta.source),
        ));
    }
    if !is_iso_date(&meta.as_of) {
        return Err(CodegenError::schema_validation(
            ctx,
            format!("meta.as_of must be YYYY-MM-DD (got {:?})", meta.as_of),
        ));
    }
    let sha = &meta.source_sha256_16;
    if sha.len() != 16
        || !sha
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(CodegenError::schema_validation(
            ctx,
            format!("meta.source_sha256_16 must be 16 lowercase hex chars (got {sha:?})"),
        ));
    }
    Ok(())
}

/// `YYYY-MM-DD` with sane month/day ranges (no chrono dep — the codegen
/// stays std-only; leap-day precision is not load-bearing for a staleness
/// stamp).
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: core::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    let num = |r: core::ops::Range<usize>| s[r].parse::<u8>().unwrap_or(0);
    (1..=12).contains(&num(5..7)) && (1..=31).contains(&num(8..10))
}

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
pub(crate) fn generate_pricing_rs(meta: &PricingMeta, entries: &[PricingEntry]) -> String {
    let mut out = String::with_capacity(8_192);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/model-pricing.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub(crate) static PRICING_SNAPSHOT: crate::types::model::PricingSnapshot ="
    );
    let _ = writeln!(out, "    crate::types::model::PricingSnapshot::new(");
    let _ = writeln!(out, "        {},", rstr(&meta.source));
    let _ = writeln!(out, "        {},", rstr(&meta.as_of));
    let _ = writeln!(out, "        {},", rstr(&meta.source_sha256_16));
    let _ = writeln!(out, "    );");
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

    fn fixture_meta() -> PricingMeta {
        PricingMeta {
            source: "https://models.dev/api.json".to_string(),
            as_of: "2026-07-07".to_string(),
            source_sha256_16: "0123456789abcdef".to_string(),
        }
    }

    // ── [meta] validation — a malformed provenance fails the BUILD ──

    #[test]
    fn validate_meta_accepts_canonical() {
        validate_meta(&fixture_meta(), Path::new("/x")).unwrap();
    }

    #[test]
    fn validate_meta_rejects_non_url_source() {
        let mut m = fixture_meta();
        m.source = "models.dev".to_string();
        let msg = format!("{}", validate_meta(&m, Path::new("/x")).unwrap_err());
        assert!(msg.contains("must be a URL"), "got: {msg}");
    }

    #[test]
    fn validate_meta_rejects_bad_dates() {
        for bad in [
            "2026-7-07",
            "2026-13-01",
            "2026-00-10",
            "2026-01-32",
            "yesterday",
            "",
        ] {
            let mut m = fixture_meta();
            m.as_of = bad.to_string();
            let err = validate_meta(&m, Path::new("/x")).unwrap_err();
            assert!(
                format!("{err}").contains("YYYY-MM-DD"),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn validate_meta_rejects_bad_sha() {
        for bad in [
            "abc",
            "0123456789ABCDEF",
            "0123456789abcdeg",
            "0123456789abcdef00",
        ] {
            let mut m = fixture_meta();
            m.source_sha256_16 = bad.to_string();
            let err = validate_meta(&m, Path::new("/x")).unwrap_err();
            assert!(
                format!("{err}").contains("16 lowercase hex"),
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn is_iso_date_boundaries() {
        assert!(is_iso_date("2026-01-01"));
        assert!(is_iso_date("2026-12-31"));
        assert!(!is_iso_date("2026-12-3"));
        assert!(!is_iso_date("2026/12/31"));
        assert!(!is_iso_date("2026-12-311"));
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
        let s = generate_pricing_rs(&fixture_meta(), &[e]);
        assert!(s.contains("input_per_million"));
        assert!(s.contains("output_per_million"));
        assert!(s.contains("cache_write_per_million: Some"));
        assert!(s.contains("cache_read_per_million: Some"));
        assert!(s.contains("image_per_million: Some"));
        assert!(s.contains("reasoning_tokens_per_million: Some"));
    }

    #[test]
    fn generate_emits_canonical_header_and_static() {
        let s = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        assert!(s.contains("// GENERATED by build.rs from data/model-pricing.toml. DO NOT EDIT."));
        assert!(s.contains("pub(crate) static ALL_PRICING"));
        assert!(s.contains("crate::types::model::ModelPricing"));
    }

    #[test]
    fn generate_emits_snapshot_provenance() {
        // The machine-readable identity — source · as_of · sha ride the
        // emitted file as ONE const-constructed static (never comments).
        let s = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        assert!(s.contains("pub(crate) static PRICING_SNAPSHOT"));
        assert!(s.contains("crate::types::model::PricingSnapshot::new("));
        assert!(s.contains("\"https://models.dev/api.json\""));
        assert!(s.contains("\"2026-07-07\""));
        assert!(s.contains("\"0123456789abcdef\""));
    }

    #[test]
    fn idempotent_emission_pricing() {
        let e = fixture_entry();
        let a = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        let b = generate_pricing_rs(&fixture_meta(), &[e]);
        assert_eq!(a, b);
    }

    #[test]
    fn parse_rejects_missing_meta() {
        // @1.1 requires [meta] — a metaless file is a refresh-script bug.
        let toml = b"schema = \"nika/model-pricing@1.1\"\n\n[[rules]]\nprovider = \"p\"\nmodel_pattern = \"m\"\ninput_per_million = 1.0\noutput_per_million = 2.0\n";
        let err = parse_pricing_bytes(toml, Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("meta"), "got: {err}");
    }
}
