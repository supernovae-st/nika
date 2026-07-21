// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the check ladder (#320) + the pricing preflight
//! (#213) — split from `check/mod.rs` at the 1500-LOC file cap; the
//! bodies moved verbatim.

/// One MODELS-rung finding — a `model:` this binary cannot run (#320).
pub(super) struct ModelFinding {
    pub(super) model: String,
    pub(super) tasks: Vec<String>,
    pub(super) why: String,
}

/// Cross `requirements.models` against the RESOLVER (the runnable
/// provider set, [`nika_providers::CANONICAL_IDS`]) — never the vendor
/// catalog, which advertises providers this binary cannot drive (the
/// azure class: cataloged, unresolvable, green until the run died).
pub(super) fn unresolvable_models(report: &nika_check::CheckReport) -> Vec<ModelFinding> {
    report
        .requirements
        .models
        .iter()
        .filter_map(|m| {
            // The ONE law, shared with the MCP lane (#320 follow-up:
            // the two machine surfaces consult the same fn beside the
            // resolver — they cannot drift apart again).
            let why = nika_providers::resolve_refusal(&m.model)?;
            Some(ModelFinding {
                model: m.model.clone(),
                tasks: m.tasks.clone(),
                why,
            })
        })
        .collect()
}

/// The rates the preflight shows BEFORE the first run: each model the
/// requirements collected (#213), priced from the vendored catalog.
/// UNKNOWN is null, never 0.00 — a missing price must look missing.
/// Rates only (USD per 1M tokens): token counts are unknowable
/// statically; the estimate with honest bounds is the next arc.
///
/// A model the resolver cannot run is NEVER priced (#320): the pricing
/// table fuzzy-matches by name, so a hallucinated id could wear a
/// CONJURED price — unpriced beats conjured, always.
///
/// `snapshot` = the vendored catalog's provenance (source · `as_of` ·
/// sha) + derived counts — the machine-readable answer to « priced
/// against WHAT, from WHEN? » (no surveyed tool ships this · 2026-07).
pub(super) fn pricing_section(
    report: &nika_check::CheckReport,
    model_findings: &[ModelFinding],
) -> serde_json::Value {
    let models: Vec<serde_json::Value> = report
        .requirements
        .models
        .iter()
        .map(|m| {
            let resolvable = !model_findings.iter().any(|f| f.model == m.model);
            let priced = resolvable
                .then(|| nika_catalog::find_pricing_for(&m.model))
                .flatten();
            serde_json::json!({
                "model": m.model,
                "input_per_million": priced.map(|p| p.input_per_million),
                "output_per_million": priced.map(|p| p.output_per_million),
            })
        })
        .collect();
    let snap = nika_catalog::pricing_snapshot();
    let rules = nika_catalog::all_pricing();
    let providers: std::collections::BTreeSet<&str> = rules.iter().map(|p| p.provider).collect();
    serde_json::json!({
        "snapshot": {
            "source": snap.source,
            "as_of": snap.as_of,
            "source_sha256_16": snap.source_sha256_16,
            // DERIVED at read time, never embedded (the born-stale law).
            "rules": rules.len(),
            "providers": providers.len(),
        },
        "models": models,
    })
}
