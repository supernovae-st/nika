// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MODELS rung of the check ladder (#320) + the pricing preflight
//! (#213) — split from `check/mod.rs` at the 1500-LOC file cap; the
//! bodies moved verbatim. The finding TYPE lives beside its renderer
//! (`nika_display::check_render` · the 15k descent).

pub(crate) use nika_display::check_render::{ModelFinding, ModelsAudit};
use nika_providers::resolve_access::{AccessRefusal, resolve_access};
use nika_types::access::{AccessPlan, AccessRejection};

/// The admission-time access decision per statically-known model
/// (D-2026-08-04-N1 · P2.5) — resolved over THIS machine's probe truth
/// (env key presence · no socket), the SAME derivation the run's
/// admission gate judges. Advisory: `clean` and the exit codes never
/// read it — the runtime gate holds the refusal authority, these rows
/// narrate it. One derivation, two renders (`check --json` ·
/// `explain`).
pub(crate) fn access_decisions(
    report: &nika_check::CheckReport,
) -> Vec<(String, Result<AccessPlan, AccessRefusal>)> {
    let judged: Vec<&str> = report
        .requirements
        .models
        .iter()
        .map(|m| m.model.as_str())
        .filter(|m| !m.contains("${{"))
        .collect();
    if judged.is_empty() {
        return Vec::new();
    }
    // P3 B6 · one channel: provider rows + harness rows (feature-on).
    let probes = nika_cli_host::probe::access_probes_with_harness();
    judged
        .into_iter()
        .map(|model| {
            let candidates =
                nika_providers::candidates_for(&probes, nika_providers::provider_of(model));
            (
                model.to_owned(),
                resolve_access(model, &candidates, None, None),
            )
        })
        .collect()
}

/// The R-2 boot-manifest access stamps (P3 B5 · the composer-computed
/// half): `access_pin` verbatim + `access_plan` — the per-model
/// admission decision as ONE compact JSON text, derived by the ONE
/// resolver ([`nika_providers::access_plan_map`]) over THIS machine's
/// probe rows (the doctor gesture: presence only, no socket). The
/// runtime journals the fields verbatim (`with_boot_access_fields` ·
/// the F-P13 composer-derives-runtime-journals posture). A model the
/// resolver refuses is absent from the plan — never a guessed row.
pub(crate) fn boot_access_fields(
    report: &nika_check::CheckReport,
    access_pin: Option<&str>,
) -> Vec<(&'static str, nika_types::resource::Value)> {
    use nika_types::resource::Value as FieldValue;
    let mut fields = Vec::new();
    if let Some(pin) = access_pin {
        fields.push(("access_pin", FieldValue::String(pin.to_owned())));
    }
    let models: Vec<String> = report
        .requirements
        .models
        .iter()
        .map(|m| m.model.clone())
        .collect();
    // P3 B6 · one channel (provider + harness rows, feature-on).
    let probes = nika_cli_host::probe::access_probes_with_harness();
    let plan: serde_json::Map<String, serde_json::Value> =
        nika_providers::access_plan_map(&models, &probes, access_pin)
            .into_iter()
            .map(|(model, plan)| {
                (
                    model,
                    serde_json::json!({
                        "access": plan.access,
                        "billing": plan.billing.as_str(),
                    }),
                )
            })
            .collect();
    if !plan.is_empty() {
        fields.push((
            "access_plan",
            FieldValue::String(serde_json::Value::Object(plan).to_string()),
        ));
    }
    fields
}

/// The `check --json` rows over [`access_decisions`] — wire keys match
/// the `AccessPlan` serde shape (`chosen`/`billing` `snake_case`), plus
/// the `resolved` discriminant a machine consumer branches on.
pub(super) fn access_plan_rows(report: &nika_check::CheckReport) -> Vec<serde_json::Value> {
    access_decisions(report)
        .into_iter()
        .map(|(model, decision)| match decision {
            Ok(plan) => serde_json::json!({
                "model": model,
                "provider": plan.provider,
                "resolved": true,
                "access": plan.access,
                "chosen": plan.chosen.as_str(),
                "billing": plan.billing.as_str(),
                "pinned": plan.pinned,
                "rejected": rejection_rows(&plan.rejected),
            }),
            Err(refusal) => serde_json::json!({
                "model": model,
                "provider": refusal.provider,
                "resolved": false,
                "rejected": rejection_rows(&refusal.rejected),
            }),
        })
        .collect()
}

fn rejection_rows(rejected: &[AccessRejection]) -> Vec<serde_json::Value> {
    rejected
        .iter()
        .map(|r| {
            serde_json::json!({
                "access": r.access,
                "dimension": r.dimension.as_str(),
                "layer": r.layer.as_str(),
                "witness": r.witness,
            })
        })
        .collect()
}

/// Cross `requirements.models` against the RESOLVER (the runnable
/// provider set, [`nika_providers::CANONICAL_IDS`]) — never the vendor
/// catalog, which advertises providers this binary cannot drive (the
/// azure class: cataloged, unresolvable, green until the run died).
pub(crate) fn unresolvable_models(
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
        // the typo. Advisory beside the green line, never a finding —
        // and spoken ONCE (the block rode in twice until 2026-08-05,
        // doubling every warning row).
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
