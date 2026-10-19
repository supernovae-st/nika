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
    /// A curated whole-request text tariff, never inferred from omitted rates.
    #[serde(default)]
    declared_free: bool,
    /// `@1.2`: the rate of input tokens written to the provider's cache, when
    /// the provider bills them apart. Absent: a call that wrote stays unknown.
    #[serde(default)]
    cache_write_nano_per_token: Option<u64>,
    /// `@1.2`: models the provider's own documentation names as served for
    /// `model` (a dated snapshot its replies report), each the same route.
    #[serde(default)]
    served_models: Vec<String>,
    /// `@1.3`: the most tokens this route can generate whatever the request
    /// names, when no request parameter bounds its reasoning (xAI): the
    /// reservation covers it at the output rate in place of the requested
    /// output. At least the output cap.
    #[serde(default)]
    generation_tokens: Option<u64>,
}

const SCHEMA: &str = "nika/inference-admission@1.3";
const MAX_RATE: u64 = 9_007_199_254_740_991;
pub(crate) fn generate(raw: &[u8]) -> Result<String, CodegenError> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| CodegenError::schema_validation("admission", e.to_string()))?;
    let file: File = toml::from_str(text).map_err(|source| CodegenError::TomlParse {
        path: "inference-admission.toml".into(),
        source,
    })?;
    let mut out = String::from("static TARIFFS: &[InferenceTariff] = &[\n");
    if file.schema != SCHEMA {
        return Err(CodegenError::schema_validation(
            "admission",
            "unknown schema",
        ));
    }
    if !file.tariffs.iter().all(qualified) || !unambiguous(&file.tariffs) {
        return Err(CodegenError::schema_validation(
            "admission",
            "invalid or duplicate qualified tariff",
        ));
    }
    for r in file.tariffs {
        let cache_write = r.cache_write_nano_per_token.map_or_else(
            || "None".to_owned(),
            |n| format!("Some({})", integer_literal(n)),
        );
        let generation = r.generation_tokens.map_or_else(
            || "None".to_owned(),
            |n| format!("Some({})", integer_literal(n)),
        );
        writeln!(
            out,
            concat!(
                "InferenceTariff {{ provider: {:?}, billing_provider: {:?}, currency: {:?}, output_token_param: {:?}, model: {:?}, endpoints: &{:?}, ",
                "source: {:?}, limits_source: {:?}, route_source: {:?}, as_of: {:?}, source_sha256: {:?}, ",
                "limits_sha256: {:?}, context_tokens: {}, max_output_tokens: {}, served_models: &{:?}, ",
                "input: {}, output: {}, cached: {}, cache_write: {}, generation: {} }},"
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
            r.served_models,
            integer_literal(r.input_nano_per_token),
            integer_literal(r.output_nano_per_token),
            integer_literal(r.cached_nano_per_token),
            cache_write,
            generation
        )
        .map_err(|error| CodegenError::schema_validation("admission", error.to_string()))?;
    }
    out.push_str("];\n");
    Ok(out)
}

/// One row's own laws: sourced, dated and hashed when USD, bounded, and priced
/// on every axis or declared free on every axis.
fn qualified(r: &Tariff) -> bool {
    !(r.provider.is_empty()
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
        || [&r.source_sha256, &r.limits_sha256]
            .iter()
            .any(|s| !s.is_empty() && (s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit())))
        || (r.currency == "USD" && (r.source_sha256.is_empty() || r.limits_sha256.is_empty()))
        || r.context_tokens == 0
        || r.max_output_tokens == 0
        || !valid_rates(r)
        || r.cached_nano_per_token > r.input_nano_per_token
        || [
            r.input_nano_per_token,
            r.output_nano_per_token,
            r.cached_nano_per_token,
            r.cache_write_nano_per_token.unwrap_or(0),
        ]
        .iter()
        .any(|n| *n > MAX_RATE)
        || r.served_models.iter().any(|m| {
            m.is_empty() || *m == r.model || m.chars().any(|c| c.is_whitespace() || c.is_control())
        })
        || r.generation_tokens
            .is_some_and(|n| n < u64::from(r.max_output_tokens) || n > MAX_RATE))
}

fn valid_rates(tariff: &Tariff) -> bool {
    let rates = [
        tariff.input_nano_per_token,
        tariff.output_nano_per_token,
        tariff.cached_nano_per_token,
    ];
    if tariff.declared_free {
        // A free declaration names no write rate: it is free on every axis it names.
        rates.iter().all(|rate| *rate == 0) && tariff.cache_write_nano_per_token.is_none()
    } else {
        rates.iter().all(|rate| *rate > 0) && tariff.cache_write_nano_per_token != Some(0)
    }
}

/// Every model a provider's rows name, exact or served, belongs to one row: a
/// reply then identifies exactly one route.
fn unambiguous(tariffs: &[Tariff]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    tariffs.iter().all(|r| {
        std::iter::once(&r.model)
            .chain(&r.served_models)
            .all(|m| seen.insert((r.provider.as_str(), m.as_str())))
    })
}

/// Project exact first-party USD facts from their owning admission source.
/// The models.dev file and its historical identity remain untouched. Route
/// qualification is still required at execution; a catalog row is not a grant.
///
/// A snapshot row names its provider by the catalog id or one of its aliases
/// (models.dev ships Gemini under `google`). A tariffed model the snapshot
/// lacks (newer than it, as Gemini 4 Argon) gains a row from its tariff, first
/// in the table so the contains-fallback never shadows it: every priced-model
/// gate then reads the one price the run settles at.
pub(crate) fn project_pricing(
    raw: &[u8],
    pricing: &mut crate::pricing::PricingFile,
    providers: &[crate::providers::ProviderEntry],
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
        let names = provider_names(&t.provider, providers);
        // Qualified rates are integers below 2^53; validation above owns bounds.
        #[allow(clippy::cast_precision_loss)]
        let per_million = |nano: u64| nano as f64 / 1000.0;
        if let Some(row) = pricing
            .rules
            .iter_mut()
            .find(|r| names.contains(&r.provider.as_str()) && r.model_pattern == t.model)
        {
            row.input_per_million = per_million(t.input_nano_per_token);
            row.output_per_million = per_million(t.output_nano_per_token);
            row.cache_read_per_million = Some(per_million(t.cached_nano_per_token));
            // No write rate: a write is input to the snapshot math.
            row.cache_write_per_million = t.cache_write_nano_per_token.map(per_million);
            row.reasoning_tokens_per_million = None;
        } else {
            pricing.rules.insert(
                0,
                crate::pricing::PricingEntry {
                    provider: t.provider.clone(),
                    model_pattern: t.model.clone(),
                    input_per_million: per_million(t.input_nano_per_token),
                    output_per_million: per_million(t.output_nano_per_token),
                    cache_write_per_million: t.cache_write_nano_per_token.map(per_million),
                    cache_read_per_million: Some(per_million(t.cached_nano_per_token)),
                    image_per_million: None,
                    reasoning_tokens_per_million: None,
                    max_output_tokens: Some(t.max_output_tokens),
                    context_window_tokens: None,
                    open_weights: None,
                    status: None,
                    energy: None,
                    tokenizer: None,
                    determinism: None,
                },
            );
        }
    }
    Ok(())
}

/// The names a snapshot row may give `provider`: its catalog id and aliases.
fn provider_names<'a>(
    provider: &'a str,
    providers: &'a [crate::providers::ProviderEntry],
) -> Vec<&'a str> {
    let mut names = vec![provider];
    if let Some(entry) = providers.iter().find(|p| p.id == provider) {
        names.extend(entry.aliases.iter().map(String::as_str));
    }
    names
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
        assert!((before - 0.66).abs() < f64::EPSILON);
        let providers = crate::providers::parse_llm_providers_bytes(
            include_bytes!("../../nika-catalog/data/llm-providers.toml"),
            std::path::Path::new("providers"),
        )
        .expect("providers");
        project_pricing(DATA, &mut p, &providers).expect("projection");
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
        // models.dev ships Gemini under `google`: the tariff projects through the alias.
        let pro = p
            .rules
            .iter()
            .find(|r| r.provider == "google" && r.model_pattern == "gemini-3.1-pro-preview")
            .expect("snapshot row");
        assert_eq!((pro.input_per_million, pro.output_per_million), (2.0, 12.0));
        // A tariffed model newer than the snapshot gains its row, first in the table.
        let argon = p
            .rules
            .iter()
            .position(|r| r.model_pattern == "gemini-4-argon")
            .expect("a row from the tariff");
        let row = &p.rules[argon];
        assert_eq!(
            (
                row.provider.as_str(),
                row.input_per_million,
                row.output_per_million
            ),
            ("gemini", 4.0, 20.0)
        );
        assert!(argon < p.rules.len() / 2, "ahead of the snapshot rows");
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

    #[test]
    fn free_tariffs_require_an_explicit_complete_zero_declaration() {
        let text = std::str::from_utf8(DATA).expect("utf8");
        let out = generate(DATA).expect("curated tariffs");
        assert!(out.contains("nvidia/nemotron-3-super-120b-a12b:free"));
        assert!(out.contains("input: 0, output: 0, cached: 0"));
        for bad in [
            text.replace("declared_free = true", "declared_free = false"),
            text.replace("declared_free = true\n", ""),
            text.replace("input_nano_per_token = 0\n", "input_nano_per_token = 1\n"),
            text.replace("output_nano_per_token = 0\n", "output_nano_per_token = 1\n"),
            text.replace("cached_nano_per_token = 0\n", "cached_nano_per_token = 1\n"),
            text.replace("cached_nano_per_token = 0\n", ""),
        ] {
            assert!(generate(bad.as_bytes()).is_err(), "{bad}");
        }
        let free = text.rsplit("[[tariffs]]").next().expect("last tariff");
        assert!(generate(format!("{text}\n[[tariffs]]{free}").as_bytes()).is_err());
        let written = text.replace(
            "declared_free = true\n",
            "declared_free = true\ncache_write_nano_per_token = 0\n",
        );
        assert!(
            generate(written.as_bytes()).is_err(),
            "a free row names no write rate"
        );
    }

    /// `@1.2`: a row may name the dated models its replies report and a
    /// cache-write rate; a served model names one route only, never its own
    /// model or another row's, and a paid write rate is positive.
    #[test]
    fn served_models_and_a_cache_write_rate_emit_and_stay_unambiguous() {
        let text = std::str::from_utf8(DATA).expect("utf8");
        let pro = "cached_nano_per_token = 44\n";
        assert_eq!(text.matches(pro).count(), 1, "one anchor row");
        let row = |extra: &str| text.replace(pro, &format!("{pro}{extra}"));
        let out = generate(
            row("served_models = [\"deepseek-v4-pro-0813\"]\ncache_write_nano_per_token = 1650\n")
                .as_bytes(),
        )
        .expect("@1.2 row");
        syn::parse_file(&out).expect("Rust");
        assert!(
            out.contains("served_models: &[\"deepseek-v4-pro-0813\"]"),
            "the row's served model is emitted"
        );
        assert!(
            out.contains("cache_write: Some(1_650)"),
            "its cache-write rate is emitted"
        );
        assert!(
            out.contains("served_models: &[], input: 300"),
            "the other rows serve no model"
        );
        for bad in [
            row("served_models = [\"deepseek-v4-pro\"]\n"),
            row("served_models = [\"deepseek-flash\"]\n"),
            row("served_models = [\"\"]\n"),
            row("served_models = [\"deepseek v4\"]\n"),
            row("served_models = [\"a\", \"a\"]\n"),
            row("cache_write_nano_per_token = 0\n"),
            row("cache_write_nano_per_token = 9007199254740992\n"),
            row("generation_tokens = 9\n"),
            text.replace("inference-admission@1.3", "inference-admission@1.2"),
        ] {
            assert!(generate(bad.as_bytes()).is_err(), "{bad}");
        }
    }
}
