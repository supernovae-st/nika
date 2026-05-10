// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities catalog codegen.
//!
//! Lifted from `nika-catalog/build/capabilities.rs` (978 LOC). All TOML
//! schema types use `#[serde(deny_unknown_fields)]` so a typo like
//! `prefex_any` on `match.kind` produces a clear parse error at build
//! time rather than silently deserialising into the default variant.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{opt_rstr, rstr, str_slice_expr};
use crate::error::CodegenError;
use crate::providers::{ProviderEntry, validate_api_dialect};
use crate::schema::{CAPABILITIES_SCHEMA, assert_schema};

// ─── TOML schema types ───────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct CapabilitiesFile {
    schema: String,
    defaults: CapsPatchEntry,
    #[serde(default)]
    rules: Vec<RuleEntry>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct RuleEntry {
    name: String,
    #[serde(default)]
    scope: ScopeEntry,
    #[serde(rename = "match")]
    match_: MatchEntry,
    caps: CapsPatchEntry,
    /// Author-asserted "this rule intentionally inherits defaults for any
    /// un-set routing-critical field". Documentary at build time.
    #[serde(default)]
    #[allow(
        dead_code,
        reason = "Session 2b authoring hint; Session 3 wires to `cargo::warning`"
    )]
    routing_complete: bool,
}

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
struct ScopeEntry {
    #[serde(default)]
    providers: Vec<String>,
    #[serde(default)]
    api_dialect: Option<String>,
    /// Cloud region scope. `None` = global (all regions).
    #[serde(default)]
    region: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MatchEntry {
    Any,
    Exact {
        model: String,
    },
    ExactAny {
        models: Vec<String>,
    },
    PrefixAny {
        prefixes: Vec<String>,
    },
    /// Word-boundary-anchored substring match (Session 4a A6).
    ContainsAny {
        patterns: Vec<String>,
    },
}

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
    LlamaV4,
    Granite,
    Glm,
    #[serde(rename = "grok")]
    Grok,
}

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum JsonModeToml {
    Unavailable,
    Object,
    Schema,
}

#[derive(Deserialize, Default, Debug)]
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
    tokenizer: Option<TokenizerFamilyToml>,
    #[serde(default)]
    input_modalities: Option<Vec<String>>,
    #[serde(default)]
    output_modalities: Option<Vec<String>>,
    #[serde(default)]
    supported_parameters: Option<Vec<String>>,
    #[serde(default)]
    supports_system_messages: Option<bool>,
    #[serde(default)]
    context_window_tokens: Option<u32>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
    #[serde(default)]
    json_mode: Option<JsonModeToml>,
}

// ─── Public codegen entry ────────────────────────────────────────────────

/// Codegen entry — parse TOML bytes + emit Rust source. The FK check
/// requires the canonical providers list.
pub fn codegen_capabilities(
    toml_bytes: &[u8],
    providers: &[ProviderEntry],
) -> Result<String, CodegenError> {
    let synthetic = Path::new("<bytes>:model-capabilities.toml");
    let parsed = parse_capabilities_bytes(toml_bytes, synthetic, providers)?;
    Ok(generate_capabilities_rs(&parsed))
}

#[derive(Debug)]
struct ParsedCapabilities {
    defaults: CapsPatchEntry,
    rules: Vec<RuleEntry>,
}

fn parse_capabilities_bytes(
    raw: &[u8],
    path: &Path,
    providers: &[ProviderEntry],
) -> Result<ParsedCapabilities, CodegenError> {
    let raw_str = std::str::from_utf8(raw).map_err(|e| CodegenError::SchemaValidation {
        context: path.display().to_string(),
        reason: format!("file is not valid UTF-8: {e}"),
    })?;
    let mut file: CapabilitiesFile =
        toml::from_str(raw_str).map_err(|source| CodegenError::TomlParse {
            path: path.to_path_buf(),
            source,
        })?;

    assert_schema(CAPABILITIES_SCHEMA, &file.schema, path)?;

    // Canonicalise scope.providers at parse time — sort + dedup in place.
    for rule in &mut file.rules {
        rule.scope.providers.sort();
        rule.scope.providers.dedup();
    }

    let path_ctx = path.display().to_string();
    validate_caps_patch(&file.defaults, "[defaults]", true)
        .map_err(|r| CodegenError::schema_validation(&path_ctx, r))?;

    let known_provider_ids: HashSet<String> = providers
        .iter()
        .map(|p| p.id.to_ascii_lowercase())
        .collect();

    let declared_dialects: HashSet<String> = providers
        .iter()
        .filter_map(|p| p.api_dialect.as_deref())
        .map(str::to_ascii_lowercase)
        .collect();

    let mut seen_names: HashSet<String> = HashSet::new();
    for rule in &file.rules {
        if rule.name.is_empty() {
            return Err(CodegenError::schema_validation(
                &path_ctx,
                "capability rule: empty `name`".to_string(),
            ));
        }
        if !seen_names.insert(rule.name.clone()) {
            return Err(CodegenError::schema_validation(
                &path_ctx,
                format!("duplicate capability rule name: {:?}", rule.name),
            ));
        }

        for p in &rule.scope.providers {
            if !known_provider_ids.contains(&p.to_ascii_lowercase()) {
                return Err(CodegenError::foreign_key(
                    format!("rule {:?}", rule.name),
                    format!(
                        "scope.providers entry {p:?} is not a canonical \
                         provider id declared in llm-providers.toml"
                    ),
                ));
            }
        }
        if let Some(d) = rule.scope.api_dialect.as_deref() {
            validate_api_dialect(d, &format!("rule {:?}", rule.name))
                .map_err(|r| CodegenError::schema_validation(&path_ctx, r))?;
            if !declared_dialects.contains(&d.to_ascii_lowercase()) {
                return Err(CodegenError::schema_validation(
                    &path_ctx,
                    format!(
                        "rule {:?}: scope.api_dialect {d:?} is valid but no provider \
                         declares it in llm-providers.toml — either remove the scope \
                         or add the dialect to a provider",
                        rule.name
                    ),
                ));
            }
        }
        if let Some(regions) = &rule.scope.region {
            for r in regions {
                validate_region(r, &format!("rule {:?}", rule.name))
                    .map_err(|reason| CodegenError::schema_validation(&path_ctx, reason))?;
            }
            if !regions.is_empty() {
                return Err(CodegenError::schema_validation(
                    &path_ctx,
                    format!(
                        "rule {:?}: scope.region is set but the resolver does not \
                         evaluate region yet — remove scope.region or wire the \
                         resolver (Phase E2)",
                        rule.name,
                    ),
                ));
            }
        }

        validate_match(&rule.match_, &rule.name)
            .map_err(|r| CodegenError::schema_validation(&path_ctx, r))?;
        validate_caps_patch(&rule.caps, &format!("rule {:?}", rule.name), false)
            .map_err(|r| CodegenError::schema_validation(&path_ctx, r))?;
    }

    check_any_last_in_scope(&file.rules)
        .map_err(|r| CodegenError::schema_validation(&path_ctx, r))?;

    Ok(ParsedCapabilities {
        defaults: file.defaults,
        rules: file.rules,
    })
}

/// Enforce "`Matcher::Any` must be last within its provider scope".
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
        tokenizer,
        input_modalities,
        output_modalities,
        supported_parameters,
        supports_system_messages,
        context_window_tokens,
        max_output_tokens,
        json_mode,
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

    if let Some(list) = input_modalities {
        if list.is_empty() {
            return Err(format!(
                "{where_}: input_modalities = [] is invalid — omit the field \
                 to inherit defaults, or list at least \"text\""
            ));
        }
        for m in list {
            match m.as_str() {
                "text" | "image" | "audio" | "video" | "pdf" | "embedding" | "speech"
                | "image-gen" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown input modality {m:?} — expected \
                         one of: text, image, audio, video, pdf, embedding, speech, image-gen"
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
                "text" | "image" | "audio" | "video" | "pdf" | "embedding" | "speech"
                | "image-gen" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown output modality {m:?} — expected \
                         one of: text, image, audio, video, pdf, embedding, speech, image-gen"
                    ));
                }
            }
        }
    }

    if let Some(flags) = supported_parameters {
        for f in flags {
            match f.as_str() {
                "parallel-tool-calls"
                | "reasoning-effort"
                | "thinking-budget"
                | "prompt-caching"
                | "file-search"
                | "web-search"
                | "streaming-thinking"
                | "batch-api"
                | "context-caching"
                | "predicted-outputs"
                | "computer-use"
                | "citations"
                | "include-reasoning" => {}
                _ => {
                    return Err(format!(
                        "{where_}: unknown param_flag {f:?} — expected one of: \
                         parallel-tool-calls, reasoning-effort, thinking-budget, \
                         prompt-caching, file-search, web-search, streaming-thinking, \
                         batch-api, context-caching, predicted-outputs, computer-use, \
                         citations, include-reasoning"
                    ));
                }
            }
        }
    }

    if let (Some(ctx), Some(max_out)) = (context_window_tokens, max_output_tokens)
        && max_out > ctx
    {
        return Err(format!(
            "{where_}: max_output_tokens ({max_out}) > context_window_tokens ({ctx}) — \
             a model's max output cannot exceed its context window"
        ));
    }

    if where_.starts_with("rule ") {
        let all_none = token_limit_param.is_none()
            && supports_temperature.is_none()
            && supports_stop_sequences.is_none()
            && reasoning.is_none()
            && tokenizer.is_none()
            && input_modalities.is_none()
            && output_modalities.is_none()
            && supported_parameters.is_none()
            && supports_system_messages.is_none()
            && context_window_tokens.is_none()
            && max_output_tokens.is_none()
            && json_mode.is_none();
        if all_none {
            return Err(format!(
                "{where_}: all caps fields are None — an empty-caps rule \
                 matches and then shadows every later rule. Either add at \
                 least one Some field, or delete the rule."
            ));
        }
    }

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
        MatchEntry::ContainsAny { patterns } => {
            if patterns.is_empty() {
                return Err(format!(
                    "rule {rule_name:?}: match.patterns must not be empty — use \
                     kind = \"any\" for match-all rules"
                ));
            }
            for p in patterns {
                if p.is_empty() {
                    return Err(format!(
                        "rule {rule_name:?}: match.patterns contains an empty \
                         pattern — every model would match (did you mean kind = \"any\"?)"
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
        _ => panic_validated("token_limit_param", s),
    }
}

// ─── Rust source emission ────────────────────────────────────────────────

#[must_use]
fn generate_capabilities_rs(caps: &ParsedCapabilities) -> String {
    let mut out = String::with_capacity(4_096);
    let _ = writeln!(
        out,
        "// GENERATED by build.rs from data/model-capabilities.toml. DO NOT EDIT."
    );
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "pub(crate) static CAPABILITY_DEFAULTS: crate::types::capabilities::CapPatch = {};",
        emit_cap_patch(&caps.defaults)
    );
    let _ = writeln!(out);

    let _ = writeln!(
        out,
        "pub(crate) static CAPABILITY_RULES: &[crate::types::capabilities::CapRule] = &["
    );
    for rule in &caps.rules {
        emit_rule(&mut out, rule);
    }
    let _ = writeln!(out, "];");

    out
}

/// Emit a single `Option<T>` field line.
fn emit_opt<T: ?Sized>(
    out: &mut String,
    name: &str,
    value: Option<&T>,
    render_some: impl Fn(&T) -> String,
) {
    let rendered = value.map_or_else(
        || "None".to_string(),
        |v| format!("Some({})", render_some(v)),
    );
    let _ = writeln!(out, "    {name}: {rendered},");
}

/// Declaration-order sort key for `Modality` strings.
fn modality_sort_order(s: &str) -> u8 {
    match s {
        "text" => 0,
        "image" => 1,
        "audio" => 2,
        "video" => 3,
        "pdf" => 4,
        "embedding" => 5,
        "speech" => 6,
        "image-gen" => 7,
        _ => 255,
    }
}

fn param_flag_sort_order(s: &str) -> u8 {
    match s {
        "parallel-tool-calls" => 0,
        "reasoning-effort" => 1,
        "thinking-budget" => 2,
        "prompt-caching" => 3,
        "file-search" => 4,
        "web-search" => 5,
        "streaming-thinking" => 6,
        "batch-api" => 7,
        "context-caching" => 8,
        "predicted-outputs" => 9,
        "computer-use" => 10,
        "citations" => 11,
        "include-reasoning" => 12,
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
        "embedding" => "Embedding",
        "speech" => "Speech",
        "image-gen" => "ImageGen",
        _ => panic_validated("modality", s),
    }
}

fn param_flag_variant(s: &str) -> &'static str {
    match s {
        "parallel-tool-calls" => "ParallelToolCalls",
        "reasoning-effort" => "ReasoningEffort",
        "thinking-budget" => "ThinkingBudget",
        "prompt-caching" => "PromptCaching",
        "file-search" => "FileSearch",
        "web-search" => "WebSearch",
        "streaming-thinking" => "StreamingThinking",
        "batch-api" => "BatchApi",
        "context-caching" => "ContextCaching",
        "predicted-outputs" => "PredictedOutputs",
        "computer-use" => "ComputerUse",
        "citations" => "Citations",
        "include-reasoning" => "IncludeReasoning",
        _ => panic_validated("param_flag", s),
    }
}

#[allow(
    clippy::panic,
    reason = "validated upstream — branch is unreachable in valid input"
)]
fn panic_validated(field: &str, value: &str) -> ! {
    panic!(
        "internal invariant violated: {field}={value:?} reached variant mapping without prior validation"
    );
}

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
        TokenizerFamilyToml::LlamaV4 => "crate::types::TokenizerFamily::LlamaV4",
        TokenizerFamilyToml::Granite => "crate::types::TokenizerFamily::Granite",
        TokenizerFamilyToml::Glm => "crate::types::TokenizerFamily::Glm",
        TokenizerFamilyToml::Grok => "crate::types::TokenizerFamily::Grok",
    }
}

fn emit_json_mode_variant(j: JsonModeToml) -> &'static str {
    match j {
        JsonModeToml::Unavailable => "crate::types::JsonMode::Unavailable",
        JsonModeToml::Object => "crate::types::JsonMode::Object",
        JsonModeToml::Schema => "crate::types::JsonMode::Schema",
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
    emit_opt(
        &mut out,
        "context_window_tokens",
        p.context_window_tokens.as_ref(),
        u32::to_string,
    );
    emit_opt(
        &mut out,
        "max_output_tokens",
        p.max_output_tokens.as_ref(),
        u32::to_string,
    );
    emit_opt(
        &mut out,
        "json_mode",
        p.json_mode.as_ref(),
        |j: &JsonModeToml| emit_json_mode_variant(*j).to_string(),
    );
    out.push('}');
    out
}

fn emit_rule(out: &mut String, rule: &RuleEntry) {
    let _ = writeln!(out, "    // rule {:?}", rule.name);
    let _ = writeln!(out, "    crate::types::capabilities::CapRule {{");
    let _ = writeln!(
        out,
        "        providers: {},",
        str_slice_expr(&rule.scope.providers)
    );
    let _ = writeln!(
        out,
        "        api_dialect: {},",
        opt_rstr(rule.scope.api_dialect.as_deref())
    );
    let _ = writeln!(
        out,
        "        region: {},",
        emit_region_scope(rule.scope.region.as_deref())
    );
    let _ = writeln!(out, "        matcher: {},", emit_matcher(&rule.match_));
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
    let _ = writeln!(out, "        caps: {indented},");
    let _ = writeln!(out, "    }},");
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
        MatchEntry::ContainsAny { patterns } => format!(
            "crate::types::capabilities::Matcher::ContainsAny({})",
            str_slice_expr(patterns)
        ),
    }
}

fn validate_region(r: &str, context: &str) -> Result<(), String> {
    match r {
        "us" | "eu" | "apac" | "china" => Ok(()),
        _ => Err(format!(
            "{context}: unknown region {r:?} (expected one of: us, eu, apac, china)"
        )),
    }
}

fn region_variant(s: &str) -> &'static str {
    match s {
        "us" => "Us",
        "eu" => "Eu",
        "apac" => "Apac",
        "china" => "China",
        _ => panic_validated("region", s),
    }
}

fn emit_region_scope(regions: Option<&[String]>) -> String {
    match regions {
        None | Some(&[]) => "None".to_string(),
        Some(list) => {
            let variants: Vec<String> = list
                .iter()
                .map(|r| format!("crate::types::Region::{}", region_variant(r)))
                .collect();
            format!("Some(&[{}])", variants.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_provider(id: &str, dialect: Option<&str>) -> ProviderEntry {
        ProviderEntry {
            id: id.to_string(),
            name: id.to_string(),
            aliases: vec![],
            env_var: "X".to_string(),
            key_prefixes: vec![],
            default_model: "m".to_string(),
            cheap_model: "m".to_string(),
            requires_key: false,
            description: "x".to_string(),
            models: vec![],
            api_dialect: dialect.map(str::to_string),
            tags: vec![],
            extra_tags: vec![],
        }
    }

    fn minimal_defaults() -> CapsPatchEntry {
        CapsPatchEntry {
            token_limit_param: Some("max-tokens".to_string()),
            supports_temperature: Some(true),
            supports_stop_sequences: Some(true),
            reasoning: Some(false),
            tokenizer: None,
            input_modalities: Some(vec!["text".to_string()]),
            output_modalities: Some(vec!["text".to_string()]),
            supported_parameters: Some(vec![]),
            supports_system_messages: Some(true),
            context_window_tokens: None,
            max_output_tokens: None,
            json_mode: None,
        }
    }

    #[test]
    fn validate_caps_patch_rule_all_none_rejected() {
        let p = CapsPatchEntry::default();
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("all caps fields are None"));
    }

    #[test]
    fn validate_caps_patch_defaults_require_all_fields() {
        let mut p = minimal_defaults();
        p.token_limit_param = None;
        let err = validate_caps_patch(&p, "[defaults]", true).unwrap_err();
        assert!(err.contains("token_limit_param"));
    }

    #[test]
    fn validate_caps_patch_unknown_modality_rejected() {
        let p = CapsPatchEntry {
            input_modalities: Some(vec!["text".to_string(), "magic".to_string()]),
            ..minimal_defaults()
        };
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("unknown input modality"));
    }

    #[test]
    fn validate_caps_patch_input_modalities_must_include_text() {
        let p = CapsPatchEntry {
            input_modalities: Some(vec!["image".to_string()]),
            ..minimal_defaults()
        };
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("must include \"text\""));
    }

    #[test]
    fn validate_caps_patch_max_out_must_not_exceed_ctx() {
        let p = CapsPatchEntry {
            context_window_tokens: Some(1_000),
            max_output_tokens: Some(2_000),
            ..minimal_defaults()
        };
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("max_output_tokens"));
    }

    #[test]
    fn validate_caps_patch_unknown_token_limit_rejected() {
        let p = CapsPatchEntry {
            token_limit_param: Some("invalid".to_string()),
            ..minimal_defaults()
        };
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("unknown token_limit_param"));
    }

    #[test]
    fn validate_caps_patch_unknown_param_flag_rejected() {
        let p = CapsPatchEntry {
            supported_parameters: Some(vec!["bogus".to_string()]),
            ..minimal_defaults()
        };
        let err = validate_caps_patch(&p, "rule \"x\"", false).unwrap_err();
        assert!(err.contains("unknown param_flag"));
    }

    #[test]
    fn validate_match_each_variant() {
        validate_match(&MatchEntry::Any, "x").unwrap();
        validate_match(
            &MatchEntry::Exact {
                model: "m".to_string(),
            },
            "x",
        )
        .unwrap();
        validate_match(
            &MatchEntry::ExactAny {
                models: vec!["m".to_string()],
            },
            "x",
        )
        .unwrap();
        validate_match(
            &MatchEntry::PrefixAny {
                prefixes: vec!["p".to_string()],
            },
            "x",
        )
        .unwrap();
        validate_match(
            &MatchEntry::ContainsAny {
                patterns: vec!["c".to_string()],
            },
            "x",
        )
        .unwrap();
    }

    #[test]
    fn validate_match_exact_empty_rejected() {
        let err = validate_match(
            &MatchEntry::Exact {
                model: String::new(),
            },
            "x",
        )
        .unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_match_exact_any_empty_rejected() {
        let err = validate_match(&MatchEntry::ExactAny { models: vec![] }, "x").unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_match_prefix_any_empty_string_rejected() {
        let err = validate_match(
            &MatchEntry::PrefixAny {
                prefixes: vec![String::new()],
            },
            "x",
        )
        .unwrap_err();
        assert!(err.contains("empty prefix"));
    }

    #[test]
    fn check_any_last_in_scope_detects_dead_rule() {
        let any_rule = RuleEntry {
            name: "any-first".to_string(),
            scope: ScopeEntry::default(),
            match_: MatchEntry::Any,
            caps: CapsPatchEntry {
                reasoning: Some(true),
                ..Default::default()
            },
            routing_complete: false,
        };
        let later = RuleEntry {
            name: "exact-later".to_string(),
            scope: ScopeEntry::default(),
            match_: MatchEntry::Exact {
                model: "m".to_string(),
            },
            caps: CapsPatchEntry {
                reasoning: Some(false),
                ..Default::default()
            },
            routing_complete: false,
        };
        let err = check_any_last_in_scope(&[any_rule, later]).unwrap_err();
        assert!(err.contains("appears BEFORE non-Any rule"));
    }

    #[test]
    fn check_any_last_in_scope_accepts_any_at_end() {
        let exact = RuleEntry {
            name: "exact-first".to_string(),
            scope: ScopeEntry::default(),
            match_: MatchEntry::Exact {
                model: "m".to_string(),
            },
            caps: CapsPatchEntry {
                reasoning: Some(false),
                ..Default::default()
            },
            routing_complete: false,
        };
        let any_rule = RuleEntry {
            name: "any-last".to_string(),
            scope: ScopeEntry::default(),
            match_: MatchEntry::Any,
            caps: CapsPatchEntry {
                reasoning: Some(true),
                ..Default::default()
            },
            routing_complete: false,
        };
        check_any_last_in_scope(&[exact, any_rule]).unwrap();
    }

    #[test]
    fn validate_region_closed_set() {
        for ok in ["us", "eu", "apac", "china"] {
            assert!(validate_region(ok, "x").is_ok());
        }
        assert!(validate_region("nope", "x").is_err());
    }

    #[test]
    fn parse_capabilities_rejects_unknown_provider_in_scope() {
        let toml = br#"
schema = "nika/model-capabilities@1.0"

[defaults]
token_limit_param = "max-tokens"
supports_temperature = true
supports_stop_sequences = true
reasoning = false
input_modalities = ["text"]
output_modalities = ["text"]
supported_parameters = []
supports_system_messages = true

[[rules]]
name = "x"
scope = { providers = ["unknown"] }
match = { kind = "any" }
caps = { reasoning = true }
"#;
        let providers = vec![fixture_provider("known", None)];
        let err = parse_capabilities_bytes(toml, Path::new("/x"), &providers).unwrap_err();
        match err {
            CodegenError::ForeignKey { reason, .. } => {
                assert!(reason.contains("not a canonical"));
            }
            other => panic!("expected ForeignKey, got {other:?}"),
        }
    }

    #[test]
    fn parse_capabilities_canonicalises_scope_providers() {
        let toml = br#"
schema = "nika/model-capabilities@1.0"

[defaults]
token_limit_param = "max-tokens"
supports_temperature = true
supports_stop_sequences = true
reasoning = false
input_modalities = ["text"]
output_modalities = ["text"]
supported_parameters = []
supports_system_messages = true

[[rules]]
name = "x"
scope = { providers = ["zeta", "alpha", "alpha"] }
match = { kind = "any" }
caps = { reasoning = true }
"#;
        let providers = vec![
            fixture_provider("zeta", None),
            fixture_provider("alpha", None),
        ];
        let parsed = parse_capabilities_bytes(toml, Path::new("/x"), &providers).unwrap();
        // sorted + dedup
        assert_eq!(parsed.rules[0].scope.providers, vec!["alpha", "zeta"]);
    }

    #[test]
    fn generate_emits_static_names() {
        let parsed = ParsedCapabilities {
            defaults: minimal_defaults(),
            rules: vec![],
        };
        let s = generate_capabilities_rs(&parsed);
        assert!(s.contains("pub(crate) static CAPABILITY_DEFAULTS"));
        assert!(s.contains("pub(crate) static CAPABILITY_RULES"));
    }

    #[test]
    fn idempotent_emission_capabilities() {
        let parsed_a = ParsedCapabilities {
            defaults: minimal_defaults(),
            rules: vec![],
        };
        let parsed_b = ParsedCapabilities {
            defaults: minimal_defaults(),
            rules: vec![],
        };
        let a = generate_capabilities_rs(&parsed_a);
        let b = generate_capabilities_rs(&parsed_b);
        assert_eq!(a, b);
    }
}
