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
    model: String,
    endpoints: Vec<String>,
    source: String,
    limits_source: String,
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
    if file.schema != "nika/inference-admission@1.0" {
        return Err(CodegenError::schema_validation(
            "admission",
            "unknown schema",
        ));
    }
    for r in file.tariffs {
        if r.provider.is_empty()
            || r.model.is_empty()
            || r.endpoints.is_empty()
            || r.endpoints
                .iter()
                .any(|e| !e.starts_with("https://") || e.contains(['?', '#']))
            || !r.source.starts_with("https://")
            || !r.limits_source.starts_with("https://")
            || r.as_of.len() != 10
            || [&r.source_sha256, &r.limits_sha256]
                .iter()
                .any(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
            || r.context_tokens == 0
            || r.max_output_tokens == 0
            || r.input_nano_per_token == 0
            || r.output_nano_per_token == 0
            || r.cached_nano_per_token == 0
            || r.cached_nano_per_token > r.input_nano_per_token
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
                "InferenceTariff {{ provider: {:?}, model: {:?}, endpoints: &{:?}, ",
                "source: {:?}, limits_source: {:?}, as_of: {:?}, source_sha256: {:?}, ",
                "limits_sha256: {:?}, context_tokens: {}, max_output_tokens: {}, ",
                "input: {}, output: {}, cached: {} }},"
            ),
            r.provider,
            r.model,
            r.endpoints,
            r.source,
            r.limits_source,
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
    fn dated_tariffs_emit_and_invalid_axes_refuse() {
        let out = generate(DATA).expect("tariffs");
        syn::parse_file(&out).expect("Rust");
        assert!(out.contains("input: 1_320"));
        assert!(out.contains("deepseek-v4-pro"));
        let text = std::str::from_utf8(DATA).expect("utf8");
        for bad in [
            text.replace("1320", "0"),
            text.replace("input_nano_per_token", "unpriced_axis"),
            text.replace("1048576", "0"),
            text.replace("https://", "http://"),
        ] {
            assert!(generate(bad.as_bytes()).is_err());
        }
    }
}
