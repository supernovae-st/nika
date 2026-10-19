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
    /// Sourced data-handling policy (schema `@1.1`). Absent = no sourced
    /// policy vendored — never an implicit "does not train".
    #[serde(default)]
    pub data_policy: Option<DataPolicyEntry>,
}

/// One `[[providers.models]]` row. `deny_unknown_fields`: a typo'd fact
/// key fails the build instead of silently vanishing.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ProviderModelEntry {
    pub id: String,
    pub model: String,
    pub context_window_tokens: u32,
    pub max_output_tokens: u32,
    /// Where the two token limits were read (schema `@1.2`); arrives with
    /// `as_of`. Absent = curated limits with no recorded source.
    #[serde(default)]
    pub limits_source: Option<String>,
    /// The date `limits_source` was read (`YYYY-MM-DD`).
    #[serde(default)]
    pub as_of: Option<String>,
    /// The retirement floor the provider publishes for this model.
    #[serde(default)]
    pub lifecycle: Option<LifecycleEntry>,
}

/// A published retirement floor (schema `@1.2`): the provider retires the
/// model not before `retires_not_before` and names `successor`, another
/// wire of the same provider. A floor, never a retirement date.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct LifecycleEntry {
    pub retires_not_before: String,
    pub successor: String,
    pub source: String,
    pub as_of: String,
}

/// One sourced data-handling policy (schema `@1.1`) — the sovereignty
/// facts. Every field REQUIRED once the table is present: a policy claim
/// without its source is exactly the class of confident guess this
/// catalog refuses, and `deny_unknown_fields` makes a typo'd key fail
/// the build instead of silently vanishing.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct DataPolicyEntry {
    /// Training class — closed set: `no` / `yes` / `opt-out` / `split` /
    /// `passthrough`. A bool cannot say "trains unless you act" (Mistral),
    /// "paid no · free yes" (Google) or "depends on routing" (aggregators).
    pub trains: String,
    /// Default retention as the policy states it (`none` · `30` ·
    /// `unspecified`). Free text, never empty — the wording varies too
    /// much across providers to close the set yet.
    pub retention: String,
    /// Zero-data-retention availability — closed set: `yes` /
    /// `enterprise-only` / `no` / `local`.
    pub zdr: String,
    /// Citation for the claims (policy URL and/or date). Never empty.
    pub source: String,
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

    if let Some(dp) = p.data_policy.as_ref() {
        validate_data_policy(dp, &p.id).map_err(|r| CodegenError::schema_validation(ctx, r))?;
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
    crate::model_facts::validate(p, ctx)?;

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

/// Nicknames are unique per provider. Two nicknames may name one wire only
/// as aliases of the same model: with identical token limits.
fn validate_provider_models(p: &ProviderEntry, ctx: &str) -> Result<HashSet<String>, CodegenError> {
    let mut seen_nicks: HashSet<String> = HashSet::new();
    let mut seen_wires: HashSet<String> = HashSet::new();
    for (i, m) in p.models.iter().enumerate() {
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
        if !seen_wires.insert(m.model.clone())
            && p.models[..i].iter().any(|other| {
                other.model == m.model
                    && (other.context_window_tokens, other.max_output_tokens)
                        != (m.context_window_tokens, m.max_output_tokens)
            })
        {
            return Err(CodegenError::schema_validation(
                ctx,
                format!(
                    "provider {:?}: model wire {:?} named twice with different token limits",
                    p.id, m.model
                ),
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

/// A vendored policy must arrive whole. `trains` and `zdr` are OUR
/// classification vocabularies (closed sets — growing them is a schema
/// decision, so validation cannot break a data refresh); `retention`
/// stays free text; `source` is the non-negotiable citation.
pub(crate) fn validate_data_policy(dp: &DataPolicyEntry, provider_id: &str) -> Result<(), String> {
    const TRAINS: &[&str] = &["no", "yes", "opt-out", "split", "passthrough"];
    const ZDR: &[&str] = &["yes", "enterprise-only", "no", "local"];
    if !TRAINS.contains(&dp.trains.as_str()) {
        return Err(format!(
            "provider {provider_id:?}: data_policy.trains must be one of \
             no / yes / opt-out / split / passthrough (got {:?})",
            dp.trains
        ));
    }
    if !ZDR.contains(&dp.zdr.as_str()) {
        return Err(format!(
            "provider {provider_id:?}: data_policy.zdr must be one of \
             yes / enterprise-only / no / local (got {:?})",
            dp.zdr
        ));
    }
    if dp.retention.trim().is_empty() {
        return Err(format!(
            "provider {provider_id:?}: data_policy.retention must not be empty \
             — write \"unspecified\" when the policy does not say"
        ));
    }
    if dp.source.trim().is_empty() {
        return Err(format!(
            "provider {provider_id:?}: data_policy.source must not be empty — \
             a policy claim without a citation is not a fact; omit the table instead"
        ));
    }
    Ok(())
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
    crate::model_facts::emit(&mut out, providers);

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
    let _ = writeln!(
        out,
        "        data_policy: {},",
        data_policy_expr(p.data_policy.as_ref())
    );
    let _ = writeln!(out, "    }},");
}

/// `Option<DataPolicyEntry>` as Rust source. `None` stays `None` — a
/// provider without a vendored policy must never reach the binary as an
/// implicit "does not train".
fn data_policy_expr(dp: Option<&DataPolicyEntry>) -> String {
    match dp {
        Some(dp) => format!(
            "Some(crate::types::DataPolicy {{ trains: {}, retention: {}, \
             zdr: {}, source: {} }})",
            rstr(&dp.trains),
            rstr(&dp.retention),
            rstr(&dp.zdr),
            rstr(&dp.source),
        ),
        None => "None".to_string(),
    }
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

    fn row(id: &str, model: &str, context: u32, output: u32) -> ProviderModelEntry {
        ProviderModelEntry {
            id: id.to_string(),
            model: model.to_string(),
            context_window_tokens: context,
            max_output_tokens: output,
            limits_source: None,
            as_of: None,
            lifecycle: None,
        }
    }

    fn sourced(mut m: ProviderModelEntry) -> ProviderModelEntry {
        m.limits_source = Some("https://docs.example/models".to_string());
        m.as_of = Some("2026-10-12".to_string());
        m
    }

    fn retiring(mut m: ProviderModelEntry, successor: &str) -> ProviderModelEntry {
        m.lifecycle = Some(LifecycleEntry {
            retires_not_before: "2026-10-15".to_string(),
            successor: successor.to_string(),
            source: "https://docs.example/deprecations".to_string(),
            as_of: "2026-10-12".to_string(),
        });
        m
    }

    /// Two nicknames may alias one wire with identical limits; different
    /// limits on one wire are a contradiction the build refuses.
    #[test]
    fn an_alias_nickname_shares_its_wire_only_with_identical_limits() {
        let mut p = fixture_provider();
        p.models.push(row("alias", "demo-large", 200_000, 8192));
        assert!(validate_providers(&[p], Path::new("/x")).is_ok());
        let mut p = fixture_provider();
        p.models.push(row("alias", "demo-large", 100_000, 8192));
        let msg = format!("{}", validate_providers(&[p], Path::new("/x")).unwrap_err());
        assert!(msg.contains("different token limits"), "got: {msg}");
    }

    /// `@1.2` facts arrive whole and sourced, a successor is another wire of
    /// the provider, and two rows of one wire never disagree.
    #[test]
    fn model_facts_arrive_whole_sourced_and_consistent() {
        let ok = |models: Vec<ProviderModelEntry>| {
            let mut p = fixture_provider();
            p.models = models;
            validate_providers(&[p], Path::new("/x"))
        };
        let base = || row("large", "demo-large", 200_000, 8192);
        let next = || row("next", "demo-next", 1_000_000, 128_000);
        assert!(ok(vec![sourced(base()), next()]).is_ok());
        assert!(ok(vec![retiring(sourced(base()), "demo-next"), next()]).is_ok());
        let mut half = base();
        half.limits_source = Some("https://docs.example/models".to_string());
        assert!(ok(vec![half, next()]).is_err(), "source without date");
        let mut plain = sourced(base());
        plain.limits_source = Some("http://docs.example/models".to_string());
        assert!(ok(vec![plain, next()]).is_err(), "not https");
        let mut undated = sourced(base());
        undated.as_of = Some("12 Oct 2026".to_string());
        assert!(ok(vec![undated, next()]).is_err(), "not YYYY-MM-DD");
        assert!(
            ok(vec![retiring(base(), "demo-absent"), next()]).is_err(),
            "unknown successor"
        );
        assert!(
            ok(vec![retiring(base(), "demo-large"), next()]).is_err(),
            "self successor"
        );
        let mut other = sourced(row("alias", "demo-large", 200_000, 8192));
        other.as_of = Some("2026-10-13".to_string());
        assert!(
            ok(vec![sourced(base()), other, next()]).is_err(),
            "conflicting facts on one wire"
        );
        assert!(
            ok(vec![
                base(),
                sourced(row("alias", "demo-large", 200_000, 8192))
            ])
            .is_ok(),
            "one silent row beside a sourced one"
        );
    }

    /// The facts table names each sourced wire once, in catalogue order; a
    /// catalogue without facts emits an empty table.
    #[test]
    fn model_facts_emit_one_entry_per_sourced_wire() {
        let mut p = fixture_provider();
        p.models = vec![
            retiring(
                sourced(row("large", "demo-large", 200_000, 8192)),
                "demo-next",
            ),
            sourced(row("alias", "demo-large", 200_000, 8192)),
            row("next", "demo-next", 1_000_000, 128_000),
        ];
        let out = generate_providers_rs(&[p]);
        assert_eq!(
            out.matches("crate::types::ModelFacts {").count(),
            1,
            "{out}"
        );
        assert!(out.contains("limits_as_of: Some(\"2026-10-12\")"), "{out}");
        assert!(out.contains("successor: \"demo-next\""), "{out}");
        let empty = generate_providers_rs(&[fixture_provider()]);
        assert!(
            empty.ends_with("pub static MODEL_FACTS: &[crate::types::ModelFacts] = &[\n];\n"),
            "{empty}"
        );
    }

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
            models: vec![row("large", "demo-large", 200_000, 8192)],
            api_dialect: Some("openai-chat".to_string()),
            tags: vec!["fast".to_string()],
            extra_tags: vec![],
            data_policy: None,
        }
    }

    fn fixture_policy() -> DataPolicyEntry {
        DataPolicyEntry {
            trains: "no".to_string(),
            retention: "none".to_string(),
            zdr: "local".to_string(),
            source: "local runtime · no egress by construction".to_string(),
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
        p.models.push(row("large", "demo-other", 100_000, 4096)); // duplicate nickname
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

    // ─── Mutation-kill arc 2026-06-11 (vector 39 · 47-survivor sweep) ────────

    #[test]
    fn assert_ascii_key_rejects_non_ascii_and_empty() {
        assert!(assert_ascii_key("id", "ok-id").is_ok());
        assert!(assert_ascii_key("id", "café").is_err());
        assert!(assert_ascii_key("id", "").is_err());
    }

    // ── @1.1 data_policy ───────────────────────────────────────────

    #[test]
    fn generate_emits_data_policy_some_and_none() {
        let mut p = fixture_provider();
        p.data_policy = Some(fixture_policy());
        let s = generate_providers_rs(&[p]);
        assert!(
            s.contains(
                "data_policy: Some(crate::types::DataPolicy { trains: \"no\", \
                 retention: \"none\", zdr: \"local\", \
                 source: \"local runtime · no egress by construction\" })"
            ),
            "got: {s}"
        );
        // The None path: an unvendored provider must never read as a policy.
        let s = generate_providers_rs(&[fixture_provider()]);
        assert!(s.contains("data_policy: None"), "got: {s}");
    }

    #[test]
    fn validate_data_policy_trains_closed_set() {
        for good in ["no", "yes", "opt-out", "split", "passthrough"] {
            let mut dp = fixture_policy();
            dp.trains = good.to_string();
            validate_data_policy(&dp, "p").unwrap_or_else(|err| panic!("{good:?}: {err}"));
        }
        // A bool spelling is the load-bearing rejection: three of the five
        // real-world classes cannot be said in true/false.
        for bad in ["true", "false", "sometimes", ""] {
            let mut dp = fixture_policy();
            dp.trains = bad.to_string();
            let err = validate_data_policy(&dp, "p").unwrap_err();
            assert!(err.contains("data_policy.trains"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn validate_data_policy_zdr_closed_set() {
        for good in ["yes", "enterprise-only", "no", "local"] {
            let mut dp = fixture_policy();
            dp.zdr = good.to_string();
            validate_data_policy(&dp, "p").unwrap_or_else(|err| panic!("{good:?}: {err}"));
        }
        let mut dp = fixture_policy();
        dp.zdr = "maybe".to_string();
        let err = validate_data_policy(&dp, "p").unwrap_err();
        assert!(err.contains("data_policy.zdr"), "{err}");
    }

    #[test]
    fn validate_data_policy_requires_retention_and_source() {
        let mut dp = fixture_policy();
        dp.retention = " ".to_string();
        let err = validate_data_policy(&dp, "p").unwrap_err();
        assert!(err.contains("retention"), "{err}");

        let mut dp = fixture_policy();
        dp.source = String::new();
        let err = validate_data_policy(&dp, "p").unwrap_err();
        assert!(err.contains("source"), "{err}");
    }

    #[test]
    fn validate_providers_accepts_vendored_policy() {
        let mut p = fixture_provider();
        p.data_policy = Some(fixture_policy());
        validate_providers(&[p], Path::new("/x")).expect("sourced policy must validate");
    }
}
