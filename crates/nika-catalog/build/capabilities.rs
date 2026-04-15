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
    CAPABILITIES_SCHEMA, ProviderEntry, assert_schema, opt_rstr, rstr, str_slice_expr,
    validate_api_dialect,
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
    /// Author-asserted "this rule intentionally inherits defaults for any
    /// un-set routing-critical field". Session-2b flag — purely
    /// documentary at build time (not yet wired into a `cargo::warning`
    /// emitter). Read by `deny_unknown_fields` so the TOML can carry it
    /// for `Matcher::Any` rules that intentionally fall back. Session 3
    /// plans to wire it to the routing-critical-field warning; until
    /// then, the flag serves as inline author intent.
    ///
    /// Not emitted into the runtime `Rule` struct (would be dead data
    /// there — routing happens at TOML-authoring time, not at
    /// materialise time).
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "Session 2b authoring hint; Session 3 wires to `cargo::warning`"
    )]
    routing_complete: bool,
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

/// Typed TOML tokenizer enum — serde replaces hand-written string validation.
///
/// The `DeepSeek` variant uses an explicit `#[serde(rename = "deepseek")]`
/// because the `rename_all = "kebab-case"` attribute would produce
/// `"deep-seek"` (hyphen at the lowercase→uppercase transition) — not the
/// compact `"deepseek"` the canonical TOML uses. This is the P0 fix
/// surfaced in Session 2b round-5 review.
#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum TokenizerFamilyToml {
    Cl100k,
    O200k,
    ClaudeV3,
    Gemini,
    LlamaV3,
    MistralV3,
    #[serde(rename = "deepseek")]
    DeepSeek,
    Qwen,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CapsPatchEntry {
    // ── existing 5 slots (Session 2a) ──────────────────────────────────
    #[serde(default)]
    token_limit_param: Option<String>,
    #[serde(default)]
    supports_temperature: Option<bool>,
    #[serde(default)]
    supports_stop_sequences: Option<bool>,
    #[serde(default)]
    reasoning: Option<bool>,
    // ── Session 2b additions (5 new slots) ─────────────────────────────
    /// Typed enum (not String) — serde parse error replaces hand-written
    /// validation on an unknown tokenizer name.
    #[serde(default)]
    tokenizer: Option<TokenizerFamilyToml>,
    /// String list validated against the fixed Modality vocabulary below;
    /// invalid strings are build-time errors (not runtime panics).
    #[serde(default)]
    input_modalities: Option<Vec<String>>,
    #[serde(default)]
    output_modalities: Option<Vec<String>>,
    /// String list validated against the fixed `ParamFlag` vocabulary.
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
    #[serde(default)]
    supports_system_messages: Option<bool>,
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
    let mut file: CapabilitiesFile =
        toml::from_str(&raw).map_err(|e| format!("parsing {}: {e}", path.display()))?;

    assert_schema(CAPABILITIES_SCHEMA, &file.schema, path)?;

    // Canonicalise scope.providers at parse time — sort + dedup in place.
    //
    // Phase H fix: `check_any_last_in_scope` compares `scope.providers`
    // lists via `Vec<String>` equality, which is order-sensitive. Two
    // rules that author-cosmetic differ only in provider ordering
    // (`["openai", "openrouter"]` vs `["openrouter", "openai"]`) would
    // be treated as different scopes, silently escaping the
    // Any-must-be-last check. Canonicalising to a stable sort means the
    // invariant holds regardless of TOML-author order.
    //
    // `sort` is stable; `dedup` removes accidental duplicates (a future
    // author could paste `["openai", "openai"]`). Provider names are
    // short, count ≤ 10 per rule — cost is negligible.
    for rule in &mut file.rules {
        rule.scope.providers.sort();
        rule.scope.providers.dedup();
    }

    validate_caps_patch(&file.defaults, "[defaults]", /* require_all */ true)?;

    // Set of canonical provider ids (lowercased) for FK checks — rules
    // reference providers by canonical id only, not by alias.
    let known_provider_ids: HashSet<String> = providers
        .iter()
        .map(|p| p.id.to_ascii_lowercase())
        .collect();

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
        validate_caps_patch(
            &rule.caps,
            &format!("rule {:?}", rule.name),
            /* require_all */ false,
        )?;
    }

    // Enforce the "`Matcher::Any` must be last within its provider scope"
    // invariant documented at the top of data/model-capabilities.toml.
    // Without this, a scoped rule inserted after a `Matcher::Any` rule for
    // the same provider gets silently shadowed — first-match-wins + `any`
    // always matches = the later rule is dead. P0 fix from Session 2b
    // review swarm.
    check_any_last_in_scope(&file.rules)?;

    Ok(ParsedCapabilities {
        defaults: file.defaults,
        rules: file.rules,
    })
}

/// Enforce "`Matcher::Any` must be last within its provider scope".
///
/// Two rules share a scope when their `scope.providers` lists are equal
/// (order-sensitive via `Vec<String>` equality; the TOML author must use
/// a canonical ordering per scope — every `[openai, openrouter]` rule
/// uses exactly that order). Within each scope bucket, any non-`Any`
/// rule that appears after an `Any` rule is dead. The check is O(N²) but
/// N ≤ 40 at build time, so the cost is negligible.
fn check_any_last_in_scope(rules: &[RuleEntry]) -> Result<(), String> {
    for i in 0..rules.len() {
        let MatchEntry::Any = rules[i].match_ else {
            continue;
        };
        let scope_i = &rules[i].scope.providers;
        let dialect_i = rules[i].scope.api_dialect.as_deref();
        for later in rules.iter().skip(i + 1) {
            let scope_j = &later.scope.providers;
            let dialect_j = later.scope.api_dialect.as_deref();
            if scope_i == scope_j
                && dialect_i == dialect_j
                && !matches!(later.match_, MatchEntry::Any)
            {
                return Err(format!(
                    "rule {:?} (`Matcher::Any`) appears BEFORE non-Any rule \
                     {:?} in the same scope (providers={:?}, \
                     api_dialect={:?}) — Any always matches, so rule \
                     {:?} is dead. Move rule {:?} to appear BEFORE rule \
                     {:?}, or remove one of them.",
                    rules[i].name,
                    later.name,
                    scope_i,
                    dialect_i,
                    later.name,
                    later.name,
                    rules[i].name,
                ));
            }
        }
    }
    Ok(())
}

/// Validate every field of a `CapsPatchEntry`.
///
/// Uses an exhaustive destructure so `#![deny(unused_variables)]` at the
/// top of the parent module turns an omitted field into a compile error
/// — adding a new slot to [`CapsPatchEntry`] without updating this
/// function fails the build.
///
/// `require_all` — when `true`, every `Option<T>` field that has a
/// `unwrap_or` fallback in `CapPatch::materialize` MUST be `Some`. This
/// applies to `[defaults]` so the hard-coded fallbacks in `materialize`
/// become structurally unreachable: a mis-authored `[defaults]` block that
/// omits (for example) `supports_temperature` gets caught at build time
/// instead of silently letting `materialize` take the `unwrap_or(true)`
/// branch. Rules pass `false` — rules intentionally omit fields they
/// don't override. The single exception is `tokenizer`, which stays
/// legitimately optional on both `[defaults]` and rules because
/// `materialize` uses `self.tokenizer.or(defaults.tokenizer)` with NO
/// `unwrap_or` fallback (absent tokenizer = conservative estimation by
/// design, not a missing value).
// REASON: covers 9 fields with per-field error-message branches
// (input/output modality validation including must-contain-"text", param-flag
// vocabulary, token_limit_param whitelist, per-field require-some check,
// all-None rule check). Splitting the vocabulary checks into helpers would
// scatter the error-message text across the file — readability win is zero.
#[allow(clippy::too_many_lines)]
fn validate_caps_patch(
    patch: &CapsPatchEntry,
    where_: &str,
    require_all: bool,
) -> Result<(), String> {
    let CapsPatchEntry {
        token_limit_param,
        supports_temperature,
        supports_stop_sequences,
        reasoning,
        tokenizer, // typed enum — serde already validated
        input_modalities,
        output_modalities,
        supported_parameters,
        supports_system_messages,
    } = patch;

    if let Some(t) = token_limit_param.as_deref() {
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

    // Validate input_modalities strings against the fixed Modality vocabulary.
    // Reject unknown modalities at build time — clearer error than silently
    // falling through to a parse failure in the generated Rust.
    if let Some(list) = input_modalities {
        if list.is_empty() {
            return Err(format!(
                "{where_}: input_modalities = [] is invalid — omit the field \
                 to inherit defaults, or list at least \"text\""
            ));
        }
        for m in list {
            match m.as_str() {
                "text" | "image" | "audio" | "video" | "pdf" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown input modality {m:?} — expected \
                         one of: text, image, audio, video, pdf"
                    ));
                }
            }
        }
        if !list.iter().any(|m| m == "text") {
            return Err(format!(
                "{where_}: input_modalities must include \"text\" (chat-capable \
                 models always accept text input — explicit omission would \
                 corrupt the runtime's prompt-building path)"
            ));
        }
    }
    if let Some(list) = output_modalities {
        if list.is_empty() {
            return Err(format!(
                "{where_}: output_modalities = [] is invalid — omit the field \
                 to inherit defaults, or list at least \"text\""
            ));
        }
        for m in list {
            match m.as_str() {
                "text" | "image" | "audio" | "video" | "pdf" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown output modality {m:?} — expected \
                         one of: text, image, audio, video, pdf"
                    ));
                }
            }
        }
    }

    // Validate supported_parameters strings against ParamFlag vocabulary.
    if let Some(flags) = supported_parameters {
        for f in flags {
            match f.as_str() {
                "parallel-tool-calls"
                | "reasoning-effort"
                | "thinking-budget"
                | "prompt-caching"
                | "structured-output-native"
                | "file-search"
                | "web-search"
                | "streaming-thinking" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown param_flag {f:?} — expected one of: \
                         parallel-tool-calls, reasoning-effort, thinking-budget, \
                         prompt-caching, structured-output-native, file-search, \
                         web-search, streaming-thinking"
                    ));
                }
            }
        }
    }

    // Rule-scoped caps must set at least one field; an all-None caps block
    // matches the rule but merges nothing, silently shadowing every later
    // rule for the same (provider, model) pair. `[defaults]` never lands
    // here — it goes through the require_all branch below, which is
    // strictly stronger.
    if where_.starts_with("rule ") {
        let all_none = token_limit_param.is_none()
            && supports_temperature.is_none()
            && supports_stop_sequences.is_none()
            && reasoning.is_none()
            && tokenizer.is_none()
            && input_modalities.is_none()
            && output_modalities.is_none()
            && supported_parameters.is_none()
            && supports_system_messages.is_none();
        if all_none {
            return Err(format!(
                "{where_}: all caps fields are None — an empty-caps rule \
                 matches and then shadows every later rule. Either add at \
                 least one Some field, or delete the rule."
            ));
        }
    }

    // `require_all` — the `[defaults]` block MUST set every field that has
    // a fallback in `CapPatch::materialize` (8 fields). Without this, an
    // omitted field in `[defaults]` silently collapses to the `unwrap_or`
    // in `materialize` — two sources of truth disagreeing invisibly.
    // `tokenizer` is excluded by design: `materialize` uses
    // `self.tokenizer.or(defaults.tokenizer)` with no unwrap_or, so
    // `tokenizer = None` in `[defaults]` is semantically well-defined
    // ("no tokenizer baseline; fall back to conservative estimation").
    if require_all {
        let missing: &[(&str, bool)] = &[
            ("token_limit_param", token_limit_param.is_none()),
            ("supports_temperature", supports_temperature.is_none()),
            ("supports_stop_sequences", supports_stop_sequences.is_none()),
            ("reasoning", reasoning.is_none()),
            ("input_modalities", input_modalities.is_none()),
            ("output_modalities", output_modalities.is_none()),
            ("supported_parameters", supported_parameters.is_none()),
            (
                "supports_system_messages",
                supports_system_messages.is_none(),
            ),
        ];
        for (name, is_none) in missing {
            if *is_none {
                return Err(format!(
                    "{where_}: field `{name}` is required — every field with a \
                     `unwrap_or` fallback in CapPatch::materialize must be \
                     explicitly set in `[defaults]` so the fallback branch \
                     stays structurally unreachable. `tokenizer` is the only \
                     legitimately optional field on `[defaults]`."
                ));
            }
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

/// Emit a single `Option<T>` field line. Used by [`emit_cap_patch`] so new
/// optional fields in Session 2b+ add one line instead of a 4-line
/// `writeln!` block each.
///
/// `render_some` takes the inner `T` and returns its Rust literal
/// (e.g. `"true"`, `"Some(\"foo\")"`, `"crate::types::Modality::Image"`).
fn emit_opt<T: ?Sized>(
    out: &mut String,
    name: &str,
    value: Option<&T>,
    render_some: impl Fn(&T) -> String,
) {
    use std::fmt::Write as _;
    let rendered = value.map_or_else(
        || "None".to_string(),
        |v| format!("Some({})", render_some(v)),
    );
    writeln!(out, "    {name}: {rendered},").unwrap();
}

/// Declaration-order sort key for [`Modality`] strings.
///
/// P0-2 fix: the enum derives `Ord` by declaration order
/// (`Text=0, Image=1, Audio=2, Video=3, Pdf=4`), but naive alphabetical
/// sorting emits `audio < image < pdf < text < video` — the emitted slice
/// would NOT be sorted under the derived `Ord`, and any future caller
/// using `binary_search(&Modality::Text)` would silently miss.
fn modality_sort_order(s: &str) -> u8 {
    match s {
        "text" => 0,
        "image" => 1,
        "audio" => 2,
        "video" => 3,
        "pdf" => 4,
        _ => 255, // validate_caps_patch rejects unknown strings earlier
    }
}

/// Same invariant as [`modality_sort_order`] — declaration order must
/// agree with emitted slice order.
fn param_flag_sort_order(s: &str) -> u8 {
    match s {
        "parallel-tool-calls" => 0,
        "reasoning-effort" => 1,
        "thinking-budget" => 2,
        "prompt-caching" => 3,
        "structured-output-native" => 4,
        "file-search" => 5,
        "web-search" => 6,
        "streaming-thinking" => 7,
        _ => 255,
    }
}

fn modality_variant(s: &str) -> &'static str {
    match s {
        "text" => "Text",
        "image" => "Image",
        "audio" => "Audio",
        "video" => "Video",
        "pdf" => "Pdf",
        _ => unreachable!("validated earlier: modality={s}"),
    }
}

fn param_flag_variant(s: &str) -> &'static str {
    match s {
        "parallel-tool-calls" => "ParallelToolCalls",
        "reasoning-effort" => "ReasoningEffort",
        "thinking-budget" => "ThinkingBudget",
        "prompt-caching" => "PromptCaching",
        "structured-output-native" => "StructuredOutputNative",
        "file-search" => "FileSearch",
        "web-search" => "WebSearch",
        "streaming-thinking" => "StreamingThinking",
        _ => unreachable!("validated earlier: param_flag={s}"),
    }
}

/// Emit a `&[Modality::X, Modality::Y, …]` literal, sorted by declaration
/// order (P0-2 invariant) with duplicates collapsed.
fn emit_modality_slice(list: &[String]) -> String {
    let mut sorted = list.to_vec();
    sorted.sort_by_key(|m| modality_sort_order(m));
    sorted.dedup();
    let variants: Vec<String> = sorted
        .iter()
        .map(|m| format!("crate::types::Modality::{}", modality_variant(m)))
        .collect();
    format!("&[{}]", variants.join(", "))
}

fn emit_param_flag_slice(list: &[String]) -> String {
    let mut sorted = list.to_vec();
    sorted.sort_by_key(|p| param_flag_sort_order(p));
    sorted.dedup();
    let variants: Vec<String> = sorted
        .iter()
        .map(|p| format!("crate::types::ParamFlag::{}", param_flag_variant(p)))
        .collect();
    format!("&[{}]", variants.join(", "))
}

fn emit_tokenizer_variant(t: TokenizerFamilyToml) -> &'static str {
    match t {
        TokenizerFamilyToml::Cl100k => "crate::types::TokenizerFamily::Cl100k",
        TokenizerFamilyToml::O200k => "crate::types::TokenizerFamily::O200k",
        TokenizerFamilyToml::ClaudeV3 => "crate::types::TokenizerFamily::ClaudeV3",
        TokenizerFamilyToml::Gemini => "crate::types::TokenizerFamily::Gemini",
        TokenizerFamilyToml::LlamaV3 => "crate::types::TokenizerFamily::LlamaV3",
        TokenizerFamilyToml::MistralV3 => "crate::types::TokenizerFamily::MistralV3",
        TokenizerFamilyToml::DeepSeek => "crate::types::TokenizerFamily::DeepSeek",
        TokenizerFamilyToml::Qwen => "crate::types::TokenizerFamily::Qwen",
    }
}

fn emit_cap_patch(p: &CapsPatchEntry) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("crate::types::capabilities::CapPatch {\n");
    emit_opt(
        &mut out,
        "token_limit_param",
        p.token_limit_param.as_deref(),
        |t| format!("crate::types::TokenLimitParam::{}", token_limit_variant(t)),
    );
    emit_opt(
        &mut out,
        "supports_temperature",
        p.supports_temperature.as_ref(),
        bool::to_string,
    );
    emit_opt(
        &mut out,
        "supports_stop_sequences",
        p.supports_stop_sequences.as_ref(),
        bool::to_string,
    );
    emit_opt(&mut out, "reasoning", p.reasoning.as_ref(), bool::to_string);
    // Session 2b additions — 5 new slots.
    emit_opt(
        &mut out,
        "input_modalities",
        p.input_modalities.as_deref(),
        |list: &[String]| emit_modality_slice(list),
    );
    emit_opt(
        &mut out,
        "output_modalities",
        p.output_modalities.as_deref(),
        |list: &[String]| emit_modality_slice(list),
    );
    emit_opt(
        &mut out,
        "tokenizer",
        p.tokenizer.as_ref(),
        |t: &TokenizerFamilyToml| emit_tokenizer_variant(*t).to_string(),
    );
    emit_opt(
        &mut out,
        "supported_parameters",
        p.supported_parameters.as_deref(),
        |list: &[String]| emit_param_flag_slice(list),
    );
    emit_opt(
        &mut out,
        "supports_system_messages",
        p.supports_system_messages.as_ref(),
        bool::to_string,
    );
    out.push('}');
    out
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
        .map(|(i, line)| {
            if i == 0 {
                line.to_string()
            } else {
                format!("        {line}")
            }
        })
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
