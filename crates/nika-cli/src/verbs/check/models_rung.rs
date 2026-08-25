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
        if let Some(refusal) = nika_providers::resolve_refusal(judged) {
            // A via-default refusal names BOTH halves: the template
            // the author wrote and the default that was judged.
            let why = if via_default {
                format!("declared default `{judged}` — {}", refusal.why)
            } else {
                refusal.why
            };
            let mut finding = ModelFinding::new(m.model.clone(), m.tasks.clone(), why);
            if let Some(code) = refusal.code {
                finding = finding.with_code(code);
            }
            audit.findings.push(finding);
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

/// The `infer.thinking` judgments, folded into this rung's findings.
/// The judge is [`nika_check::thinking_findings`] (nika-check · the laws
/// and their scope live with it): the parse validates each field's TYPE
/// only, so the cross-field (budget vs cap) and cross-fact (the seat's
/// reasoning capability) laws descend to the check crate — a judgment
/// the MCP/machine surfaces can reach without re-deriving it here. The
/// lane lived in this file until the 15k crate wall moved it
/// (2026-08-25); the fold sites (`check` verdict · slot-only gate ·
/// dry-run swap) are unchanged.
pub(crate) fn thinking_findings(wf: &nika_schema::raw::RawWorkflow) -> Vec<ModelFinding> {
    nika_check::thinking_findings(wf)
        .into_iter()
        .map(|f| ModelFinding::new(f.model, vec![f.task], f.why))
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

#[cfg(test)]
mod tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn infer_wf(model: &str, max_tokens: &str, thinking: &str) -> String {
        format!(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: {model}\n      \
             max_tokens: {max_tokens}\n      thinking: {thinking}\n"
        )
    }

    /// The wiring pin: the judgment must reach the VERDICT — a finding
    /// computed but never folded into `clean` is the false-green class
    /// this arc exists to close. Drives the real `check` verb end to
    /// end; deleting the `findings.extend(thinking_findings(..))` fold
    /// turns this red while the judgment's own tests (nika-check's
    /// `thinking` module) stay green.
    #[test]
    fn a_thinking_finding_turns_the_check_red() {
        let dir =
            std::env::temp_dir().join(format!("nika-cli-thinking-rung-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let theme = crate::Theme::new(false, true, false);

        let bad = dir.join("thinking-budget-at-cap.nika.yaml");
        std::fs::write(
            &bad,
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 100 }"),
        )
        .expect("fixture");
        let out = crate::verbs::check::run(bad.to_str().expect("utf8"), false, false, None, theme);
        assert_eq!(
            out.code, 2,
            "the judgment reaches the verdict: {}",
            out.text
        );
        assert!(
            out.text.contains("MODELS") && out.text.contains("budget_tokens"),
            "the finding row renders under the rung: {}",
            out.text
        );

        // Control: the legal twin stays green through the same verb.
        let ok_path = dir.join("thinking-budget-under-cap.nika.yaml");
        std::fs::write(
            &ok_path,
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 50 }"),
        )
        .expect("fixture");
        let ok =
            crate::verbs::check::run(ok_path.to_str().expect("utf8"), false, false, None, theme);
        assert_eq!(ok.code, 0, "the legal twin stays green: {}", ok.text);
    }

    /// The wrapper maps the check crate's rows into this rung's finding
    /// shape — the model seat, the ONE task, the why, and never a
    /// conjured spec code.
    #[test]
    fn the_wrapper_maps_thinking_findings_into_model_findings() {
        let wf = parse(
            infer_wf("mock/echo", "100", "{ enabled: true, budget_tokens: 100 }").as_str(),
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect("fixture parses");
        let rows = super::thinking_findings(&wf);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].tasks, vec!["t"], "{rows:?}");
        assert!(rows[0].why.contains("budget_tokens"), "{rows:?}");
        assert!(rows[0].code.is_none(), "engine-local, no conjured code");
    }
}
