// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model pricing catalog codegen — TOML schema, validation, Rust emission.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::emit::{opt_f64, opt_plain, opt_rstr, rstr};
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
    /// Upstream `limit.output` (schema `@1.2`). Absent = not disclosed.
    /// The generator drops upstream zeros, so a present value is > 0.
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    /// Upstream `limit.context` (schema `@1.2`). Absent = not disclosed.
    #[serde(default)]
    pub context_window_tokens: Option<u32>,
    /// Upstream `open_weights` (schema `@1.2`). Absent = not disclosed —
    /// which is NOT the same as `false`.
    #[serde(default)]
    pub open_weights: Option<bool>,
    /// Upstream `status` (schema `@1.2`), e.g. `deprecated` · `beta`.
    /// Open vocabulary — validated for shape, never against a fixed set.
    #[serde(default)]
    pub status: Option<String>,
    /// Sourced energy figure (schema `@1.3`). Absent = no sourced
    /// measurement or estimate vendored — never a zero.
    #[serde(default)]
    pub energy: Option<EnergyEntry>,
    /// Tokenizer family id (schema `@1.3`), e.g. `o200k_base`. Open
    /// ecosystem vocabulary — validated for shape (`[a-z0-9._-]`, ≤64),
    /// never against a fixed set. Absent = not vendored.
    #[serde(default)]
    pub tokenizer: Option<String>,
    /// Replay class (schema `@1.3`): `seed` · `best-effort` · `none`.
    /// OUR vocabulary — validated against the closed set. Absent = not
    /// yet researched, which is NOT `none`.
    #[serde(default)]
    pub determinism: Option<String>,
}

/// One sourced energy claim (schema `@1.3`) — Wh per million OUTPUT
/// tokens plus the two axes that keep two honest numbers comparable
/// (WHO measured × WHAT was measured), a citable source, and a month.
///
/// Every field is REQUIRED once the table is present: a number without
/// its provenance, scope, source or date is not a fact, and
/// `deny_unknown_fields` makes a typo'd axis fail the build instead of
/// silently vanishing.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct EnergyEntry {
    /// Watt-hours per million OUTPUT tokens (decode dominates measured
    /// inference energy — a per-total figure would reward long prompts).
    /// Must be finite and > 0: zero would claim free inference; the
    /// null is the absent table.
    pub wh_per_mtok_out: f64,
    /// WHO produced the number: `measured-local` · `independent-measured`
    /// · `vendor-claim` · `independent-estimate` (closed set).
    pub provenance: String,
    /// WHAT the number covers: `gpu` · `device` · `fleet` (closed set).
    /// A GPU-only figure is roughly half a fleet figure for the same
    /// model — without this axis two truthful numbers are incomparable.
    pub scope: String,
    /// Citable origin (benchmark · hardware · runtime version).
    pub source: String,
    /// Month the figure was produced, ISO `YYYY-MM`.
    pub measured_at: String,
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

        validate_entry_facts(entry, &ctx)?;
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

/// One entry's per-field facts — rates, upstream limits, research claims.
///
/// Upstream facts (@1.2): a stored zero is the failure mode this schema
/// exists to prevent — upstream writes 0 for "not applicable" (image ·
/// video · speech models), and a 0 read as a ceiling reports the
/// least-bounded model as the most-bounded. The generator drops those; a
/// zero arriving here is a generator bug, so it fails the build.
///
/// Research facts (@1.3): same absence law, one rule harder — a present
/// claim carries its whole chain of custody (value · provenance · scope ·
/// source · date), because an unattributed energy number is exactly the
/// class of plausible garbage this catalog exists to keep out.
///
/// `max_output_tokens <= context_window_tokens` is deliberately NOT
/// checked: on non-text models the two count different units, and 10 of
/// the 633 rules in the vendored 2026-07-28 snapshot have output >
/// context. That invariant (NIKA-014) governs our hand-authored
/// capability rules; here we mirror somebody else's catalog and must be
/// able to mirror it faithfully.
fn validate_entry_facts(entry: &PricingEntry, ctx: &str) -> Result<(), CodegenError> {
    validate_rate(entry.input_per_million, "input_per_million", ctx)?;
    validate_rate(entry.output_per_million, "output_per_million", ctx)?;
    if let Some(v) = entry.cache_write_per_million {
        validate_rate(v, "cache_write_per_million", ctx)?;
    }
    if let Some(v) = entry.cache_read_per_million {
        validate_rate(v, "cache_read_per_million", ctx)?;
    }
    if let Some(v) = entry.image_per_million {
        validate_rate(v, "image_per_million", ctx)?;
    }
    if let Some(v) = entry.reasoning_tokens_per_million {
        validate_rate(v, "reasoning_tokens_per_million", ctx)?;
    }

    validate_token_limit(entry.max_output_tokens, "max_output_tokens", ctx)?;
    validate_token_limit(entry.context_window_tokens, "context_window_tokens", ctx)?;
    if let Some(s) = entry.status.as_deref() {
        validate_status(s, ctx)?;
    }

    if let Some(e) = entry.energy.as_ref() {
        validate_energy(e, ctx)?;
    }
    if let Some(t) = entry.tokenizer.as_deref() {
        validate_tokenizer(t, ctx)?;
    }
    if let Some(d) = entry.determinism.as_deref() {
        validate_determinism(d, ctx)?;
    }
    Ok(())
}

/// A token limit that is present must be a real limit. Absence is how the
/// snapshot says "not disclosed"; zero would say "bounded by nothing",
/// which is the opposite of what upstream means by it.
fn validate_token_limit(v: Option<u32>, field: &str, ctx: &str) -> Result<(), CodegenError> {
    if v == Some(0) {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "{field} is 0 — upstream writes 0 for 'not applicable'; \
                 omit the field instead (absent = not disclosed)"
            ),
        ));
    }
    Ok(())
}

/// `status` carries upstream's vocabulary, which grows without asking us,
/// so this checks SHAPE only — never membership in a fixed set. A closed
/// list here would turn "upstream added a value" into a failed refresh.
fn validate_status(s: &str, ctx: &str) -> Result<(), CodegenError> {
    let ok = !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !ok {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "status must be 1..=32 chars of [a-z0-9-] (got {s:?}) — \
                 shape only; the value set is upstream's to grow"
            ),
        ));
    }
    Ok(())
}

/// An energy claim must arrive whole: a finite positive figure, both
/// axes from their closed sets, a non-empty source, a real month.
/// These are OUR vocabularies (measurement classes we defined), so
/// closed-set validation cannot break a refresh — only a deliberate
/// schema decision grows them.
fn validate_energy(e: &EnergyEntry, ctx: &str) -> Result<(), CodegenError> {
    const PROVENANCES: &[&str] = &[
        "measured-local",
        "independent-measured",
        "vendor-claim",
        "independent-estimate",
    ];
    const SCOPES: &[&str] = &["gpu", "device", "fleet"];
    if !e.wh_per_mtok_out.is_finite() || e.wh_per_mtok_out <= 0.0 {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "energy.wh_per_mtok_out must be finite and > 0 (got {:?}) — \
                 zero would claim free inference; omit the table instead \
                 (absent = no sourced figure)",
                e.wh_per_mtok_out
            ),
        ));
    }
    if !PROVENANCES.contains(&e.provenance.as_str()) {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "energy.provenance must be one of measured-local / \
                 independent-measured / vendor-claim / independent-estimate \
                 (got {:?})",
                e.provenance
            ),
        ));
    }
    if !SCOPES.contains(&e.scope.as_str()) {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "energy.scope must be one of gpu / device / fleet (got {:?}) — \
                 a figure without its scope makes honest numbers incomparable",
                e.scope
            ),
        ));
    }
    if e.source.trim().is_empty() {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            "energy.source must not be empty — a number without a citation \
             is not a fact; omit the table instead"
                .to_string(),
        ));
    }
    if !is_iso_month(&e.measured_at) {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "energy.measured_at must be YYYY-MM (got {:?}) — energy \
                 figures rot with hardware generations; a dateless figure \
                 is not a fact",
                e.measured_at
            ),
        ));
    }
    Ok(())
}

/// `YYYY-MM` — the month grain [`EnergyEntry::measured_at`] carries.
/// Same byte discipline as [`is_iso_date`], one segment shorter.
fn is_iso_month(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 7 || b[4] != b'-' {
        return false;
    }
    let digits = |r: core::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !digits(0..4) || !digits(5..7) {
        return false;
    }
    (1..=12).contains(&s[5..7].parse::<u8>().unwrap_or(0))
}

/// Tokenizer family ids belong to the OPEN ecosystem vocabulary (new
/// models ship new tokenizers without asking us), so this checks SHAPE
/// only — `[a-z0-9._-]`, 1..=64 — the `status` discipline with dots and
/// underscores admitted for tiktoken-style names (`o200k_base`).
fn validate_tokenizer(t: &str, ctx: &str) -> Result<(), CodegenError> {
    let ok = !t.is_empty()
        && t.len() <= 64
        && t.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
        });
    if !ok {
        return Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!(
                "tokenizer must be 1..=64 chars of [a-z0-9._-] (got {t:?}) — \
                 shape only; the family set is the ecosystem's to grow"
            ),
        ));
    }
    Ok(())
}

/// `determinism` is OUR replay vocabulary — closed set. `none` is a
/// researched FACT (« no replay story »); an unresearched model omits
/// the field. Collapsing the two would report the least-studied model
/// as the least replayable.
fn validate_determinism(d: &str, ctx: &str) -> Result<(), CodegenError> {
    match d {
        "seed" | "best-effort" | "none" => Ok(()),
        _ => Err(CodegenError::schema_validation(
            ctx.to_string(),
            format!("determinism must be one of seed / best-effort / none (got {d:?})"),
        )),
    }
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
    // The schema marker rides the emitted file (F-P18 · the boot pin
    // journals it): ONE source of truth — this crate's `PRICING_SCHEMA`
    // const, which `assert_schema` already proved the TOML speaks.
    let _ = writeln!(
        out,
        "pub(crate) const PRICING_SCHEMA: &str = {PRICING_SCHEMA:?};"
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
    let _ = writeln!(
        out,
        "        max_output_tokens: {},",
        opt_plain(e.max_output_tokens)
    );
    let _ = writeln!(
        out,
        "        context_window_tokens: {},",
        opt_plain(e.context_window_tokens)
    );
    let _ = writeln!(out, "        open_weights: {},", opt_plain(e.open_weights));
    let _ = writeln!(out, "        status: {},", opt_rstr(e.status.as_deref()));
    let _ = writeln!(out, "        energy: {},", energy_expr(e.energy.as_ref()));
    let _ = writeln!(
        out,
        "        tokenizer: {},",
        opt_rstr(e.tokenizer.as_deref())
    );
    let _ = writeln!(
        out,
        "        determinism: {},",
        opt_rstr(e.determinism.as_deref())
    );
    let _ = writeln!(out, "    }},");
}

/// `Option<EnergyEntry>` as Rust source. `None` stays `None` — an
/// unmeasured model must never reach the binary as a zero-Wh claim.
/// The f64 rides `{v:?}` for round-trippable precision (the [`opt_f64`]
/// discipline).
fn energy_expr(e: Option<&EnergyEntry>) -> String {
    match e {
        Some(e) => format!(
            "Some(crate::types::model::ModelEnergy {{ wh_per_mtok_out: {:?}, \
             provenance: {}, scope: {}, source: {}, measured_at: {} }})",
            e.wh_per_mtok_out,
            rstr(&e.provenance),
            rstr(&e.scope),
            rstr(&e.source),
            rstr(&e.measured_at),
        ),
        None => "None".to_string(),
    }
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
            max_output_tokens: None,
            context_window_tokens: None,
            open_weights: None,
            status: None,
            energy: None,
            tokenizer: None,
            determinism: None,
        }
    }

    fn fixture_energy() -> EnergyEntry {
        EnergyEntry {
            wh_per_mtok_out: 42.0,
            provenance: "independent-measured".to_string(),
            scope: "gpu".to_string(),
            source: "ml.energy v3.0 · B200 · vLLM 0.11.1".to_string(),
            measured_at: "2025-12".to_string(),
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

    /// Mutation sweep 2026-08-18: three `||`→`&&` mutants in
    /// [`is_iso_date`] survived because every refused probe broke TWO
    /// clauses at once (`2026/12/31` — both separators). Each probe here
    /// breaks exactly ONE clause, so a conjunction cannot fake the
    /// disjunction: one wrong separator · one non-digit segment · one
    /// out-of-range field.
    #[test]
    fn is_iso_date_refuses_on_a_single_broken_clause() {
        // exactly one separator wrong
        assert!(!is_iso_date("2026/12-31"));
        assert!(!is_iso_date("2026-12/31"));
        // exactly one non-digit segment
        assert!(!is_iso_date("2O26-12-31"));
        assert!(!is_iso_date("2026-1x-31"));
        assert!(!is_iso_date("2026-12-3x"));
        // a sign is not a digit, yet `u8::from_str` ACCEPTS a leading `+`
        // — the probe that separates the digit check from the range check
        // (a mutant that only refuses when BOTH segments are non-digit
        // lets `+1` through the parse and into range).
        assert!(!is_iso_date("2026-+1-05"));
        assert!(!is_iso_date("2026-01-+5"));
        // exactly one field out of range
        assert!(!is_iso_date("2026-13-01"));
        assert!(!is_iso_date("2026-00-01"));
        assert!(!is_iso_date("2026-01-32"));
        assert!(!is_iso_date("2026-01-00"));
    }

    /// Same sweep, [`is_iso_month`]: one non-digit segment at a time.
    #[test]
    fn is_iso_month_refuses_on_a_single_broken_clause() {
        assert!(!is_iso_month("2O25-12"));
        assert!(!is_iso_month("2025-1x"));
        assert!(!is_iso_month("2025-+1"));
        assert!(!is_iso_month("2025/12"));
        assert!(!is_iso_month("2025-13"));
        assert!(!is_iso_month("2025-00"));
        assert!(is_iso_month("2025-01"));
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

    // ── @1.2 upstream facts ────────────────────────────────────────

    #[test]
    fn generate_emits_the_four_upstream_facts() {
        let mut e = fixture_entry();
        e.max_output_tokens = Some(64_000);
        e.context_window_tokens = Some(200_000);
        e.open_weights = Some(true);
        e.status = Some("deprecated".to_string());
        let s = generate_pricing_rs(&fixture_meta(), &[e]);
        assert!(s.contains("max_output_tokens: Some(64000)"), "got: {s}");
        assert!(
            s.contains("context_window_tokens: Some(200000)"),
            "got: {s}"
        );
        assert!(s.contains("open_weights: Some(true)"), "got: {s}");
        assert!(s.contains("status: Some(\"deprecated\")"), "got: {s}");
    }

    #[test]
    fn absent_upstream_facts_emit_none_never_zero() {
        // The inversion guard: a row upstream is silent about must reach
        // Rust as None. `Some(0)` / `Some(false)` here would report the
        // least-known model as the most-bounded (and the cheapest).
        let s = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        assert!(s.contains("max_output_tokens: None"), "got: {s}");
        assert!(s.contains("context_window_tokens: None"), "got: {s}");
        assert!(s.contains("open_weights: None"), "got: {s}");
        assert!(s.contains("status: None"), "got: {s}");
        assert!(!s.contains("max_output_tokens: Some(0)"));
    }

    #[test]
    fn open_weights_false_is_not_none() {
        // "upstream says closed" and "upstream said nothing" are two
        // different facts and must not collapse into one another.
        let mut e = fixture_entry();
        e.open_weights = Some(false);
        let s = generate_pricing_rs(&fixture_meta(), &[e]);
        assert!(s.contains("open_weights: Some(false)"), "got: {s}");
    }

    #[test]
    fn validate_rejects_zero_token_limit() {
        for (field, mut e) in [
            ("max_output_tokens", fixture_entry()),
            ("context_window_tokens", fixture_entry()),
        ] {
            if field == "max_output_tokens" {
                e.max_output_tokens = Some(0);
            } else {
                e.context_window_tokens = Some(0);
            }
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            let msg = format!("{err}");
            assert!(msg.contains(field), "got: {msg}");
            assert!(msg.contains("omit the field instead"), "got: {msg}");
        }
    }

    #[test]
    fn validate_accepts_output_greater_than_context() {
        // Upstream really does ship this on non-text models (10 rules in
        // the vendored 2026-07-28 snapshot — e.g. speech models whose
        // output tokens are audio). Enforcing NIKA-014 here would fail the
        // build on a truthful snapshot, so the mirror must accept it.
        let mut e = fixture_entry();
        e.max_output_tokens = Some(16_384);
        e.context_window_tokens = Some(8_192);
        validate_pricing(&[e], Path::new("/x")).unwrap();
    }

    #[test]
    fn validate_status_shape_only() {
        // A value we have never seen must pass — the vocabulary is
        // upstream's to grow, and a refresh must not fail on a new word.
        for good in ["deprecated", "beta", "sunset-2027", "ga"] {
            let mut e = fixture_entry();
            e.status = Some(good.to_string());
            validate_pricing(&[e], Path::new("/x"))
                .unwrap_or_else(|err| panic!("{good:?} must pass: {err}"));
        }
        for bad in ["", "Deprecated", "has space", "a".repeat(33).as_str()] {
            let mut e = fixture_entry();
            e.status = Some(bad.to_string());
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            assert!(format!("{err}").contains("status must be"), "{bad:?}");
        }
    }

    #[test]
    fn parse_accepts_at_1_3_with_facts() {
        let toml = br#"schema = "nika/model-pricing@1.3"

[meta]
source = "https://models.dev/api.json"
as_of = "2026-07-28"
source_sha256_16 = "0123456789abcdef"

[[rules]]
provider = "anthropic"
model_pattern = "claude-haiku-4-5"
input_per_million = 1.0
output_per_million = 5.0
max_output_tokens = 64000
context_window_tokens = 200000
open_weights = false
status = "beta"
"#;
        let f = parse_pricing_bytes(toml, Path::new("/x")).unwrap();
        let r = &f.rules[0];
        assert_eq!(r.max_output_tokens, Some(64_000));
        assert_eq!(r.context_window_tokens, Some(200_000));
        assert_eq!(r.open_weights, Some(false));
        assert_eq!(r.status.as_deref(), Some("beta"));
    }

    #[test]
    fn parse_rejects_superseded_1_2() {
        // The bump is load-bearing: under @1.2 an absent research fact
        // meant "the generator never carried it", so a stale file must
        // not be read as though absence were a fact about the model.
        let toml = br#"schema = "nika/model-pricing@1.2"

[meta]
source = "https://models.dev/api.json"
as_of = "2026-07-28"
source_sha256_16 = "0123456789abcdef"

[[rules]]
provider = "p"
model_pattern = "m"
input_per_million = 1.0
output_per_million = 2.0
"#;
        let err = parse_pricing_bytes(toml, Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("1.3"), "got: {err}");
    }

    // ── @1.3 research facts ────────────────────────────────────────

    #[test]
    fn parse_accepts_at_1_3_with_research_facts() {
        let toml = br#"schema = "nika/model-pricing@1.3"

[meta]
source = "https://models.dev/api.json"
as_of = "2026-07-28"
source_sha256_16 = "0123456789abcdef"

[[rules]]
provider = "openai"
model_pattern = "gpt-4o"
input_per_million = 2.5
output_per_million = 10.0
tokenizer = "o200k_base"
determinism = "seed"
energy = { wh_per_mtok_out = 42.0, provenance = "independent-measured", scope = "gpu", source = "ml.energy v3.0", measured_at = "2025-12" }
"#;
        let f = parse_pricing_bytes(toml, Path::new("/x")).unwrap();
        let r = &f.rules[0];
        assert_eq!(r.tokenizer.as_deref(), Some("o200k_base"));
        assert_eq!(r.determinism.as_deref(), Some("seed"));
        let e = r.energy.as_ref().expect("energy table parsed");
        assert!((e.wh_per_mtok_out - 42.0).abs() < f64::EPSILON);
        assert_eq!(e.provenance, "independent-measured");
        assert_eq!(e.scope, "gpu");
        assert_eq!(e.measured_at, "2025-12");
    }

    #[test]
    fn generate_emits_the_three_research_facts() {
        let mut e = fixture_entry();
        e.energy = Some(fixture_energy());
        e.tokenizer = Some("o200k_base".to_string());
        e.determinism = Some("seed".to_string());
        let s = generate_pricing_rs(&fixture_meta(), &[e]);
        // concat! of single-line literals (a `\`-continuation string here
        // would blind the fn-length gate's line-based literal stripping).
        let energy = concat!(
            "energy: Some(crate::types::model::ModelEnergy { ",
            "wh_per_mtok_out: 42.0, ",
            "provenance: \"independent-measured\", ",
            "scope: \"gpu\", ",
            "source: \"ml.energy v3.0 · B200 · vLLM 0.11.1\", ",
            "measured_at: \"2025-12\" })"
        );
        assert!(s.contains(energy), "got: {s}");
        assert!(s.contains("tokenizer: Some(\"o200k_base\")"), "got: {s}");
        assert!(s.contains("determinism: Some(\"seed\")"), "got: {s}");
    }

    #[test]
    fn absent_research_facts_emit_none_never_defaults() {
        // The inversion guard, third generation: a model nobody measured
        // must reach Rust as None — a Some(0.0) would report the least-
        // studied model as the greenest one on the board.
        let s = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        assert!(s.contains("energy: None"), "got: {s}");
        assert!(s.contains("tokenizer: None"), "got: {s}");
        assert!(s.contains("determinism: None"), "got: {s}");
        assert!(!s.contains("wh_per_mtok_out: 0"));
    }

    #[test]
    fn validate_rejects_nonpositive_energy() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut e = fixture_entry();
            let mut en = fixture_energy();
            en.wh_per_mtok_out = bad;
            e.energy = Some(en);
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            let msg = format!("{err}");
            assert!(msg.contains("wh_per_mtok_out"), "{bad}: got {msg}");
            assert!(msg.contains("omit the table"), "{bad}: got {msg}");
        }
    }

    #[test]
    fn validate_energy_axes_closed_sets() {
        for good in [
            "measured-local",
            "independent-measured",
            "vendor-claim",
            "independent-estimate",
        ] {
            let mut e = fixture_entry();
            let mut en = fixture_energy();
            en.provenance = good.to_string();
            e.energy = Some(en);
            validate_pricing(&[e], Path::new("/x"))
                .unwrap_or_else(|err| panic!("{good:?} must pass: {err}"));
        }
        let mut e = fixture_entry();
        let mut en = fixture_energy();
        en.provenance = "trust-me".to_string();
        e.energy = Some(en);
        let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("energy.provenance"), "{err}");

        for good in ["gpu", "device", "fleet"] {
            let mut e = fixture_entry();
            let mut en = fixture_energy();
            en.scope = good.to_string();
            e.energy = Some(en);
            validate_pricing(&[e], Path::new("/x"))
                .unwrap_or_else(|err| panic!("{good:?} must pass: {err}"));
        }
        let mut e = fixture_entry();
        let mut en = fixture_energy();
        en.scope = "datacenter".to_string();
        e.energy = Some(en);
        let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("energy.scope"), "{err}");
    }

    #[test]
    fn validate_energy_requires_source_and_month() {
        let mut e = fixture_entry();
        let mut en = fixture_energy();
        en.source = "  ".to_string();
        e.energy = Some(en);
        let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("energy.source"), "{err}");

        for bad in ["2025", "2025-13", "2025-00", "2025-1", "12-2025", "2025/12"] {
            let mut e = fixture_entry();
            let mut en = fixture_energy();
            en.measured_at = bad.to_string();
            e.energy = Some(en);
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            assert!(format!("{err}").contains("measured_at"), "{bad:?}: {err}");
        }
        assert!(is_iso_month("2025-01"));
        assert!(is_iso_month("2025-12"));
    }

    #[test]
    fn validate_tokenizer_shape_only() {
        // Dots and underscores admitted (tiktoken names); the family set
        // is the ecosystem's to grow — an unseen id must pass.
        for good in ["o200k_base", "cl100k_base", "claude-v3", "llama3.1-bpe"] {
            let mut e = fixture_entry();
            e.tokenizer = Some(good.to_string());
            validate_pricing(&[e], Path::new("/x"))
                .unwrap_or_else(|err| panic!("{good:?} must pass: {err}"));
        }
        for bad in ["", "O200K", "has space", "a".repeat(65).as_str()] {
            let mut e = fixture_entry();
            e.tokenizer = Some(bad.to_string());
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            assert!(format!("{err}").contains("tokenizer must be"), "{bad:?}");
        }
    }

    #[test]
    fn validate_determinism_closed_set() {
        for good in ["seed", "best-effort", "none"] {
            let mut e = fixture_entry();
            e.determinism = Some(good.to_string());
            validate_pricing(&[e], Path::new("/x"))
                .unwrap_or_else(|err| panic!("{good:?} must pass: {err}"));
        }
        // "unknown" is the load-bearing rejection: not-yet-researched is
        // the ABSENT field, never a fourth value.
        for bad in ["unknown", "deterministic", "Seed", ""] {
            let mut e = fixture_entry();
            e.determinism = Some(bad.to_string());
            let err = validate_pricing(&[e], Path::new("/x")).unwrap_err();
            assert!(format!("{err}").contains("determinism"), "{bad:?}");
        }
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
    fn generate_emits_the_schema_marker() {
        // F-P18 · the boot pin journals the schema the table speaks —
        // the emitted const derives from THIS crate's `PRICING_SCHEMA`
        // (the one `assert_schema` proved the TOML speaks), so the
        // marker has exactly one source of truth.
        let s = generate_pricing_rs(&fixture_meta(), &[fixture_entry()]);
        assert!(s.contains("pub(crate) const PRICING_SCHEMA: &str = \"nika/model-pricing@1.3\";"));
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
        // @1.1+ requires [meta] — a metaless file is a refresh-script bug.
        let toml = b"schema = \"nika/model-pricing@1.3\"\n\n[[rules]]\nprovider = \"p\"\nmodel_pattern = \"m\"\ninput_per_million = 1.0\noutput_per_million = 2.0\n";
        let err = parse_pricing_bytes(toml, Path::new("/x")).unwrap_err();
        assert!(format!("{err}").contains("meta"), "got: {err}");
    }
}
