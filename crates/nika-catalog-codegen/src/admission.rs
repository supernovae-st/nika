// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Exact tariff bindings. Private generator; no runtime parser or IO.
use crate::CodegenError;
use serde::Deserialize;
use std::fmt::Write as _;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: String,
    tariffs: Vec<Tariff>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tariff {
    provider: String,
    billing_provider: String,
    currency: String,
    output_token_param: String,
    model: String,
    endpoints: Vec<String>,
    source: String,
    limits_source: String,
    route_source: String,
    as_of: String,
    source_sha256: String,
    limits_sha256: String,
    context_tokens: u64,
    max_output_tokens: u32,
    input_nano_per_token: u64,
    output_nano_per_token: u64,
    cached_nano_per_token: u64,
}
pub(crate) fn generate(raw: &[u8]) -> Result<String, CodegenError> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| CodegenError::schema_validation("admission", e.to_string()))?;
    let file: File = toml::from_str(text).map_err(|source| CodegenError::TomlParse {
        path: "inference-admission.toml".into(),
        source,
    })?;
    let mut out = String::from("static TARIFFS: &[InferenceTariff] = &[\n");
    let mut seen = std::collections::HashSet::new();
    if file.schema != "nika/inference-admission@1.1" {
        return Err(CodegenError::schema_validation(
            "admission",
            "unknown schema",
        ));
    }
    for r in file.tariffs {
        if r.provider.is_empty()
            || r.model.is_empty()
            || r.billing_provider.is_empty()
            || !matches!(r.currency.as_str(), "USD" | "EUR")
            || !matches!(
                r.output_token_param.as_str(),
                "max_tokens" | "max_completion_tokens"
            )
            || r.endpoints.is_empty()
            || r.endpoints
                .iter()
                .any(|e| !e.starts_with("https://") || e.contains(['?', '#']))
            || !r.source.starts_with("https://")
            || !r.limits_source.starts_with("https://")
            || !r.route_source.starts_with("https://")
            || r.as_of.len() != 10
            || [&r.source_sha256, &r.limits_sha256].iter().any(|s| {
                !s.is_empty() && (s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            })
            || (r.currency == "USD" && (r.source_sha256.is_empty() || r.limits_sha256.is_empty()))
            || r.context_tokens == 0
            || r.max_output_tokens == 0
            || r.input_nano_per_token == 0
            || r.output_nano_per_token == 0
            || r.cached_nano_per_token == 0
            || r.cached_nano_per_token > r.input_nano_per_token
            || [
                r.input_nano_per_token,
                r.output_nano_per_token,
                r.cached_nano_per_token,
            ]
            .iter()
            .any(|n| *n > 9_007_199_254_740_991)
            || !seen.insert((r.provider.clone(), r.model.clone()))
        {
            return Err(CodegenError::schema_validation(
                "admission",
                "invalid or duplicate qualified tariff",
            ));
        }
        writeln!(
            out,
            concat!(
                "InferenceTariff {{ provider: {:?}, billing_provider: {:?}, currency: {:?}, output_token_param: {:?}, model: {:?}, endpoints: &{:?}, ",
                "source: {:?}, limits_source: {:?}, route_source: {:?}, as_of: {:?}, source_sha256: {:?}, ",
                "limits_sha256: {:?}, context_tokens: {}, max_output_tokens: {}, ",
                "input: {}, output: {}, cached: {} }},"
            ),
            r.provider,
            r.billing_provider,
            r.currency,
            r.output_token_param,
            r.model,
            r.endpoints,
            r.source,
            r.limits_source,
            r.route_source,
            r.as_of,
            r.source_sha256,
            r.limits_sha256,
            integer_literal(r.context_tokens),
            integer_literal(u64::from(r.max_output_tokens)),
            integer_literal(r.input_nano_per_token),
            integer_literal(r.output_nano_per_token),
            integer_literal(r.cached_nano_per_token)
        )
        .map_err(|error| CodegenError::schema_validation("admission", error.to_string()))?;
    }
    out.push_str("];\n");
    Ok(out)
}

/// Project exact first-party USD facts from their owning admission source.
/// The models.dev file and its historical identity remain untouched. Route
/// qualification is still required at execution; a catalog row is not a grant.
pub(crate) fn project_pricing(
    raw: &[u8],
    pricing: &mut crate::pricing::PricingFile,
) -> Result<(), CodegenError> {
    generate(raw)?;
    let text = std::str::from_utf8(raw)
        .map_err(|e| CodegenError::schema_validation("admission", e.to_string()))?;
    let file: File = toml::from_str(text).map_err(|source| CodegenError::TomlParse {
        path: "inference-admission.toml".into(),
        source,
    })?;
    for t in file.tariffs {
        if t.currency != "USD" || t.provider != t.billing_provider {
            continue;
        }
        if let Some(row) = pricing
            .rules
            .iter_mut()
            .find(|r| r.provider == t.provider && r.model_pattern == t.model)
        {
            // Qualified rates are integers below 2^53; validation above owns bounds.
            #[allow(clippy::cast_precision_loss)]
            {
                row.input_per_million = t.input_nano_per_token as f64 / 1000.0;
                row.output_per_million = t.output_nano_per_token as f64 / 1000.0;
                row.cache_read_per_million = Some(t.cached_nano_per_token as f64 / 1000.0);
            }
            row.cache_write_per_million = None;
            row.reasoning_tokens_per_million = None;
        }
    }
    Ok(())
}

fn integer_literal(value: u64) -> String {
    let digits = value.to_string();
    let mut literal = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            literal.push('_');
        }
        literal.push(digit);
    }
    literal
}
#[cfg(test)]
mod tests {
    use super::*;
    const DATA: &[u8] = include_bytes!("../../nika-catalog/data/inference-admission.toml");
    #[test]
    fn exact_projection_replaces_stale_usd_and_never_projects_eur() {
        let raw = include_bytes!("../../nika-catalog/data/model-pricing.toml");
        let mut p = crate::pricing::parse_pricing_bytes(raw, std::path::Path::new("snapshot"))
            .expect("snapshot");
        let before = p
            .rules
            .iter()
            .find(|r| r.provider == "deepseek" && r.model_pattern == "deepseek-v4-pro")
            .expect("old")
            .input_per_million;
        assert!((before - 0.435).abs() < f64::EPSILON);
        project_pricing(DATA, &mut p).expect("projection");
        let row = p
            .rules
            .iter()
            .find(|r| r.provider == "deepseek" && r.model_pattern == "deepseek-v4-pro")
            .expect("projected");
        assert_eq!(
            (
                row.input_per_million,
                row.output_per_million,
                row.cache_read_per_million
            ),
            (1.32, 3.96, Some(0.044))
        );
        assert!(
            !p.rules
                .iter()
                .any(|r| r.provider == "openai" && r.model_pattern == "gpt-oss-120b")
        );
    }
    #[test]
    fn dated_tariffs_emit_and_invalid_axes_refuse() {
        let out = generate(DATA).expect("tariffs");
        syn::parse_file(&out).expect("Rust");
        assert!(out.contains("input: 1_320"));
        assert!(out.contains("deepseek-v4-pro"));
        let text = std::str::from_utf8(DATA).expect("utf8");
        for bad in [
            text.replace("1320", "0"),
            text.replace("1320", "-1"),
            text.replace("1320", "nan"),
            text.replace("1320", "inf"),
            text.replace("input_nano_per_token", "unpriced_axis"),
            text.replace("1048576", "0"),
            text.replace("https://", "http://"),
        ] {
            assert!(generate(bad.as_bytes()).is_err());
        }
    }
}
