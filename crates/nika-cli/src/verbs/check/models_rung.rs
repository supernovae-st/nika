// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the check ladder (#320) + the pricing preflight
//! (#213) — split from `check/mod.rs` at the 1500-LOC file cap; the
//! bodies moved verbatim. The finding TYPE lives beside its renderer
//! (`nika_display::check_render` · the 15k descent).

pub(crate) use nika_display::check_render::{ModelFinding, ModelsAudit};

/// Cross `requirements.models` against the RESOLVER (the runnable
/// provider set, [`nika_providers::CANONICAL_IDS`]) — never the vendor
/// catalog, which advertises providers this binary cannot drive (the
/// azure class: cataloged, unresolvable, green until the run died).
pub(super) fn unresolvable_models(
    report: &nika_check::CheckReport,
    wf: &nika_schema::raw::RawWorkflow,
) -> ModelsAudit {
    let mut audit = ModelsAudit::new(Vec::new(), 0, 0);
    for m in &report.requirements.models {
        // A TEMPLATED `model:` is not a static fact — its value arrives
        // at run time (`--var`) — but its DECLARED DEFAULT is: a bare
        // `${{ <authority>.<name> }}` whose declaration carries a
        // literal string is judged AS that default, through the ONE
        // shared resolver ([`nika_check::static_literal_of`] — the same
        // fn the cost lane counts `for_each` fan-outs with; a third
        // private copy is how lanes drift). This keeps the rung's teeth
        // on the parameterization pattern the spec recommends BY NAME
        // (08 §H8 · measured 2026-07-29: the fix before this one skipped
        // `${{ const.model }}` wholesale, and the fix before THAT
        // refused it as « a bare model id » on the spec's own fixture,
        // `stdlib/providers/005-valid-parameterized-model`). Anything
        // the resolver cannot answer gets NO claim — skipped, never
        // wrong — and is COUNTED, so the headline says so.
        let (judged, via_default) = if m.model.contains("${{") {
            let Some(default_model) =
                nika_check::static_literal_of(wf, &m.model).and_then(serde_json::Value::as_str)
            else {
                audit.unjudged += 1;
                continue;
            };
            (default_model, true)
        } else {
            (m.model.as_str(), false)
        };
        // The ONE law, shared with the MCP lane (#320 follow-up: the two
        // machine surfaces consult the same fn beside the resolver —
        // they cannot drift apart again).
        if let Some(why) = nika_providers::resolve_refusal(judged) {
            audit.findings.push(ModelFinding::new(
                m.model.clone(),
                m.tasks.clone(),
                // A via-default refusal names BOTH halves: the template
                // the author wrote and the default that was judged.
                if via_default {
                    format!("declared default `{judged}` — {why}")
                } else {
                    why
                },
            ));
        } else {
            // B-5's sibling: a resolvable model on a server-backed
            // keyless engine earns the green line's liveness nuance —
            // this rung never dialed the server it names.
            if judged
                .split_once('/')
                .is_some_and(|(provider, _)| nika_providers::server_backed_local(provider))
            {
                audit.local_server += 1;
            }
            if via_default {
                audit.via_default += 1;
            }
        }
        // The sister law, same home (audit UX 2026-07-31): a model that
        // RESOLVES but matches nothing the snapshot prices for its
        // provider warned nobody — the user bought the key, then met
        // the typo. Advisory beside the green line, never a finding.
        if let Some(why) = nika_providers::catalog_warning(judged) {
            audit
                .catalog_warnings
                .push(ModelFinding::new(m.model.clone(), m.tasks.clone(), why));
        }
        // The sister law, same home (audit UX 2026-07-31): a model that
        // RESOLVES but matches nothing the snapshot prices for its
        // provider warned nobody — the user bought the key, then met
        // the typo. Advisory beside the green line, never a finding.
        if let Some(why) = nika_providers::catalog_warning(judged) {
            audit
                .catalog_warnings
                .push(ModelFinding::new(m.model.clone(), m.tasks.clone(), why));
        }
    }
    audit
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
pub(crate) fn pricing_section(
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
