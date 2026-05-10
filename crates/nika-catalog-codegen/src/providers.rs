// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LLM providers catalog codegen — TOML schema, validation, Rust emission.
//!
//! Lifted from `nika-catalog/build.rs`. `ProviderEntry` is `pub` so the
//! capabilities + embeddings codegen modules can pass it as the FK
//! validation input.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{opt_rstr, rstr, str_slice_expr, write_str_slice};
use crate::error::CodegenError;
use crate::schema::{LLM_PROVIDERS_SCHEMA, assert_schema};
use crate::tags::{tags_slice_expr, validate_tags};

// ─── TOML schema types ───────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct LlmProvidersFile {
    pub schema: String,
    pub providers: Vec<ProviderEntry>,
}

/// A single LLM provider entry. Public so capabilities + embeddings
/// codegen can pass it as the FK validation input.
#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub env_var: String,
    #[serde(default)]
    pub key_prefixes: Vec<String>,
    pub default_model: String,
    pub cheap_model: String,
    pub requires_key: bool,
    pub description: String,
    #[serde(default)]
    pub models: Vec<ProviderModelEntry>,
    /// Wire-protocol family — closed set: `anthropic` / `openai-chat` /
    /// `openai-responses` / `gemini` / `cohere` / `ai21` / `bedrock` /
    /// `voyage` / `mock`. Validated at parse time.
    #[serde(default)]
    pub api_dialect: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub extra_tags: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[non_exhaustive]
pub struct ProviderModelEntry {
    pub id: String,
    pub model: String,
    pub context_window_tokens: u32,
    pub max_output_tokens: u32,
}

// ─── Public codegen entry ────────────────────────────────────────────────

/// Parse + validate `llm-providers.toml` bytes.
pub fn parse_llm_providers_bytes(
    raw: &[u8],
    path: &Path,
) -> Result<Vec<ProviderEntry>, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let file: LlmProvidersFile =
        toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
            path: path.to_path_buf(),
            source,
        })?;

    assert_schema(LLM_PROVIDERS_SCHEMA, &file.schema, path)?;

    validate_providers(&file.providers, path)?;
    Ok(file.providers)
}

/// Codegen entry — parse TOML bytes + emit Rust source.
pub fn codegen_providers(toml_bytes: &[u8]) -> Result<String, CodegenError> {
    let synthetic = Path::new("<bytes>:llm-providers.toml");
    let providers = parse_llm_providers_bytes(toml_bytes, synthetic)?;
    Ok(generate_providers_rs(&providers))
}

// ─── Validation ──────────────────────────────────────────────────────────

fn validate_providers(providers: &[ProviderEntry], path: &Path) -> Result<(), CodegenError> {
    let ctx = path.display().to_string();
    let mut seen_keys: HashSet<String> = HashSet::new();
    for p in providers {
        validate_one_provider(p, &ctx, &mut seen_keys)?;
    }
    Ok(())
}

fn validate_one_provider(
    p: &ProviderEntry,
    ctx: &str,
    seen_keys: &mut HashSet<String>,
) -> Result<(), CodegenError> {
    assert_ascii_key("provider id", &p.id).map_err(|r| CodegenError::schema_validation(ctx, r))?;
    if !seen_keys.insert(p.id.to_ascii_lowercase()) {
        return Err(CodegenError::schema_validation(
            ctx,
            format!("duplicate provider id (case-insensitive): {:?}", p.id),
        ));
    }
    for a in &p.aliases {
        assert_ascii_key("provider alias", a)
            .map_err(|r| CodegenError::schema_validation(ctx, r))?;
        if !seen_keys.insert(a.to_ascii_lowercase()) {
            return Err(CodegenError::schema_validation(
                ctx,
                format!(
                    "provider {:?}: alias {a:?} collides with an existing id or alias (case-insensitive)",
                    p.id
                ),
            ));
        }
    }
    validate_tags(&p.tags, &p.id).map_err(|r| CodegenError::schema_validation(ctx, r))?;

    if p.requires_key && p.env_var.is_empty() {
        return Err(CodegenError::schema_validation(
            ctx,
            format!(
                "provider {:?}: requires_key=true but env_var is empty",
                p.id
            ),
        ));
    }

    if let Some(dialect) = p.api_dialect.as_deref() {
        validate_api_dialect(dialect, &p.id)
            .map_err(|r| CodegenError::schema_validation(ctx, r))?;
    }

    if p.models.is_empty() {
        return Err(CodegenError::schema_validation(
            ctx,
            format!(
                "provider {:?}: models list is empty (at least one model required)",
                p.id
            ),
        ));
    }

    let seen_wires = validate_provider_models(p, ctx)?;

    if !seen_wires.contains(&p.default_model) {
        return Err(CodegenError::schema_validation(
            ctx,
            format!(
                "provider {:?}: default_model {:?} not in models list",
                p.id, p.default_model
            ),
        ));
    }
    if !seen_wires.contains(&p.cheap_model) {
        return Err(CodegenError::schema_validation(
            ctx,
            format!(
                "provider {:?}: cheap_model {:?} not in models list",
                p.id, p.cheap_model
            ),
        ));
    }
    Ok(())
}

fn validate_provider_models(p: &ProviderEntry, ctx: &str) -> Result<HashSet<String>, CodegenError> {
    let mut seen_nicks: HashSet<String> = HashSet::new();
    let mut seen_wires: HashSet<String> = HashSet::new();
    for m in &p.models {
        if !seen_nicks.insert(m.id.clone()) {
            return Err(CodegenError::schema_validation(
                ctx,
                format!("provider {:?}: duplicate model nickname {:?}", p.id, m.id),
            ));
        }
        if m.model.is_empty() {
            return Err(CodegenError::schema_validation(
                ctx,
                format!(
                    "provider {:?}: model {:?} has empty wire identifier",
                    p.id, m.id
                ),
            ));
        }
        if !seen_wires.insert(m.model.clone()) {
            return Err(CodegenError::schema_validation(
                ctx,
                format!("provider {:?}: duplicate model wire {:?}", p.id, m.model),
            ));
        }
        if m.context_window_tokens == 0 || m.max_output_tokens == 0 {
            return Err(CodegenError::schema_validation(
                ctx,
                format!("provider {:?}: model {:?} has zero token limit", p.id, m.id),
            ));
        }
    }
    Ok(seen_wires)
}

/// Closed set of wire-protocol dialects. Synced with `Provider::api_dialect`
/// docs in `nika-catalog`.
pub(crate) fn validate_api_dialect(dialect: &str, provider_id: &str) -> Result<(), String> {
    match dialect {
        "anthropic" | "openai-chat" | "openai-responses" | "gemini" | "cohere" | "ai21"
        | "bedrock" | "voyage" | "mock" => Ok(()),
        _ => Err(format!(
            "provider {provider_id:?}: unknown api_dialect {dialect:?} — \
             must be one of anthropic / openai-chat / openai-responses / gemini / \
             cohere / ai21 / bedrock / voyage / mock"
        )),
    }
}

fn assert_ascii_key(kind: &str, s: &str) -> Result<(), String> {
    if !s.is_ascii() {
        return Err(format!(
            "{kind} {s:?} contains non-ASCII characters (ASCII-only ids/aliases required for case-insensitive phf lookup)"
        ));
    }
    if s.is_empty() {
        return Err(format!("{kind} is empty"));
    }
    Ok(())
}

// ─── Rust source emission ────────────────────────────────────────────────

#[must_use]
pub(crate) fn generate_providers_rs(providers: &[ProviderEntry]) -> String {
    let mut out = String::with_capacity(8_192);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/llm-providers.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "pub static ALL_PROVIDERS: &[crate::types::Provider] = &["
    );

    for p in providers {
        emit_provider(&mut out, p);
    }

    let _ = writeln!(out, "];");
    let _ = writeln!(out);

    let mut builder = phf_codegen::Map::<unicase::UniCase<&str>>::new();
    let entries: Vec<(String, String)> = {
        let mut v = Vec::new();
        for (i, p) in providers.iter().enumerate() {
            v.push((p.id.clone(), i.to_string()));
            for a in &p.aliases {
                v.push((a.clone(), i.to_string()));
            }
        }
        v
    };
    for (k, v) in &entries {
        builder.entry(unicase::UniCase::ascii(k.as_str()), v.as_str());
    }
    let _ = writeln!(
        out,
        "pub static PROVIDER_INDEX: phf::Map<unicase::UniCase<&'static str>, usize> = {};",
        builder.build()
    );

    out
}

fn emit_provider(out: &mut String, p: &ProviderEntry) {
    let _ = writeln!(out, "    crate::types::Provider {{");
    let _ = writeln!(out, "        id: {},", rstr(&p.id));
    let _ = writeln!(out, "        name: {},", rstr(&p.name));
    write_str_slice(out, "aliases", &p.aliases, 8);
    let _ = writeln!(out, "        env_var: {},", rstr(&p.env_var));
    let _ = writeln!(
        out,
        "        key_prefixes: {},",
        str_slice_expr(&p.key_prefixes)
    );
    let _ = writeln!(out, "        default_model: {},", rstr(&p.default_model));
    let _ = writeln!(out, "        cheap_model: {},", rstr(&p.cheap_model));
    let _ = writeln!(out, "        requires_key: {},", p.requires_key);
    let _ = writeln!(out, "        description: {},", rstr(&p.description));
    write_models(out, &p.models);
    let _ = writeln!(
        out,
        "        api_dialect: {},",
        opt_rstr(p.api_dialect.as_deref())
    );
    let _ = writeln!(out, "        tags: {},", tags_slice_expr(&p.tags));
    let _ = writeln!(
        out,
        "        extra_tags: {},",
        str_slice_expr(&p.extra_tags)
    );
    let _ = writeln!(out, "    }},");
}

fn write_models(out: &mut String, models: &[ProviderModelEntry]) {
    if models.is_empty() {
        let _ = writeln!(out, "        models: &[],");
        return;
    }
    let _ = writeln!(out, "        models: &[");
    for m in models {
        let _ = writeln!(out, "            crate::types::ProviderModel {{");
        let _ = writeln!(out, "                id: {},", rstr(&m.id));
        let _ = writeln!(out, "                model: {},", rstr(&m.model));
        let _ = writeln!(
            out,
            "                context_window_tokens: {},",
            m.context_window_tokens
        );
        let _ = writeln!(
            out,
            "                max_output_tokens: {},",
            m.max_output_tokens
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_provider() -> ProviderEntry {
        ProviderEntry {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            aliases: vec![],
            env_var: "DEMO_API_KEY".to_string(),
            key_prefixes: vec!["sk-demo-".to_string()],
            default_model: "demo-large".to_string(),
            cheap_model: "demo-large".to_string(),
            requires_key: true,
            description: "Demo provider".to_string(),
            models: vec![ProviderModelEntry {
                id: "large".to_string(),
                model: "demo-large".to_string(),
                context_window_tokens: 200_000,
                max_output_tokens: 8192,
            }],
            api_dialect: Some("openai-chat".to_string()),
            tags: vec!["fast".to_string()],
            extra_tags: vec![],
        }
    }

    #[test]
    fn validate_api_dialect_closed_set() {
        for ok in [
            "anthropic",
            "openai-chat",
            "openai-responses",
            "gemini",
            "cohere",
            "ai21",
            "bedrock",
            "voyage",
            "mock",
        ] {
            assert!(validate_api_dialect(ok, "p").is_ok());
        }
        assert!(validate_api_dialect("nope", "p").is_err());
    }

    #[test]
    fn validate_providers_empty_models_rejected() {
        let mut p = fixture_provider();
        p.models.clear();
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("models list is empty"), "got: {msg}");
    }

    #[test]
    fn validate_providers_default_model_must_exist() {
        let mut p = fixture_provider();
        p.default_model = "nonexistent".to_string();
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("default_model"), "got: {msg}");
    }

    #[test]
    fn validate_providers_cheap_model_must_exist() {
        let mut p = fixture_provider();
        p.cheap_model = "nonexistent".to_string();
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cheap_model"), "got: {msg}");
    }

    #[test]
    fn validate_providers_zero_token_limit_rejected() {
        let mut p = fixture_provider();
        p.models[0].context_window_tokens = 0;
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("zero token limit"), "got: {msg}");
    }

    #[test]
    fn validate_providers_duplicate_model_nickname_rejected() {
        let mut p = fixture_provider();
        p.models.push(ProviderModelEntry {
            id: "large".to_string(), // duplicate
            model: "demo-other".to_string(),
            context_window_tokens: 100_000,
            max_output_tokens: 4096,
        });
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate model nickname"), "got: {msg}");
    }

    #[test]
    fn validate_providers_duplicate_id_case_insensitive() {
        let p1 = fixture_provider();
        let mut p2 = fixture_provider();
        p2.id = "DEMO".to_string();
        let err = validate_providers(&[p1, p2], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("duplicate provider id"), "got: {msg}");
    }

    #[test]
    fn validate_providers_alias_collides_with_id() {
        let mut p1 = fixture_provider();
        p1.aliases = vec!["demo2".to_string()];
        let mut p2 = fixture_provider();
        p2.id = "demo2".to_string();
        let err = validate_providers(&[p1, p2], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        // alias inserted first, then id collides
        assert!(
            msg.contains("duplicate provider id") || msg.contains("collides"),
            "got: {msg}"
        );
    }

    #[test]
    fn validate_providers_requires_key_needs_env_var() {
        let mut p = fixture_provider();
        p.env_var.clear();
        let err = validate_providers(&[p], Path::new("/x")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("env_var is empty"), "got: {msg}");
    }

    #[test]
    fn generate_emits_canonical_static_names() {
        let s = generate_providers_rs(&[fixture_provider()]);
        assert!(s.contains("pub static ALL_PROVIDERS"));
        assert!(s.contains("pub static PROVIDER_INDEX"));
        assert!(s.contains("crate::types::Provider"));
        assert!(s.contains("crate::types::ProviderModel"));
    }

    #[test]
    fn idempotent_emission_providers() {
        let p = fixture_provider();
        let a = generate_providers_rs(&[fixture_provider()]);
        let b = generate_providers_rs(&[p]);
        assert_eq!(a, b);
    }
}
