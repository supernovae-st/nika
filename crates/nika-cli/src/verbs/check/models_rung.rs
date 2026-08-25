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

/// The `infer.thinking` judgments the parse cannot make: the parser
/// validates each field's TYPE only — the two laws below are cross-field
/// (budget vs cap) and cross-fact (the seat's reasoning capability), and
/// the dispatch copies `budget_tokens` verbatim into the provider call —
/// nothing downstream re-judges either. Both were false greens: ✔ check,
/// then the refusal (or the silently dropped budget) at run.
///
/// - **the budget lives UNDER the cap** — with `budget_tokens` and
///   `max_tokens` both declared, the budget must be smaller: the
///   reasoning share lives inside `max_tokens`, so a budget ≥ the cap
///   leaves no room for the visible answer and the provider refuses the
///   call at run.
/// - **the seat can reason** — `thinking: { enabled: true }` on a model
///   the catalog KNOWS cannot reason is a dead declaration (the wire
///   400s or drops the budget). Refused only when the catalog carries
///   the model row — a model the catalog does not know is the
///   `catalog_warning` lane's advisory, never this refusal (the
///   snapshot is dated; silence beats a wrong refusal).
///
/// A templated seat (`${{ }}`) is judged THROUGH its declared default —
/// the same `static_literal_of` law as [`unresolvable_models`]; one with
/// no literal default defers to the run, making no claim.
pub(crate) fn thinking_findings(wf: &nika_schema::raw::RawWorkflow) -> Vec<ModelFinding> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let nika_schema::raw::RawAction::Infer(action) = &task.value.action else {
            continue;
        };
        let Some(thinking) = &action.thinking else {
            continue;
        };
        if !thinking.value.enabled {
            continue;
        }
        let id = task.value.id.value.as_str();
        // The budget-vs-cap compare is seat-independent: both fields are
        // literal u32s by the time the parse admits them.
        if let (Some(budget), Some(max)) =
            (thinking.value.budget_tokens, action.max_tokens.as_ref())
            && budget >= max.value
        {
            findings.push(ModelFinding::new(
                seat_label(wf, action),
                vec![id.to_owned()],
                format!(
                    "`thinking.budget_tokens` ({budget}) on `{id}` must stay UNDER \
                     `max_tokens` ({}) — the reasoning share lives inside the cap, so a \
                     budget ≥ the cap leaves no room for the answer and the provider \
                     refuses the call at run",
                    max.value
                ),
            ));
        }
        // The reasoning-seat law needs a statically known seat.
        let Some(seat) = action.model.as_ref().or(wf.model.as_ref()) else {
            continue;
        };
        let judged = if seat.value.contains("${{") {
            let Some(literal) = nika_check::static_literal_of(wf, &seat.value)
                .and_then(|v| v.as_str().map(str::to_owned))
            else {
                continue; // no literal default — the run judges
            };
            literal
        } else {
            seat.value.clone()
        };
        let Some((provider, name)) = judged.split_once('/') else {
            continue; // no provider prefix — the resolver's refusal owns that class
        };
        // The refusal needs the catalog to POSITIVELY know this exact
        // seat — the two lanes the run itself trusts (the
        // `catalog_warning` precedent): a provider row (the binary's own
        // nicknames + wire ids) or an EXACT pricing pattern. A fuzzy
        // pricing match never counts: it exists to absorb dated variants
        // for a cost estimate, not to license a refusal — and a model
        // the catalog has never heard of keeps the defaults
        // (`reasoning = false` = "no evidence it reasons", NOT "it
        // cannot"), so without this gate every newly shipped model would
        // refuse a legal `thinking:`.
        let known = nika_catalog::find_provider(provider)
            .is_some_and(|row| row.models.iter().any(|m| m.id == name || m.model == name))
            || nika_catalog::all_pricing().iter().any(|p| {
                p.provider.eq_ignore_ascii_case(provider)
                    && (p.model_pattern == name || p.model_pattern == judged)
            });
        if known && !nika_catalog::model_capabilities(provider, name).reasoning {
            findings.push(ModelFinding::new(
                judged.clone(),
                vec![id.to_owned()],
                format!(
                    "`thinking: {{ enabled: true }}` on `{id}` seats `{judged}` — the \
                     catalog knows this model cannot reason, so the declaration is dead \
                     at the wire (the budget is dropped or the call refused); remove \
                     `thinking:` or seat a reasoning-capable model"
                ),
            ));
        }
    }
    findings
}

/// The seat label a thinking finding prints — the task's `model:` or the
/// envelope default, `-` when neither reaches the task.
fn seat_label(
    wf: &nika_schema::raw::RawWorkflow,
    action: &nika_schema::raw::RawInferAction,
) -> String {
    action
        .model
        .as_ref()
        .or(wf.model.as_ref())
        .map_or_else(|| "-".to_owned(), |s| s.value.clone())
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
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn findings_of(yaml: &str) -> Vec<ModelFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        thinking_findings(&wf)
    }

    fn infer_wf(model: &str, max_tokens: &str, thinking: &str) -> String {
        format!(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: {model}\n      \
             max_tokens: {max_tokens}\n      thinking: {thinking}\n"
        )
    }

    /// The budget-vs-cap law: the parse typed both fields as u32 and
    /// stopped there; the dispatch copies the budget verbatim, so a
    /// budget ≥ the cap audited green and the provider refused the call
    /// at run. The finding fires at check, before a token is spent.
    #[test]
    fn a_budget_at_or_over_the_cap_is_a_finding() {
        for (budget, max) in [("100", "100"), ("101", "100")] {
            let f = findings_of(&infer_wf(
                "mock/echo",
                max,
                &format!("{{ enabled: true, budget_tokens: {budget} }}"),
            ));
            assert_eq!(f.len(), 1, "budget {budget} vs cap {max}: {f:?}");
            assert_eq!(f[0].tasks, vec!["t"], "{f:?}");
            assert!(
                f[0].why.contains("budget_tokens") && f[0].why.contains("max_tokens"),
                "the finding names both fields: {}",
                f[0].why
            );
        }

        // Control pair: a budget UNDER the cap makes no finding (a
        // compare that always fires is the same defect, mirrored).
        let ok = findings_of(&infer_wf(
            "mock/echo",
            "100",
            "{ enabled: true, budget_tokens: 50 }",
        ));
        assert!(ok.is_empty(), "the legal twin stays clean: {ok:?}");
    }

    /// The compare needs BOTH literals: a budget without a declared cap
    /// (the provider default governs) and a cap without a budget (the
    /// provider's own thinking bound governs) make no claim.
    #[test]
    fn the_compare_needs_both_sides() {
        let no_cap = findings_of(
            "nika: w\ntasks:\n  t:\n    infer:\n      prompt: hi\n      model: mock/echo\n      \
             thinking: { enabled: true, budget_tokens: 5000 }\n",
        );
        assert!(no_cap.is_empty(), "no declared cap, no compare: {no_cap:?}");
        let no_budget = findings_of(&infer_wf("mock/echo", "100", "{ enabled: true }"));
        assert!(
            no_budget.is_empty(),
            "no declared budget, no compare: {no_budget:?}"
        );
        let disabled = findings_of(&infer_wf(
            "mock/echo",
            "100",
            "{ enabled: false, budget_tokens: 100 }",
        ));
        assert!(
            disabled.is_empty(),
            "enabled: false declares no thinking: {disabled:?}"
        );
    }

    /// The reasoning-seat law: `enabled: true` on a model the catalog
    /// KNOWS cannot reason (gpt-4o-mini is a catalog row with
    /// `reasoning = false`) is a dead declaration — refused at check.
    #[test]
    fn thinking_on_a_known_non_reasoning_seat_is_a_finding() {
        let f = findings_of(&infer_wf(
            "\"openai/gpt-4o-mini\"",
            "500",
            "{ enabled: true, budget_tokens: 100 }",
        ));
        assert_eq!(f.len(), 1, "a dead declaration is a finding: {f:?}");
        assert_eq!(f[0].model, "openai/gpt-4o-mini", "{f:?}");
        assert!(f[0].why.contains("reason"), "{}", f[0].why);

        // Controls — every neighbor the refusal must NOT eat:
        // a reasoning seat (o3) · a seat the catalog does not carry
        // (the catalog_warning lane's advisory, never this refusal) · a
        // templated seat with no literal default (the run judges it).
        for yaml in [
            infer_wf(
                "\"openai/o3\"",
                "500",
                "{ enabled: true, budget_tokens: 100 }",
            ),
            infer_wf(
                "\"mock/echo\"",
                "500",
                "{ enabled: true, budget_tokens: 100 }",
            ),
            "nika: w\ninputs:\n  seat: { type: string, required: true }\ntasks:\n  t:\n    \
             infer:\n      prompt: hi\n      model: \"${{ inputs.seat }}\"\n      \
             max_tokens: 500\n      thinking: { enabled: true, budget_tokens: 100 }\n"
                .to_owned(),
        ] {
            let f = findings_of(&yaml);
            assert!(f.is_empty(), "no refusal here: {f:?}\n{yaml}");
        }
    }

    /// A templated seat is judged THROUGH its declared default — the
    /// rung's via-default law applied to the reasoning fact: a
    /// `${{ const.seat }}` whose default cannot reason is the same dead
    /// declaration, named as the default that was judged.
    #[test]
    fn a_templated_seat_is_judged_through_its_declared_default() {
        let f = findings_of(
            "nika: w\nconst:\n  seat: \"openai/gpt-4o-mini\"\ntasks:\n  t:\n    infer:\n      \
             prompt: hi\n      model: \"${{ const.seat }}\"\n      max_tokens: 500\n      \
             thinking: { enabled: true, budget_tokens: 100 }\n",
        );
        assert_eq!(f.len(), 1, "the declared default is judged: {f:?}");
        assert_eq!(f[0].model, "openai/gpt-4o-mini", "{f:?}");
    }

    /// `agent:` carries no `thinking:` field and `exec`/`invoke` no seat
    /// law — the judgment is infer-scoped by construction.
    #[test]
    fn non_infer_tasks_make_no_thinking_claim() {
        let f = findings_of("nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"ok\"] }\n");
        assert!(f.is_empty(), "{f:?}");
    }

    /// The wiring pin: the judgment must reach the VERDICT — a finding
    /// computed but never folded into `clean` is the false-green class
    /// this arc exists to close. Drives the real `check` verb end to
    /// end; deleting the `findings.extend(thinking_findings(..))` fold
    /// turns this red while every judgment test above stays green.
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
}
