// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Build-time codegen for `data/model-capabilities.toml`.
//!
//! Split out of `build.rs` during the Session 2a hardening pass to keep
//! build.rs under the 1500-LOC-per-file budget. This module owns the
//! capabilities TOML schema, its validation, and the `CapPatch` / `Rule`
//! Rust emission — everything scoped to the `capabilities` feature.
//!
//! Loaded from the parent build script via
//! `#[path = "build/capabilities.rs"] mod capabilities;`.
//!
//! The clippy allows on the parent build.rs (unwrap on `writeln!(String)`,
//! `format_push_string`, print_* for cargo directives) transitively cover
//! this module — it inherits the parent's crate-level attributes.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{
    assert_schema, opt_rstr, rstr, str_slice_expr, validate_api_dialect, CAPABILITIES_SCHEMA,
    ProviderEntry,
};

// ─── Capabilities TOML schema ────────────────────────────────────────────

// All capabilities TOML structs use `deny_unknown_fields` so a typo like
// `prefex_any` on a `match.kind` produces a clear parse error at build time
// rather than silently deserialising into the default variant.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesFile {
    schema: String,
    defaults: CapsPatchEntry,
    #[serde(default)]
    rules: Vec<RuleEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleEntry {
    name: String,
    #[serde(default)]
    scope: ScopeEntry,
    #[serde(rename = "match")]
    match_: MatchEntry,
    caps: CapsPatchEntry,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ScopeEntry {
    #[serde(default)]
    providers: Vec<String>,
    #[serde(default)]
    api_dialect: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MatchEntry {
    Any,
    Exact { model: String },
    ExactAny { models: Vec<String> },
    PrefixAny { prefixes: Vec<String> },
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CapsPatchEntry {
    #[serde(default)]
    token_limit_param: Option<String>,
    #[serde(default)]
    supports_temperature: Option<bool>,
    #[serde(default)]
    supports_stop_sequences: Option<bool>,
    #[serde(default)]
    reasoning: Option<bool>,
    #[serde(default)]
    supports_vision: Option<bool>,
}

// ─── Parse + validate ────────────────────────────────────────────────────

/// Parsed + validated capabilities file. `providers` is borrowed for FK
/// checks only; not retained.
pub(super) struct ParsedCapabilities {
    defaults: CapsPatchEntry,
    rules: Vec<RuleEntry>,
}

pub(super) fn parse_capabilities(
    path: &Path,
    providers: &[ProviderEntry],
) -> Result<ParsedCapabilities, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let file: CapabilitiesFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    assert_schema(CAPABILITIES_SCHEMA, &file.schema, path)?;

    validate_caps_patch(&file.defaults, "[defaults]")?;

    // Set of canonical provider ids (lowercased) for FK checks — rules
    // reference providers by canonical id only, not by alias.
    let known_provider_ids: HashSet<String> =
        providers.iter().map(|p| p.id.to_ascii_lowercase()).collect();

    // Set of declared api_dialects (lowercased) — rules can scope by dialect
    // only if at least one provider uses that dialect. Catches stale rule
    // scopes when a provider's `api_dialect` is removed.
    let declared_dialects: HashSet<String> = providers
        .iter()
        .filter_map(|p| p.api_dialect.as_deref())
        .map(str::to_ascii_lowercase)
        .collect();

    let mut seen_names: HashSet<String> = HashSet::new();
    for rule in &file.rules {
        if rule.name.is_empty() {
            return Err("capability rule: empty `name`".to_string());
        }
        if !seen_names.insert(rule.name.clone()) {
            return Err(format!("duplicate capability rule name: {:?}", rule.name));
        }

        for p in &rule.scope.providers {
            if !known_provider_ids.contains(&p.to_ascii_lowercase()) {
                return Err(format!(
                    "rule {:?}: scope.providers entry {p:?} is not a canonical \
                     provider id declared in llm-providers.toml",
                    rule.name
                ));
            }
        }
        if let Some(d) = rule.scope.api_dialect.as_deref() {
            validate_api_dialect(d, &format!("rule {:?}", rule.name))?;
            if !declared_dialects.contains(&d.to_ascii_lowercase()) {
                return Err(format!(
                    "rule {:?}: scope.api_dialect {d:?} is valid but no provider \
                     declares it in llm-providers.toml — either remove the scope \
                     or add the dialect to a provider",
                    rule.name
                ));
            }
        }

        validate_match(&rule.match_, &rule.name)?;
        validate_caps_patch(&rule.caps, &format!("rule {:?}", rule.name))?;
    }

    Ok(ParsedCapabilities {
        defaults: file.defaults,
        rules: file.rules,
    })
}

fn validate_caps_patch(patch: &CapsPatchEntry, where_: &str) -> Result<(), String> {
    if let Some(t) = patch.token_limit_param.as_deref() {
        match t {
            "max-tokens" | "max-completion-tokens" | "max-output-tokens" => {}
            _ => {
                return Err(format!(
                    "{where_}: unknown token_limit_param {t:?} — must be \
                     max-tokens / max-completion-tokens / max-output-tokens"
                ));
            }
        }
    }
    // Rule-scoped caps must set at least one field; an all-None caps block
    // matches the rule but merges nothing, silently shadowing every later
    // rule for the same (provider, model) pair. `[defaults]` is allowed to
    // be all-None (would fall back to ModelCapabilities::default()), so skip
    // the check for the defaults block.
    if where_.starts_with("rule ") {
        let all_none = patch.token_limit_param.is_none()
            && patch.supports_temperature.is_none()
            && patch.supports_stop_sequences.is_none()
            && patch.reasoning.is_none()
            && patch.supports_vision.is_none();
        if all_none {
            return Err(format!(
                "{where_}: all caps fields are None — an empty-caps rule \
                 matches and then shadows every later rule. Either add at \
                 least one Some field, or delete the rule."
            ));
        }
    }
    Ok(())
}

fn validate_match(m: &MatchEntry, rule_name: &str) -> Result<(), String> {
    match m {
        MatchEntry::Any => Ok(()),
        MatchEntry::Exact { model } => {
            if model.is_empty() {
                return Err(format!("rule {rule_name:?}: match.model must not be empty"));
            }
            Ok(())
        }
        MatchEntry::ExactAny { models } => {
            if models.is_empty() {
                return Err(format!(
                    "rule {rule_name:?}: match.models must not be empty — use \
                     kind = \"any\" for match-all rules"
                ));
            }
            for m in models {
                if m.is_empty() {
                    return Err(format!(
                        "rule {rule_name:?}: match.models contains an empty string"
                    ));
                }
            }
            Ok(())
        }
        MatchEntry::PrefixAny { prefixes } => {
            if prefixes.is_empty() {
                return Err(format!(
                    "rule {rule_name:?}: match.prefixes must not be empty — use \
                     kind = \"any\" for match-all rules"
                ));
            }
            for p in prefixes {
                if p.is_empty() {
                    return Err(format!(
                        "rule {rule_name:?}: match.prefixes contains an empty \
                         prefix — every model would match (did you mean kind = \"any\"?)"
                    ));
                }
            }
            Ok(())
        }
    }
}

fn token_limit_variant(s: &str) -> &'static str {
    match s {
        "max-tokens" => "MaxTokens",
        "max-completion-tokens" => "MaxCompletionTokens",
        "max-output-tokens" => "MaxOutputTokens",
        _ => unreachable!("validated earlier: token_limit_param={s}"),
    }
}

// ─── Rust source emission ────────────────────────────────────────────────

pub(super) fn generate_capabilities_rs(caps: &ParsedCapabilities) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(4_096);
    writeln!(
        out,
        "// GENERATED by build.rs from data/model-capabilities.toml. DO NOT EDIT."
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(
        out,
        "pub(crate) static CAPABILITY_DEFAULTS: crate::types::capabilities::CapPatch = {};",
        emit_cap_patch(&caps.defaults)
    )
    .unwrap();
    writeln!(out).unwrap();

    writeln!(
        out,
        "pub(crate) static CAPABILITY_RULES: &[crate::types::capabilities::Rule] = &["
    )
    .unwrap();
    for rule in &caps.rules {
        emit_rule(&mut out, rule);
    }
    writeln!(out, "];").unwrap();

    out
}

fn emit_cap_patch(p: &CapsPatchEntry) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(256);
    out.push_str("crate::types::capabilities::CapPatch {\n");
    writeln!(
        out,
        "    token_limit_param: {},",
        match p.token_limit_param.as_deref() {
            Some(t) => format!(
                "Some(crate::types::TokenLimitParam::{})",
                token_limit_variant(t)
            ),
            None => "None".to_string(),
        }
    )
    .unwrap();
    writeln!(
        out,
        "    supports_temperature: {},",
        opt_bool(p.supports_temperature)
    )
    .unwrap();
    writeln!(
        out,
        "    supports_stop_sequences: {},",
        opt_bool(p.supports_stop_sequences)
    )
    .unwrap();
    writeln!(out, "    reasoning: {},", opt_bool(p.reasoning)).unwrap();
    writeln!(
        out,
        "    supports_vision: {},",
        opt_bool(p.supports_vision)
    )
    .unwrap();
    out.push('}');
    out
}

fn opt_bool(b: Option<bool>) -> String {
    match b {
        Some(v) => format!("Some({v})"),
        None => "None".to_string(),
    }
}

fn emit_rule(out: &mut String, rule: &RuleEntry) {
    use std::fmt::Write as _;

    // The TOML `name` is only retained at build time (uniqueness check);
    // not emitted into the runtime struct to avoid a dead field.
    writeln!(out, "    // rule {:?}", rule.name).unwrap();
    writeln!(out, "    crate::types::capabilities::Rule {{").unwrap();
    writeln!(
        out,
        "        providers: {},",
        str_slice_expr(&rule.scope.providers)
    )
    .unwrap();
    writeln!(
        out,
        "        api_dialect: {},",
        opt_rstr(rule.scope.api_dialect.as_deref())
    )
    .unwrap();
    writeln!(out, "        matcher: {},", emit_matcher(&rule.match_)).unwrap();
    // Indent the nested CapPatch literal by 8 spaces for readability.
    let caps_literal = emit_cap_patch(&rule.caps);
    let indented = caps_literal
        .lines()
        .enumerate()
        .map(|(i, line)| if i == 0 { line.to_string() } else { format!("        {line}") })
        .collect::<Vec<_>>()
        .join("\n");
    writeln!(out, "        caps: {indented},").unwrap();
    writeln!(out, "    }},").unwrap();
}

fn emit_matcher(m: &MatchEntry) -> String {
    match m {
        MatchEntry::Any => "crate::types::capabilities::Matcher::Any".to_string(),
        MatchEntry::Exact { model } => format!(
            "crate::types::capabilities::Matcher::Exact({})",
            rstr(model)
        ),
        MatchEntry::ExactAny { models } => format!(
            "crate::types::capabilities::Matcher::ExactAny({})",
            str_slice_expr(models)
        ),
        MatchEntry::PrefixAny { prefixes } => format!(
            "crate::types::capabilities::Matcher::PrefixAny({})",
            str_slice_expr(prefixes)
        ),
    }
}
